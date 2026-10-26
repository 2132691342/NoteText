# 00 · 总架构

## 定位

这一篇回答四个问题：**NoteText 是什么、由哪些模块组成、数据怎么流、每个决定换来了什么**。
看完它再进各子系统规格（01–18），不需要先读代码。

## 一句话

NoteText 是一款轻量级 Windows 桌面文本编辑器。内核是 CodeMirror 6 + Rust 文件域，
项目只做四件事：把编辑器的每一类操作接顺、把开发者高频的文本处理工具内置、
把「关窗再开回到原样」这件事做扎实、把常用压缩格式的打包解包内置进来。

```
编辑器负责「改文本」，NoteText 负责让「打开 / 保存 / 查找 / 转换 / 恢复 / 打包」都不打断你。
```

第四条是独立子系统：15 种格式、任务队列、增删 / 转换 / 分卷、路径穿越与密码三态
（见 [18](18-archive.md)）；系统文件关联见 [17](17-deployment.md)。

**界面只做「用户每天都做的那一件事」**：不自研插件系统、不做多窗口、不做云同步。
这条边界让功能面收得住，代价是高级需求要靠外部工具。

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
**能力域之间（app / shell / platform）不互引**：要合作就在 `commands` 里编排，
一互引，独立测试与独立演进同时作废。

### 模块地图

| 模块 | 负责什么 | 规格 |
|---|---|---|
| `src-tauri/src/pkg/paths.rs` + `app/paths.rs` | 落盘位置的解析与进程内唯一实例 | [01](01-runtime-and-paths.md) |
| `src-tauri/src/db` | SQLite 打开 + 迁移（WAL） | [02](02-database.md) |
| `src-tauri/src/repo` | 一张表一个 repo（rusqlite） | [02](02-database.md) / [08](08-user-data.md) |
| `domain/config.rs` + `stores/settings` | 配置的权威与镜像 | [03](03-config.md) |
| `src-tauri/src/shell` + `platform` | 单实例、托盘、关闭策略、窗口控制 | [04](04-desktop-shell.md) |
| `service/file.rs` + `service/encoding.rs` | 读 / 写 / 目录树 / 编码探测 / 原子写 / 监控 | [05](05-file-domain.md) |
| `service/file_ops.rs` | 文件树操作（护栏统一在 `validate_target`） | [05](05-file-domain.md) |
| `service/editor.rs` + `frontend/components/editor` | 语言清单、注释切换、行编辑、CodeMirror 容器 | [06](06-editor-domain.md) |
| `frontend/src/composables` + `stores` | 命令表 / 按键分发 / 状态归属 | [07](07-frontend-state.md) |
| `frontend/src/components/*` | 布局五层、面板、浮窗、设置页的组件地图 | [16](16-page-structure.md) |
| `service/userdata.rs` | 书签 / 片段 / 宏 / 草稿 / 最近 / 备份 | [08](08-user-data.md) |
| `service/tools.rs` | JSON / Diff / 哈希 / 正则 / 迷你正则引擎 | [09](09-tools.md) |
| `service/session.rs` + `repo/session.rs` | 会话保存与恢复 | [10](10-session-restore.md) |
| `themes.css` + `ep-theme.css` | 视觉令牌与主题 | [11](11-visual-tokens.md) |
| `tests/chain_tests.rs` + `scripts/` | 测试组织与验证入口 | [12](12-testing.md) |
| `service/batch.rs` | 目录级查找替换 / 批量重命名（两段式） | [13](13-batch-operations.md) |
| `service/archive/` | 压缩 / 解压引擎、任务队列、格式能力表 | [18](18-archive.md) |
| `commands/` ↔ `api/index.ts` | invoke 命令契约 | [14](14-api-contract.md) |
| `pkg/error.rs` | 错误码分段 | [15](15-error-codes.md) |
| `tauri.conf.json` + workflows | 构建与分发 | [17](17-deployment.md) |

## 启动时序

