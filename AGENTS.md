# AGENTS.md · NoteText 工程规范

> 本文件是代码风格 / 分包 / 依赖方向 / 工作流的**唯一权威**。
> 设计说明见 `specs/`；冲突时以本文件为准。

---

## 1. 技术栈（强制锁定）

| 层 | 选型 | 版本 |
|---|---|---|
| 语言 | Rust | 1.80+（stable，单 workspace） |
| 桌面壳 | Tauri | v2.x（无边框，前端自绘标题栏） |
| 前端 | Vue 3 + TypeScript + Vite | 3.4.34 / 5.4.5 / 5.2.12 |
| 状态管理 | Pinia | ^2.1.x（Setup Store） |
| 路由 | Vue Router | ^4.3.x（hash 模式） |
| UI 组件库 | Element Plus | ^2.9.x（按需引入，变量接到 `--nt-*`） |
| 原子 CSS | UnoCSS | ^0.58.x（布局与间距；色值仍走 `--nt-*` 令牌） |
| 代码编辑器 | CodeMirror 6 | ^6.x（31 种语言识别，静态 + 动态按需加载） |
| Markdown | markdown-it · highlight.js · Mermaid · KaTeX · DOMPurify | 编辑 / 分屏 / 预览三态 |
| 格式转换 | js-yaml · @iarna/toml · fast-xml-parser | JSON / YAML / TOML / XML 互转 |
| 文档预览 | XLSX · docx-preview · WebView2 内置 PDF 阅读器 | 按扩展名分流 |
| 图表 | ECharts | ^5.5.x（按需引入，色值取 `--nt-ch-*`） |
| 国际化 | Vue I18n | ^9.10.x（zh-CN 默认 + en；设置页可切换，EP 文案同步） |
| 样式 | 原生 CSS 设计令牌 | `themes.css` + `ep-theme.css` + `base.css` + `content.css`；色值与尺寸只在 `themes.css` 定义 |
| 数据库 | SQLite via rusqlite | `bundled`（WAL；无 CGO 依赖链） |
| 文件监控 | notify | ^7.x |
| 日志 | tracing + tracing-subscriber | info / warn / error 三档，双文件落盘 |
| 测试 | testing（标准库） | 一包一入口、一个子测试一种坏法；`scripts/check.ps1` 收口全部验证 |
| 平台 | Windows | 本期只考虑 Windows；平台特定代码按 `#[cfg(windows)]` 分文件，不写 `runtime.GOOS` 大分支 |

### 1.1 依赖原则

标准库 → `golang.org/x/...` 等价物 → 生态库。新增依赖必须说明理由与替代方案。

**明确不引入**：

- **微前端（Qiankun）**：单窗口桌面编辑器没有多团队并行开发的诉求，引入只增构建复杂度；
- **图编排（AntV X6 / BPMN-JS）**：编辑器不做流程设计，引入 ~2MB 体积无对应功能；
- **Axios**：前后端纯 Tauri invoke 通信，没有 HTTP 客户端的使用场景；
- **encoding_rs**：当前只支持 12 种编码，其中 2 种（Windows-1252 / ISO-8859-1）是手写单字节表，手写表可测且可解释。
  **要扩展编码清单时优先换 encoding_rs**，别继续往表里加；
- **regex crate**：工具箱的正则测试器只需常见语法，手写引擎省 ~1MB 体积。
  要支持 `{n,m}` 量词与命名组时换 regex crate。
- **WangEditor**：富文本编辑器没有对应场景（NoteText 是代码 / 纯文本向的），
  Markdown 走 markdown-it 一条链路，不再挂第二个编辑器运行时。
- **vue-pdf-embed / pdfjs-dist**：v1 打包 1.7MB 且要自己配 worker，配错就是白屏。
  PDF 交给 WebView2 内置阅读器（`<embed type="application/pdf">` + asset 协议）：
  流式读取、支持搜索与打印，还能绕过 `read_binary` 的 20MB 上限。

