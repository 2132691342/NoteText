# 架构

这一篇讲 NoteText 怎么分层、怎么启动、前后端怎么说话、状态归谁。

## 分层

```
                    ┌─────────────────────────────┐
   浏览器 / WebView2  │  frontend/src              │
                    │  views / components / stores │
                    │  composables / api          │
                    └──────────────┬──────────────┘
                                   │ invoke / emit（Tauri IPC）
                    ┌──────────────▼──────────────┐
                    │  commands（薄 handler）      │  ← 唯一 import tauri 的层
                    └──────────────┬──────────────┘
                                   │
                    ┌──────────────▼──────────────┐
                    │  service（业务编排）         │  ← 不知道窗口与托盘的存在
                    └──────────────┬──────────────┘
                                   │
                    ┌──────────────▼──────────────┐
                    │  repo（持久层，rusqlite）    │
                    └──────────────┬──────────────┘
                                   │
                    ┌──────────────▼──────────────┐
                    │  domain（域模型）            │
                    └──────────────┬──────────────┘
                                   │
                    ┌──────────────▼──────────────┐
                    │  pkg（叶子工具包）           │  ← 谁都能依赖，自己不依赖业务包
                    └─────────────────────────────┘
```

| 层 | 允许依赖 | 禁止依赖 |
|---|---|---|
| commands | service / domain / pkg / app / shell / tauri | repo / db |
| service | repo / domain / pkg / app | commands / tauri |
| repo | domain / pkg / rusqlite | commands / service / app |
| domain | pkg | 任何上层 |
| app | domain / repo / pkg / tauri（`Manager` trait） | commands |
| shell | domain / pkg / tauri | commands / service |
| platform | pkg / tauri | commands / service |
| pkg | 标准库 / 第三方工具 | 任何其他 backend 业务包 |

**`service` 不知道窗口的存在**是这套分层最要紧的一条：它让业务逻辑能在
没有 WebView 的测试里跑（`cargo test` 编译 lib 而不拉起窗口）。
需要弹对话框 / 发事件时，由 `commands` 层做。

## 启动时序

```
main.rs
  └─ notetext_lib::run()
       ├─ app::paths::resolve()          %APPDATA%\NoteText（或 NOTETEXT_HOME）
       ├─ pkg::log::init(&paths)         app.log + warn.log
       ├─ app::bootstrap::build(&paths)
       │    ├─ Db::open(&paths.db_path)  WAL + busy_timeout + migrate
       │    ├─ SettingsRepo::load        没有则写默认配置
       │    └─ Services::new(db)         file / editor / tools / userdata
       ├─ tauri::Builder
       │    ├─ plugin(single_instance)   二次启动转交
       │    ├─ plugin(dialog)            原生打开 / 保存对话框
       │    ├─ plugin(opener)            用默认程序打开路径
       │    ├─ manage(AppState)
       │    ├─ invoke_handler(command_handlers!())
       │    └─ setup:
       │         ├─ platform::set_app_handle
       │         ├─ shell::tray::install
       │         ├─ app::lifecycle::on_ready   广播 app:ready + 记窗口尺寸
       │         └─ shell::close_policy::sync_from_config
       ├─ on_window_event(CloseRequested)  关闭到托盘 / 放行
       └─ app.run(RunEvent::Exit → on_exit)
```

**失败模式**：

| 失败点 | 行为 |
|---|---|
| 路径解析不出来 | 原生 MessageBox（WebView 还没起来，这是唯一出口）+ exit 1 |
| 数据库打不开 | fail-fast。书签 / 片段 / 宏 / 会话恢复都依赖它，半残启动更误导 |
| 日志 / 配置目录不可用 | 记一条 warn 后继续，界面仍可用 |
| 缺 WebView2 Runtime | `app.run` 返回 Err → 原生 MessageBox + exit 1 |

## 前后端怎么说话

### invoke（前端 → 后端）

```ts
// frontend/src/api/index.ts
const c = await invoke<FileContentVO>('read_file', { path, mode })
```

- **没有 HTTP 服务、没有端口、没有 SSE**：桌面应用常驻一个 axum 服务只为了
  「浏览器里也能调试」，代价是多一层服务与端口管理，不值；
- 出入参一律 `domain` 的类型，字段名 snake_case，`types/api.ts` 与 Rust 的
  json tag 一一对应；
- 错误由 `api/index.ts` 的 `normalizeError` 归一成 `{ code, message, details }`；
- **二进制不经 `Vec<u8>`**：它会序列化成 `number[]`，几 MB 就把通道堵死。
  图片 / PDF / docx / Hex 走 `read_binary`，出参是 base64（体积 +33% 而不是 +300%），
  且读取带 20MB 上限——超限给出「请用系统程序打开」这类可读提示。

### emit（后端 → 前端）

| 事件 | 载荷 | 消费方 |
|---|---|---|
| `app:ready` | 无 | 前端确认后端就绪 |
| `app:before-close` | 无 | `useQuitGuard`：存会话 → 弹确认 → 置放行位 |
| `app:open-file` | 文件路径 | `useShellIntent`：切回编辑器页并打开 |
| `app:new-document` | 无 | 托盘「新建文档」 |
| `fs:changed` | 路径数组 | `useExternalChange` + 文件树刷新 |

事件不缓冲给后注册的监听器（Tauri 的 `listen` 在注册后才收到），
所以「启动时带来的文件」用 `useShellIntent` 的意图队列兜底。

## 前端状态归属

**一条判据：这个状态改了，需要通知后端吗？**