```
main.rs
  └─ notetext_lib::run()
       ├─ app::paths::resolve()          %APPDATA%\NoteText（或 NOTETEXT_HOME）
       ├─ pkg::log::init(&paths)         app.log + warn.log
       ├─ app::bootstrap::build(&paths)
       │    ├─ Db::open(&paths.db_path)  WAL + busy_timeout + migrate
       │    ├─ SettingsRepo::load        没有则写默认配置
       │    └─ Services::new(db)         file / file_ops / session / tools / userdata / batch / archive
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

## 两个真相源

| 根 | 谁写 | 内容 |
|---|---|---|
| 用户磁盘上的文件 | **用户 / 其他程序** | 正文、编码、行尾、mtime |
| `%APPDATA%\NoteText\` | **NoteText** | SQLite（书签 / 片段 / 宏 / 草稿 / 最近 / 配置）、日志、备份 |

**不能有第三本账**。文件内容是磁盘的，NoteText 只读它、按用户指令写它，
不在数据库里存一份正文副本（草稿快照除外，那是进程猝死的兜底）。
配置是数据库的，界面只读镜像 + 派生量。「再存一份」的代价是两处需要失效，
任何一处漏失效都会让界面与磁盘对不上——表现为「明明保存了，重开却是旧内容」。

## 一次保存的链路

```
用户按 Ctrl+S
  → useGlobalKeys 命中 Control+S → commands.run('file.save')
  → useCommands 取 getText(tabId)（CodeMirror 的 EditorState）
  → invoke('write_file', { path, text, encoding, line_ending, bom })
  → commands::file::write_file（薄，转调）
  → service::file::write
       ├─ 行尾在文本层归一（多字节编码里 CR/LF 序列不能按字节判）
       ├─ encoding::encode 按目标编码编码
       ├─ 补 BOM（用户显式选了才补）
       ├─ atomic_write：同目录临时文件 + rename
       └─ RecentRepo::touch 记账
  → 返回 WriteResultVO
  → store markDirty(false)、ToastHost 提示、状态栏圆点转空心