**视觉分层纪律不变**：Element Plus 只是基元供给方，视觉语言仍由 `themes.css` 的
靛蓝五档令牌说了算——EP 的 CSS 变量经 `ep-theme.css` 全部接到 `--nt-*`，
组件里禁止写 EP 主题色之外的色值；图表色值同理走 `--nt-ch-*`。
新增组件优先用 EP 现成的，不回退到自绘弹层。

### 1.2 正文归编辑器，元数据归后端

**CodeMirror 的 EditorState 由组件层持有，store 只存标签的元数据**
（路径、编码、光标、脏标记）。理由：每次按键都把正文同步进 store 会让 Vue 的
响应式系统处理整篇文档的 diff，几百 KB 的日志文件直接卡死。

需要正文的地方（保存、统计、工具箱）通过 `composables/useEditorRegistry.ts`
的 `getText(tabId)` 取。**不要为了「方便」在 store 里存正文**——那是把性能问题
藏起来，等用户打开大文件时才爆。

---

## 2. 强制编码规范

### 2.1 包结构

```
NoteText/
├── Cargo.toml                  # workspace：版本收敛在这里
├── src-tauri/
│   ├── src/
│   │   ├── commands/           # ① invoke 入口（薄）+ 系统能力绑定
│   │   ├── service/            # ② 业务编排层（事务边界；不写 SQL；不引 tauri）
│   │   ├── repo/               # ③ 持久层（rusqlite；不引上层）
│   │   ├── domain/             # ④ 域模型（一个聚合根一个文件，DO/REQ/VO 同居一处）
│   │   ├── db/                 # ⑤ SQLite 打开 + 迁移（WAL）
│   │   ├── app/                # ⑥ 能力域：路径解析 / 装配 / 生命周期
│   │   ├── shell/              # ⑦ 壳层：单实例 / 托盘 / 关闭策略
│   │   ├── platform/           # ⑧ 平台相关：Win32 消息框 / AppHandle 存取
│   │   └── pkg/                # ⑨ 叶子工具包（AppError / paths / log / id / text）
│   ├── capabilities/           # Tauri 权限清单
│   └── tests/chain_tests.rs    # 集成测试（编译 lib 而不链接 WebView）
└── frontend/src/               # 前端唯一源码目录
```

**`app` 是能力域，不是胶水层**：它拥有「应用状态」这一个自洽概念
（数据目录 + 数据库 + 服务容器），有独立的生命周期（build → on_ready → on_exit）。
`commands` 通过 `tauri::State<AppState>` 拿它，不自己 new。

**禁止**：

- 在 `commands / service / repo` 包内定义实体 / REQ / VO / 枚举（必须放 `domain/`）
- 错误码 / 工具类分散到各业务包（统一 `pkg/`）
- 在 `main.rs` 内写业务代码；**也不要在 `main.rs` 里写 `mod lib;`**
  （那会把 lib.rs 变成 main 的子模块 `lib`，它的 `pub mod app;` 就会去找
  `src/lib/app.rs`，报一堆「file not found」）
- `pkg/` 内出现业务词汇（bookmark / snippet / tab 等）
- `pkg/` 依赖任何其他 backend 业务包（叶子工具包铁律）
- 用字符串拼 SQL：参数绑定是唯一写法（`params![...]`）

**新增一个能力的固定顺序**：
`domain`（DO/REQ/VO）→ `repo`（若要落库）→ `service` → `commands`（+ `command_handlers!` 清单）→
`frontend/src/api/index.ts` → `types/api.ts` → 组件 → 文档。
漏掉清单注册的后果是前端 invoke 报 `command not found`，编译期发现不了。

### 2.2 依赖方向（强制）

```
commands ──► service ──► repo ──► domain
    │            │                  ▲
    │            └──► app（能力域）──┘
    └──► shell（壳层）
全部层都可用 pkg（叶子工具包：各层可依赖，自身不依赖任何 backend 业务包）
```

| 层 | 允许依赖 | 禁止依赖 |
|---|---|---|
| commands | service / domain / pkg / app / shell / tauri | repo / db |
| service | repo / domain / pkg / app | commands / tauri |
| repo | domain / pkg / rusqlite | commands / service / app |
| domain | pkg | 任何上层 |
| app | domain / repo / pkg / tauri（仅 `Manager` trait） | commands |
| shell | domain / pkg / tauri | commands / service |
| platform | pkg / tauri | commands / service |
| **pkg** | 标准库 / 第三方工具 | **任何其他 backend 业务包** |

