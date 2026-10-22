# 开发指南

## 环境要求

- **Rust** 1.80+（stable，`x86_64-pc-windows-msvc` 工具链）
- **Node.js** 18+
- **Windows 10 1809+ / 11**（系统已内置 WebView2）
- Tauri CLI：`cargo install tauri-cli --version "^2"`
- Visual Studio Build Tools（MSVC 链接器）：Tauri 的 Rust 侧要 `link.exe`

## 常用命令

验证只有一个入口：`scripts/check.ps1`。分档规则与测试索引见
[AGENTS.md §3](../AGENTS.md)，这里只留平时手边那几条：

```powershell
pwsh scripts/check.ps1              # 日常快门（rustfmt → check → 测试）
pwsh scripts/check.ps1 -Chain batch  # 只跑名字命中的那条链
pwsh scripts/check.ps1 -List        # 列出全部测试链
pwsh scripts/check.ps1 -Full        # + clippy -D warnings + 前端构建

npm --prefix frontend install       # 首次
npm --prefix frontend run dev       # 浏览器预览（无后端，空态）
./frontend/node_modules/.bin/tauri dev     # 完整开发（HMR + Rust 后端）
./frontend/node_modules/.bin/tauri build   # 产物 target/release/NoteText.exe
```

定位测试的标准动作：先 `-List` 拿索引，再 `-Chain <关键字>` 落到一条链，
不翻测试文件。

单独跑某一步时才手敲：

```powershell
cargo check --manifest-path src-tauri/Cargo.toml --all-targets
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test  --manifest-path src-tauri/Cargo.toml          # 全部域
cargo test  --manifest-path src-tauri/Cargo.toml -- file    # 只跑文件域
cd frontend; npx vue-tsc -b; npx vite build
```

## 隔离数据（调试时）

```powershell
$env:NOTETEXT_HOME = "$env:TEMP\nt-dev"
./frontend/node_modules/.bin/tauri dev
```

**调试时一定要设**：不设会读写你真实的 `%APPDATA%\NoteText`，
而开发期反复写库很可能把用户的书签 / 片段搞乱。

## 数据落在哪

```
%APPDATA%\NoteText\            （或 NOTETEXT_HOME）
├── notetext.db                SQLite（WAL）：全部用户数据与界面设置
├── config.json                预留（固定项目前都在 DB 里）
├── logs\
│   ├── app.log                全量（20MB × 3 滚动）
│   └── warn.log               只要 warn 与 error
└── backup\                    导出的备份
```

排障第一步永远是看 `logs\app.log` 的启动横幅：它同时打印数据目录与版本号。

## 改后端时

| 目录 | 注意 |
|---|---|
| `src-tauri/src/pkg` | 叶子工具包铁律：不依赖任何其他 backend 业务包，不出现业务词汇 |
| `src-tauri/src/domain` | 入参 / 出参 / 实体都放这里，三层不得就地声明结构体 |
| `src-tauri/src/repo` | 只写 SQL，不写业务规则；参数绑定是唯一写法 |
| `src-tauri/src/service` | 编排层：一个关注点一个文件；不知道窗口与托盘的存在 |
| `src-tauri/src/service/encoding.rs` | 编码表是手写的子集，**要扩展时优先换 encoding_rs** |
| `src-tauri/src/service/tools.rs` | 哈希与迷你正则引擎都是手写实现，改前先看测试向量 |
| `src-tauri/src/commands` | 薄 handler；新增命令必须同时进 `command_handlers!` 清单与前端 `api/index.ts` |
| `src-tauri/src/app` | 组合根在这里；失败模式见 [ARCHITECTURE](ARCHITECTURE.md) 的启动时序 |
| `src-tauri/src/shell` | 关闭策略的不变量见 [specs/04](specs/04-desktop-shell.md)，改前先读 |
| `src-tauri/src/main.rs` | **不要写 `mod lib;`**（见 [ARCHITECTURE](ARCHITECTURE.md) 末尾） |