```

## 前后端怎么说话

### invoke（前端 → 后端）

```ts
// frontend/src/api/index.ts
const c = await invoke<FileContentVO>('read_file', { path, mode })
```

- **没有 HTTP 服务、没有端口**：桌面应用常驻一个 HTTP 服务只为
  「浏览器里也能调试」，代价是多一层服务与端口管理，不值；
- 出入参一律 `domain` 的类型，字段名 snake_case，`types/api.ts` 与 Rust 的
  json tag 一一对应；
- 错误由 `api/index.ts` 的 `normalizeError` 归一成 `{ code, message, details }`；
- **二进制不经 `Vec<u8>`**：它会序列化成 `number[]`，几 MB 就把通道堵死。
  图片 / PDF / docx / Hex 走 `read_binary`，出参是 base64（体积 +33% 而不是 +300%），
  且读取带 20MB 上限。全部命令的对照表见 [14](14-api-contract.md)。

### emit（后端 → 前端）

| 事件 | 载荷 | 消费方 |
|---|---|---|
| `app:ready` | 无 | 前端确认后端就绪 |
| `app:before-close` | 无 | `useQuitGuard`：关到托盘开 → 存布局后收托盘；否则存会话 → 弹确认 → 置放行位 |
| `app:open-file` | 文件路径 | `useShellIntent`（挂 App 根）：压缩包开浏览浮窗，其余切回编辑器页打开 |
| `app:new-document` | 无 | 托盘「新建文档」 |
| `fs:changed` | 路径数组 | `useExternalChange` + 文件树刷新 |
| `archive:task-update` | 任务快照数组 | `stores/archive`：压缩任务队列的进度 |
| `archive:task-finished` | 终态任务 | `stores/archive`：终态 toast 与结果负载 |

事件不缓冲给后注册的监听器（Tauri 的 `listen` 在注册后才收到），
所以「启动时带来的文件」不走事件：后端定格在 OnceLock，前端挂载后经
`take_launch_intent` 命令拉取一次（无竞态）。

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
全部 8 条长链路测试在 `src-tauri/tests/chain_tests.rs` 一个文件里
（组织方式见 [12](12-testing.md)）。
**注意 `main.rs` 里不能写 `mod lib;`**——那会把 lib.rs 变成 main 的子模块
`lib`，它的 `pub mod app;` 就会去找 `src/lib/app.rs`，报一堆「file not found」。

## 排障速查

| 现象 | 先看 |
|---|---|
| `cargo check` 报 `file not found for module 'app'` | `main.rs` 里写了 `mod lib;`，删掉 |
| 前端 invoke 报 `command not found` | 命令没进 `command_handlers!` 清单，或名字与 `api/index.ts` 不一致 |
| 前端类型全变 `undefined` | 后端字段名与 `types/api.ts` 不一致；查 json tag |
| 保存后整个文件行尾变了 | `write_file` 没传 `line_ending`，或前端传了 `null` |
| 界面样式全丢 | 引用了不存在的 CSS 变量——那会静默失效，最危险 |
| 深色下浮层与内容糊在一起 | `ep-theme.css` 的 `--el-overlay-color-lighter` 没按主题分开 |
| EP 按钮显示成裸文字（无底无框） | 缺 `unplugin-element-plus`：脚本里 `import { ElButton }` 的组件样式要它补（`vite.config.ts`） |
| 编辑器里 Ctrl+S / Ctrl+F 没反应 | 全局按键分发的判据应是「这条组合有没有命令认领」，不能按「正在输入」整块排除 |
| 切标签后内容 / 撤销历史丢了 | `EditorArea` 用了 `v-if`，实例被销毁。必须 `v-show` + 每个标签一个实例 |
| 保存成功但标签还是脏的 | 没调 `commitBaseline(tabId)`，脏标记基线还停在载入时 |
| 每次保存都提示「已在外部修改」 | 后端没记自写路径（`FileService::note_self_write`） |
| 测试改了用户真实数据 | 用了 `AppPaths::resolve()` 而不是 `from_root(tempdir)` |

排障第一步永远是看 `logs\app.log` 的启动横幅：它同时打印数据目录与版本号。

## 取舍

| 选择 | 优势 | 代价 |
|---|---|---|
| Tauri 2 做桌面壳 | 前端直接用 Vue 生态；包体约 5MB | 要理解 Tauri 的权限清单（capabilities）才能加新系统能力 |
| 无边框 + 自绘标题栏 | 视觉统一、标题栏能放「当前文件名」 | 要处理拖拽区、最大化态、双击标题栏 |
| 纯 invoke，不内嵌 HTTP 服务 | 无端口、无常驻服务、无 CSP 麻烦 | 浏览器里 `npm run dev` 只能看空态 |
| 拆 lib + bin 两个目标 | 集成测试链接同一份装配，不必拉起 WebView | `Cargo.toml` 要显式声明 `[lib] path` |
| 四层严格单向依赖 | 每层能独立测试；改动不跨层扩散 | 一个字段从数据库到界面要过四层类型，改字段必须同步四处 |
| 三个能力域互不引用 | 独立测试与独立演进 | 合作只能在 `commands` 里编排 |
| 叶子包 `pkg` 不依赖任何业务包 | 工具函数不会被业务逻辑反向污染 | 出现业务词汇就意味着要往上挪一层 |
| 正文归编辑器，元数据归后端 | 大文件不卡；保存 / 恢复只需元数据 | 需要 `useEditorRegistry` 这层间接 |
| rusqlite bundled | 用户机器不用装 SQLite，无 CGO | 二进制 +1.5MB |
| `encoding_rs` 码表 + 迷你正则引擎 | 12 种编码全量正确、正则行为可解释可测 | 码表约 200KB；正则不支持 `{n,m}` 与命名组 |
| 单窗口 + 多标签 | 没有「窗口跑哪去了」的问题 | 不能并排看两个文件夹 |
| 不做插件系统 | 功能面收得住，没有插件兼容性负担 | 高级需求靠外部工具 |
| 国际化在前端做，后端 message 固定中文 | 无须为「切语言」多一次 invoke，错误在切语言前发生也能正确显示 | 新错误码要在 `pkg/error.rs` 与两份 locale 同时加 |