**能力域之间不互引**：`app` / `shell` / `platform` 各自只往上交给 `commands` 接。
它们要合作就在 `commands` 里编排，不在能力域里互相 import——一互引，
独立测试与独立演进同时作废。

**`service` 不知道窗口与托盘的存在**：它只回答「这件事该怎么做」，
「怎么让用户看见」是 `commands` 与 `shell` 的事。这条边界让业务逻辑
可以在没有 WebView 的测试里跑（`cargo test` 编译 lib 而不拉起窗口）。

### 2.3 聚合根文件组织（domain 单文件多形态）

一个聚合根一个文件，该实体所有形态同居一处：

| 后缀 | 语义 | 标配 |
|---|---|---|
| DO | Domain Object，映射数据库表 | 必有 |
| REQ | 前端 → 后端入参 | 按需 |
| VO | 组装后的视图对象 | 按需 |

包级错误常量集中在 `pkg/error.rs` 的 `codes` 模块里，不在各域文件里重复声明。

#### 2.3.1 内联结构体边界（强制）

`commands / service / repo` 三层禁止就地声明入参/出参/实体结构体，
必须引用 `domain.XxxREQ/VO/DO`。内联结构体是契约漂移的起点——
后端改了字段名，编译不报错，前端拿到 undefined。

例外：GORM/rusqlite 临时查询条件允许内联（须加注释）。

#### 2.3.2 空数组契约（强制）

Rust 的 nil slice 序列化成 `null`，前端按数组读就是
`Cannot read properties of null`——它崩在渲染里，用户看到的只有「界面出现异常」。

- 出参里的切片与 map 字段**构造时一律初始化**（`Vec::new()` / `BTreeMap::new()`）
- 前端接住数组字段时同样按「可能是 null」处理（`res.entries ?? []`）
- `tests/chain_tests.rs` 的 `empty_lists_serialize_to_array_not_null` 守护这一点

#### 2.3.3 前端目录（强制）

`frontend/src` 是唯一源码目录，只有一层：
`api / components / composables / locales / router / stores / types / utils / views`，
外加五个 CSS：

| 文件 | 职责 |
|---|---|
| `themes.css` | 色值与尺寸刻度的**唯一来源**（`--nt-*` 令牌 + 两套 `[data-theme]`） |
| `ep-theme.css` | Element Plus 变量 → `--nt-*` 的桥，外加按钮五态等细节对齐 |
| `base.css` | 重置、排版基线、滚动条、可复用的面与图标基元 |
| `content.css` | EP 没有对应物的自绘区：编辑器 Chrome、标签、查找高亮、diff |
| `print.css` | 打印样式：只印正文、界面骨架不印、语法高亮转黑 |

`components/` 下按域分子目录：`layout`（标题栏 / 菜单 / 工具栏 / 标签 / 侧栏 / 状态栏）、
`editor`、`panels`、`dialogs`、`common`（图标、模态、提示）。

**禁止**：新增第六个顶层 CSS；在组件里写死色值；把布局间距写成裸像素。
布局与间距用 UnoCSS 原子类（其刻度已接到 `--nt-*` 令牌）。

Tauri 生成的绑定**不入库**：`@tauri-apps/api` 是 npm 包，不需要 `wailsjs` 那种
提交生成物的流程。前端不手写 invoke 字符串之外的绑定层。

### 2.4 错误处理（强制 AppError）

```rust
AppError::new(code, message)                  // 业务错误
AppError::wrap(code, message, cause)          // 包住底层错误，保留 details
```

```rust
// ✅ 业务错误
if path.is_empty() { return Err(AppError::new(codes::PATH_EMPTY, "路径不能为空")); }
// ✅ Wrap 底层错误
fs::write(p, data).map_err(|e| AppError::wrap("1002", "写文件失败", e))?;
// ❌ 禁止
return Err(anyhow!("write failed"));   // 丢 code，前端无法分流
```