| 状态 | 归属 | 理由 |
|---|---|---|
| 标签的正文 | CodeMirror 实例（`useEditorRegistry`） | 每次按键过 store 会让响应式系统 diff 整篇文档 |
| 标签的元数据（路径 / 编码 / 光标 / 脏标记） | `stores/editor` | 会话恢复要用；量小 |
| 目录树的展开状态与子项缓存 | `stores/fileTree` | 纯 UI 态，但切标签不丢所以进 store |
| 配置 | 后端 SQLite 为权威，`stores/settings` 是镜像 + 派生量 | 改一项整块回写（防抖 400ms） |
| 命令表 | `stores/commands`（启动时注册一次） | 菜单 / 工具栏 / 快捷键三入口共用 |
| 当前打开的浮窗 | `stores/dialogs` | 工具栏、菜单、快捷键都要能开，而浮窗挂在最外层 |
| 轻提示队列 | `stores/toast` | 全局层 |
| 编辑器主题 / 语言扩展 | `composables/useEditorTheme` / `useEditorLanguage` | 各自一个 compartment，与 store 解耦 |

**派生量不落库**：`resolvedTheme`（`system` → 实际深浅）、
`--nt-fs-scale`（缩放）、`--nt-vw-scale`（视口比例）都是算出来的，
只写 CSS 变量，不进配置。

## 国际化层

中文默认 + 英文，两套文案同构（584 个 key）。三种入口，职责不同：

| 入口 | 用在 | 怎么拿 |
|---|---|---|
| `useI18n()` 的 `t` | 组件模板与组件脚本 | 组件实例 |
| `tr()`（`@/i18n`） | store / utils / api / 命令表——没有组件实例的地方 | 直接 import |
| `labelKey` 字段 | 命令表（菜单 / 工具栏 / 快捷键设置共用一份） | 渲染时 `t(item.labelKey)` |

**后端不参与国际化**：Rust 侧没有「当前界面语言」这个概念（窗口属于前端），
所以 `AppError` 的 message 固定写中文。英文界面靠前端按错误码替换：

```
后端 { code: "1001", message: "文件读写失败", details: "os error 32" }
  → 前端 normalizeError() 查 errCode.1001
  → 中文界面："文件读写失败"（后端原句）
  → 英文界面："File read/write failed"（前端文案）
  → details 始终保留："os error 32"
```

为什么不让后端直接出英文：它得先知道界面语言，而那意味着每次切语言
都要多一次 invoke 告诉它，而且错误发生在「切语言」之前的请求上就翻不回去。

**编码名与语言名也归前端**：后端只给 `id`，显示名查
`locales` 的 `encodingLabel` / `language.<id>`。
Rust 侧 `EncodingInfo.label` 只当命令行工具的兜底。

加载器比后端 id 多一个 `tsx`、少 `text`/`log`/`csv`：后两个不挂语法包，
`.tsx` 在后端归到 `typescript`。

Element Plus 的文案也跟着切：`main.ts` 注入 `computed<Language>`，
`App.vue` 在 `setup` 顶层 `inject` 后交给 `ElConfigProvider`。
`inject()` 必须同步调用——放进 `computed` getter 会在每次求值时报
「inject() can only be used inside setup()」。

## 权限模型

`src-tauri/capabilities/default.json` 是前端唯一被允许调用的能力面：
`core:default` + 一组窗口控制 + `dialog` + `opener` + `event` + `tray`。

**文件读写全部走后端 command，前端没有 `fs` 权限**：路径校验、大小上限、
原子写都在一个地方把关。二进制（图片 / PDF / Hex）也走 command
（`read_binary` → base64）：给渲染层挂 fs 插件等于把整块磁盘的读权限交出去，
而这里只需要「读这一个文件」。

## 二进制为什么是 lib + bin 两个目标

```
src-tauri/src/lib.rs   crate root（run() + 全部模块）
src-tauri/src/main.rs  三行：调 notetext_lib::run()
```

分开是为了让集成测试（`cargo test`）能链接同一份装配而不必拉起 WebView。
测试按域分文件（`src-tauri/tests/<domain>_chain.rs`），一个测试一种坏法，
公共设施在 `common/mod.rs`。每个测试二进制各自编译一次 `common`——所以
`common` 里允许 `#![allow(dead_code)]`：不是每个域都用得上全部 helper。
**注意 `main.rs` 里不能写 `mod lib;`**——那会把 lib.rs 变成 main 的子模块
`lib`，它的 `pub mod app;` 就会去找 `src/lib/app.rs`，报一堆「file not found」。

## 取舍

| 选择 | 优势 | 代价 |
|---|---|---|
| 拆 lib + bin 两个目标 | 集成测试链接同一份装配，不必拉起 WebView | `Cargo.toml` 要显式声明 `[lib] path` |
| `commands / service / repo / domain` 四层严格单向依赖 | 每层能独立测试；改动不跨层扩散 | 一个字段从数据库到界面要过四层类型，改字段必须同步四处 |
| `app / shell / platform` 三个能力域互不引用 | 独立测试与独立演进 | 它们要合作只能在 `commands` 里编排，能力域自己拿不到对方 |
| 叶子包 `pkg` 不依赖任何业务包 | 工具函数不会被业务逻辑反向污染 | 出现业务词汇就意味着要往上挪一层 |
| 前后端纯 invoke，无 HTTP 服务 | 没有端口、没有跨域、不需要 HTTP 客户端 | 不能用一个命令同时被浏览器与桌面复用 |
| 国际化在前端做，后端 message 固定中文 | 无须为「切语言」多一次 invoke，错误在切语言前发生也能正确显示 | 新错误码要在 `pkg/error.rs` 与两份 locale 同时加 |