新增一个能力时的顺序：
`domain` → `repo`（若要落库）→ `service` → `commands`（+ 清单）→
`frontend/src/api/index.ts` → `types/api.ts` → 组件 → 文档。

## 改前端时

| 目录 | 注意 |
|---|---|
| `views/` | 三个路由视图；布局五层只在 EditorView 装配 |
| `components/layout` | 五层 chrome，高度全走令牌 |
| `components/editor` | CodeMirror 容器 + compartments；新关注点进 `composables/` 再挂 compartment |
| `composables/useEditorRegistry` | 正文的唯一出口；**不要在 store 里存正文** |
| `stores/settings` | 配置镜像 + 派生量；改动防抖 400ms 回写 |
| `types/api.ts` | 与后端 json tag 一一对应；改字段必须同步 `CONTRACT_VERSION` |
| `locales/` | 中英两套文案，key 按域分层；模板里不写用户可见字符串；裸 `@` 写成 `{'@'}` |
| `themes.css` | 色值与尺寸刻度的唯一来源；组件里禁止写 hex |
| `ep-theme.css` | Element Plus 变量 → `--nt-*` 的桥；按钮五态等细节对齐 |
| `content.css` | EP 没有对应物的自绘区：编辑器 chrome、查找高亮、diff、Markdown、十六进制 |
| `uno.config.ts` | 原子类刻度已接到 `--nt-*`：`gap-4` = `var(--nt-sp-4)`，禁止写裸像素 |

浏览器里看界面：`npm run dev` 会因为没有后端而显示空态——
要连真后端请用 `./frontend/node_modules/.bin/tauri dev`。

## 常见问题

| 现象 | 先看 |
|---|---|
| `cargo check` 报 `file not found for module 'app'` | `main.rs` 里写了 `mod lib;`，删掉 |
| 前端 invoke 报 `command not found` | 命令没进 `command_handlers!` 清单，或名字不一致 |
| 前端类型全变 `undefined` | 后端字段名与 `types/api.ts` 不一致；查 json tag |
| 保存后整个文件行尾变了 | `write_file` 没传 `line_ending`，或前端传了 `null` |
| 大文件打开卡死 | 不会自动降级：超过 32MB 应只读前段并标「已截断」，见 [specs/05](specs/05-file-domain.md) |
| 界面样式全丢 | 引用了不存在的 CSS 变量——那会静默失效，最危险 |
| 深色下浮层与内容糊在一起 | `ep-theme.css` 的 `--el-overlay-color-lighter` 没按主题分开 |
| EP 按钮显示成裸文字（无底无框） | 缺 `unplugin-element-plus`：`ElementPlusResolver` 只管模板里的标签，脚本里 `import { ElButton }` 的组件样式要它补。见 `vite.config.ts` |
| 自绘按钮的 padding/字号被吃掉 | `base.css` 里的 `button` 重置被写成了带 `:not()` 的高优先级选择器，压过了组件类。按钮重置必须是裸元素选择器 |
| 编辑器里 Ctrl+S / Ctrl+F 没反应 | `useGlobalKeys` 的 `isTyping` 把整个 `.cm-editor` 排除了。判据应是「这条组合有没有命令认领」 |
| 切标签后内容/撤销历史丢了 | `EditorArea` 用了 `v-if` 或只渲染 active 标签，实例被销毁。必须 `v-show` + 每个标签一个实例 |
| 保存成功但标签还是脏的 | 没调 `commitBaseline(tabId)`，脏标记的基线还停在载入时 |
| 每次保存都提示「已在外部修改」 | 后端没记自写路径。见 `FileService::note_self_write` |
| 测试改了用户真实数据 | 用了 `AppPaths::resolve()` 而不是 `from_root(tempdir)` |

## 提交前

```powershell
pwsh scripts/check.ps1 -Full
```

它跑：rustfmt → 前端构建 → `cargo check --all-targets` → `clippy -D warnings` → 全量测试。
任一步失败都会打印失败摘要（测试名 + `.rs:行号`），不用下载 artifact。