**码值是数字字符串**（`"1001"`），不是 int：前端要靠 code 分流，
而 int 会被误读成状态码。字符串从类型上把两者分开。

**错误码段位**：

| 段位 | 域 |
|---|---|
| 1000–1999 | 通用 / 文件 / 路径 |
| 2000–2999 | 配置 / 持久化 / DB |
| 3000–3999 | 编码 / 文本检测 |
| 4000–4999 | 编辑器域（标签 / 注释 / 行号） |
| 5000–5999 | 工具箱（JSON / diff / 哈希 / 正则） |
| 6000–6999 | 监控 / 文件监听 |
| 7000–7999 | 壳层（单实例 / 托盘 / 窗口） |
| 8000–8999 | 用户数据（书签 / 片段 / 宏 / 草稿） |

每个码的完整含义与处置动作见后端 `pkg/error.rs` 的 `codes` 模块（码值常量集中处）。

**message 必须带「下一步」**：不是「失败了」，而是「读不了：文件被占用」。
前端 `utils/errors.ts` 按码段补第二步动作，两端不重复同一句话。

### 2.5 命名规范

| 类别 | Rust | TypeScript |
|---|---|---|
| 文件名 | `snake_case.rs` | `PascalCase.vue` / `camelCase.ts` |
| 类型 / 接口 | `PascalCase`（不加 `I` 前缀） | `PascalCase` |
| 函数 | 导出 `PascalCase` / 私有 `camelCase` | `camelCase` |
| 变量 | `camelCase` | `camelCase` |
| 常量 | `SCREAMING_SNAKE_CASE` | `SCREAMING_SNAKE_CASE` |
| 错误变量 | `ErrXxx` | — |
| JSON tag | **snake_case** | 同左（与后端一一对应） |
| 前端 invoke 名 | — | 与 Rust 函数名一致的 snake_case |

**禁止**：接口加 `I` 前缀；匈牙利命名；`xxxId` 与 `xxxID` 混排；
`any` / `as never` 之类的强转绕过类型检查。

**JSON 契约铁律**：所有跨边界字段一律 snake_case。前端 `types/api.ts`
消费的字段名必须与后端 DO/VO 的 json tag 完全一致；改字段必须同步
`CONTRACT_VERSION`。

### 2.6 注释规范（强制精简）

注释的读者是「半年后要改这段代码的人」。他要的是**这个东西是什么、边界在哪**，
不是作者的思考过程。

| 位置 | 要求 |
|---|---|
| 文件头 | 1 行说清这个文件的职责（最多 2 行） |
| 类型 | 1 行说它是什么；契约类类型可到 2 行说清边界 |
| 方法 | 1 行签名级说明（干什么 / 返回什么），不写实现步骤 |
| 字段 | 名字不自解释时写 1 行；枚举与哨兵值必须写清取值含义 |
| 行内 | 只写「为什么」，1 行 |

- **最低要求就这三样**：文件有头、导出方法有签名说明、字段有注释。够了就停
- **连续注释块 ≤ 2 行**（`go:build` / `go:embed` 指令前的分隔行除外）
- 注释只写**结论与边界**，不写论证：「为什么这么设计、当初踩过什么坑」属于 `docs/` 与 `specs/`
- 禁止：过程性叙述、TODO 历史、实现细节叙事、外部项目名与「参考 / 借鉴」字样

### 2.6.1 文档规范

文档只回答三件事：**这个项目是什么、每个模块怎么设计实现、为什么这么选型**。

- 根级：`README.md`（入口与能力全景）/ `AGENTS.md`（工程规范，唯一权威）/ `DESIGN.md`（视觉语言）
- `specs/`：架构与子系统规格，编号 00-17，一个主题一个文件
- 每篇只写现状：定位 → 设计 → 契约 → 关键流程 → 约束 → 取舍。**优势与代价必须写出来**
- 表格与短段落优先；能用表就不用列表
- **不写改动过程**：不建 CHANGELOG，不记录「本次改了什么」「之前坏在哪」，
  设计变更直接改进对应文档；历史由 Git 承载
- 不写外部项目名与「参考 / 借鉴 / 类似 X」——只讲自己怎么设计、为什么这么设计
- 文档必须与代码同步：改行为时同时改对着这篇行为的文档与注释，不同步视为未完成

### 2.7 ID：不造自己的业务 id

标签 / 书签 / 片段用数据库自增整数：它们不出本机，自增更省事也更好读。
**宏与草稿用 UUID 字符串**（`pkg::id::uuid_v4()`）：它们要被导入导出，
自增整数在导入时会与本地已存在的撞车。

没有用户表：单机桌面应用里本机用户就是唯一用户，数据存在 `%APPDATA%` 里。

### 2.8 时间戳统一毫秒整数

跨边界的 struct 里时间一律 `i64` 毫秒，禁止 `time::SystemTime` 直出：

```rust
created_at: i64   // JSON 里是毫秒整数
```

RFC3339 字符串要前端再解一次，而 `Date.parse` 对不带时区的串会按本地时间猜。
毫秒整数没有这个歧义。

### 2.9 平台特定代码（build tag）

```rust
#[cfg(windows)]
mod dialog_win;
#[cfg(not(windows))]
mod dialog_stub;
```

禁止一个文件里 `runtime.GOOS` 分支处理所有平台（托盘 / 消息框等系统能力必须分文件）。
本期只发 Windows，但编译仍要过：非 Windows 给一个能编译的桩，比在调用处写两个分支干净。

### 2.10 横切关注点

| 关注点 | 方案 |
|---|---|
| 统一错误 | AppError + 错误码分段 |
| 日志 | tracing 仅 info / warn / error；分文件落盘（`app.log` 全量 + `warn.log` 告警） |
| 前后端通信 | Tauri invoke（无 HTTP 服务、无端口）+ `emit` 事件 |
| 文件监控 | `notify` 一个 watcher 复用，`watch_dir` 只加路径；事件只报 modify/create/remove |
| 配置 | SQLite `settings` KV 表（整块 JSON，前端每次改动防抖 400ms 回写） |
| 原子写 | 同目录临时文件 + rename（跨卷 rename 会退化成复制，原子性就没了） |
| 脏标记 | 按「与载入时快照不同」判，不是按「有没有输入过」 |
| 单实例 | `tauri-plugin-single-instance`，二次启动转交文件路径 |
| 路径比较 | 同一份路径的短名（`RUNNER~1`）与长名字面不等；判越界前先 canonicalize |

禁止引入：OpenTelemetry / Prometheus 客户端；AOP 框架；全局可变状态
（`static` 一律 `OnceLock` 或原子类型）。

### 2.10.1 前端设计令牌（强制）

视觉语言见 `DESIGN.md`。核心约束：

- 色值只在 `themes.css` 定义（`--nt-*` 语义令牌 + `[data-theme]` 块）
- **表面走 `--nt-elev-*` 六级阶梯**（window / workspace / surface / raised / hover / selected）：
  相邻两级只差 2%~5% 亮度，读起来是一片连续工作区而不是卡片叠卡片。
  组件禁止自己选灰阶，一律引用阶梯；**表单控件另走 `--nt-control-*` 一组**
- 尺寸令牌全部乘 `--nt-vw-scale`（由 `useViewportScale` 按「窗口宽 ÷ 1440」写入，
  上下限 0.8~1.35）：分辨率不同的机器上整套刻度等比跟随，不改代码；
  组件层写死裸像素只允许在令牌定义处
- 强调色只有同一个靛蓝色相的五档，没有第二个色相。
  数据可视化取 `--nt-ch-1..4`
- 组件禁止写死颜色：用 `var(--nt-*)`，不用 hex
- 分层靠「圆角 + 1px 中性描边 + 一层极轻阴影」（`--nt-card-shadow`）：
  禁彩色描边、禁 backdrop-blur 叠层。
  渐变只允许三处：图表面积、欢迎页光晕、用户自选的背景图
- 布局与间距用 UnoCSS 原子类：`uno.config.ts` 已把 spacing / fontSize / borderRadius / colors
  全部接到 `--nt-*`，`gap-4` 就是 `var(--nt-sp-4)`。禁止在原子类里写裸像素或 hex
- Chrome 五层高度走令牌（菜单 32 / 工具栏 34 / 标签 32 / 侧栏头 30 / 状态栏 26）：
  编辑器是密集操作界面，这五层每高一像素都吃掉一屏内容
- 按钮五态必须齐全（默认 / 悬停 / 按下 / 禁用 / 加载）：
  **全站统一 1.06 / 0.92**，EP 按钮与自绘按钮必须同一档。
  `text` / `link` 是文字级控件，只给颜色反馈；
  禁用与加载两态一律 `transform: none`，否则连点会让按钮持续抖
- 危险按钮用中性描边 + 危险色文字：彩色描边违反分层纪律
- 只读读数（光标位置 / 行数 / 编码）**不套描边胶囊、不做悬停缩放**：
  有盒子没动作等于骗点击，要反馈就换字色不换几何
- 图标统一走 `NtIcon` 内联 SVG（24 网格，`--nt-icon-stroke`）；
  禁止 Unicode 字符或 Emoji 冒充图标
- 文案一律走 i18n（`src/locales/`，key 按域分层）；
  模板里禁止硬编码用户可见字符串；文案里的裸 `@` 必须写成 `{'@'}`

### 2.10.2 国际化（强制）

中文默认 + 英文，两者永远同构：**加一个 key 就两个文件同时加**。密钥与失效由
`scripts/i18n-audit.mjs` 机械核对（`check.ps1 -Full` 会跑），不靠人工对表。

| 位置 | 该写什么 | 不能写什么 |
|---|---|---|
| 组件模板 | `t('dialog.drafts')` | 任何中文文本节点 / 中文 `title` / `placeholder` |
| 组件脚本 | `useI18n()` 的 `t` | 中文字符串字面量 |
| store / utils / api | `tr()`（`@/i18n`） | 同上——这些地方没有组件实例，但仍必须走 i18n |
| 命令表 | `labelKey` / `hintKey` 指向 key | 直接给文案 |
| 后端 | `AppError` 的 message 写中文 | — |

**后端 message 固定中文**：Rust 侧没有「界面语言」这个概念（窗口属于前端），
所以它只负责一句中文描述。英文界面由前端 `normalizeError()` 按错误码取
`errCode.<code>` 的英文句子，后端那句降级为 `details`。
**加新错误码时，`pkg/error.rs` 与 `locales/{zh-CN,en}.ts` 的 `errCode` 三处要一起加**，
审计脚本会校验。

**Element Plus 文案跟随界面语言**：`main.ts` 注入 `computed<Language>`，
`App.vue` 在最顶层 `inject` 后交给 `ElConfigProvider`。
`inject()` 必须在 `setup` 顶层同步调用——放进 `computed` 的 getter 里会在每次
求值时报「inject() can only be used inside setup()」。

### 2.11 数据库迁移

- rusqlite `execute_batch` 里 `CREATE TABLE IF NOT EXISTS`：幂等，已存在的表跳过
- `PRAGMA user_version` 记版本：下一个迁移写成 `MIGRATION_V2` 并在 `migrate()` 里
  加一段 `if v < 2`，顺序执行，跳版本（v0 → v2）时两段都会跑
- 新增字段只增不删、零值兜底；废弃字段留在表里（改语义靠代码兼容），不主动 DROP

### 2.12 invoke 命令约定（强制）

| 维度 | 规则 |
|---|---|
| 命名 | snake_case，与 Rust 函数名一致 |
| 出入参 | 一律引用 `domain` 的类型；两个以上参数用结构体而不是位置参数 |
| 删除语义 | `remove_xxx(id)` |
| 查询型系统能力 | 窗口控制 / 对话框 / 应用信息可以直接碰 Tauri 运行时（不经 service） |
| 错误 | 返回 `AppResult<T>`，由 `api/index.ts` 归一成 `{ code, message, details }` |
| 二进制 | 走 `read_binary`，出参 base64（`Vec<u8>` 会序列化成 `number[]`，
  几 MB 就把通道堵死）；读取带 20MB 上限，超限让人话提示。
  不给渲染层挂 `plugin-fs`——那等于把整块磁盘权限交出去 |

前端 API 层收敛在 `frontend/src/api/index.ts`，命令名与 `commands/mod.rs`
的 `command_handlers!` 清单一一对应。漏注册一个的后果是前端 invoke 报
`command not found`，编译期发现不了——由人工对表保证。

### 2.13 关闭与退出契约（强制）

见 [specs/04](specs/04-desktop-shell.md)。核心不变量：

| 不变量 | 锁死原因 |
|---|---|
| 前端关窗必须走 `window_close` → 等 `app:before-close` → 确认后 `confirm_quit` + `force_quit` | 后端 `CloseRequested` 默认拦下关闭请求，前端不置确认位，窗口会「点了 X 没反应」 |
| `force_quit` 先置确认位再 `app.exit(0)` | `exit` 内部会触发 `CloseRequested`，不置位会被自己拦回去 |
| 「关闭到托盘」只在窗口可见时拦截 | 窗口已经藏着时再拦一次，用户永远唤不回 |
| 收托盘前先推 `app:to-tray` 类提示 | 窗口「凭空消失」最让用户困惑 |
| 退出前先存会话与草稿 | 存不上也要继续退出——拦着用户不让关更糟 |
| 未保存修改必须问，不能静默丢 | 已命名文件的修改是用户的资产，不替用户做主 |

---

## 3. 测试规范

**原则**：一次失败必须能指认出一件真实坏事。达不到这个标准的断言不写。

### 3.1 保留判据

写之前先问：**这条测试失败时，用户界面上会出现什么坏事？**
说不出那句坏事，它就不该存在。

| 保留 | 删除 |
|---|---|
| 跨模块 / 跨进程的长链路（装配 → 建表 → 书签往返） | 单个纯函数的输入输出 |
| 协议硬约束（空数组永不 null、行尾按原样写回、BOM 剥离与补回） | 简单 CRUD、字段映射、枚举转换 |
| 安全护栏（路径穿越、原子写不留临时文件） | 幂等 setter、构造函数冒烟 |
| 顺序语义（标签顺序、光标恢复） | 只验「不 panic」「非 nil」的弱断言 |
| 跨边界的端到端（invoke 出入参序列化） | 同一行为在不同文件里的重复断言 |

两个硬规则：

1. **一个子测试只讲一种坏法**。能写成「A、B、C 都对」的子测试，失败时报文说不清坏的是哪个。
2. **同一个行为只断言一次**。repo 自己的语义归 repo 的链，编排层的语义归 service 链。

### 3.2 规模与速度约束

- **单个集成测试文件**（`src-tauri/tests/chain_tests.rs`），按 section 分「文件与编码 / 会话 / 批量 / 工具箱」四组。
  所有长链路集中一处，编译只链接一次 lib
- **昂贵的 setup 只在入口准备一次**：建临时目录、装配服务容器都属于这一类
- 测试用 `tempfile::tempdir()` 隔离，**绝不碰用户真实的 `%APPDATA%\NoteText`**
- 不写 `-short` 隔离：当前没有依赖真实网络 / 真实 git 的链路，
  真要加时用标准 `testing.Short()` 守卫，不另造自定义 tag

### 3.3 验证入口

一个脚本收口全部验证动作，不在文档 / CI / 本地各写一套命令。分档按「改了多大」选：

| 命令 | 跑什么 | 什么时候用 |
|---|---|---|
| `pwsh scripts/check.ps1` | rustfmt → cargo check → 全量测试 | 日常快门 |
| `pwsh scripts/check.ps1 -Full` | + i18n 审计 + clippy `-D warnings` + 前端构建 | 改了前端 / 提交前 |
| `pwsh scripts/check.ps1 -List` | 列出全部测试名 | 不知道入口叫什么时 |

- **默认安静**：失败时脚本自己从日志里摘出摘要（测试名 + `.rs:行号`）
- 全量测试 < 1 秒，**不设「只跑一条链」的开关**——加了省不了多少时间，却要多记一个参数
- `scripts/i18n-audit.mjs` 可以单独跑：`node scripts/i18n-audit.mjs`
  它查四件事——key 引用是否都有定义、zh/en 是否对齐、模板里有没有裸中文、
  后端错误码是否两边都有文案。四类问题都是编译期抓不到的

### 3.4 测试索引

单个文件 `src-tauri/tests/chain_tests.rs`，按四组排列；每个函数名就是它守护的那句坏事，
失败时报文直接指出坏在哪，不必对着行号往回找。简单纯函数断言不入链；每一条都守护
「用户文件没了 / 界面崩白屏」级别的事故。**同一条链路的两面合并成一条**
（如批量替换的「扫描只读」与「只改勾选」），一条测试就是一条完整的用户故事。

| 分组 | 测试 | 坏法 |
|---|---|---|
| 文件与编码 | `read_write_roundtrip_preserves_line_ending_and_bom` | 行尾没记住 / BOM 丢或多字节 |
| | `forced_encoding_read_decodes_and_falls_back_when_unsupported` | 选了编码打开的仍是乱码 |
| | `atomic_write_leaves_no_temp_file_and_never_creates_missing_dirs` | 临时文件堆积 / 打错路径在磁盘留痕 |
| | `rename_rejects_overwrite_cycles_and_escapes` | 重命名无声销毁文件 / 树成环 / 路径逃逸 |
| | `delete_accounts_for_stale_paths_and_create_refuses_existing` | 一个陈旧路径让整批删除失败 |
| 会话 | `first_launch_writes_defaults_and_gives_empty_session` | 首次启动没写默认配置 / 空库崩 |
| | `session_round_trip_restores_tabs_and_survives_close_all` | 重启后标签顺序与窗口尺寸丢失 |
| | `empty_lists_serialize_to_array_not_null` | 前端按数组读拿到 null 白屏 |
| 批量 | `batch_replace_scans_readonly_and_applies_only_checked` | 扫描就落盘 / 改掉没勾选的文件 |
| | `batch_rename_blocks_conflicts_and_lands_clean_plans` | 重命名覆盖已有文件 / 该落地的没落地 |
| 工具箱 | `bad_json_reports_line_and_column` | 只说「格式错误」用户找不到在哪 |
| | `hashes_match_known_vectors` | 哈希算错，用户误判文件损坏 |
| | `toggle_comment_round_trips_and_fills_half_commented_blocks` | Ctrl+/ 把代码注释坏 |

**改这里 → 这样验**：

| 改动位置 | 命令 |
|---|---|
| 某个域的行为 | `pwsh scripts/check.ps1` |
| 不知道入口叫什么 | `pwsh scripts/check.ps1 -List` |
| 拿不准 | `pwsh scripts/check.ps1` |
| 改了前端 | `pwsh scripts/check.ps1 -Full` |


---

## 4. 架构门禁

```bash
pwsh scripts/check.ps1 -Full         # rustfmt + 前端构建 + cargo check + clippy + 全量测试
./frontend/node_modules/.bin/tauri build   # 产物 target/release/NoteText.exe
```

脚本逐步展开就是下面这些命令；单独跑某一步时才手敲：

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check   # 必须无输出
cargo check --manifest-path src-tauri/Cargo.toml --all-targets    # 编译比测试更早暴露类型错误
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --lib --test chain_tests
cd frontend && npm run build         # vue-tsc -b && vite build：类型检查已串在构建里
```

为什么测试不喂 `--all-targets`：没有测试文件的包交给 `cargo test` 也要编译一遍，
而它们的编译错误已经由 `cargo check --all-targets` 兜住了。

`rustfmt --check` 要能当门禁用，行尾就必须统一：`.gitattributes` 里 `*.rs text eol=lf`
（Windows 上 `core.autocrlf=true` 会把检出内容写成 CRLF，而 rustfmt 把 CRLF 文件
一律报成「未格式化」）。§2.6 的注释行数上限同样可机械核对——
源码里 grep 「连续 3 行以上 `//` 的块」，命中就压到 2 行。
