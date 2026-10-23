# NoteText

**轻量级 Windows 桌面文本编辑器。** Rust 后端 + Tauri 2 壳 + Vue 3 前端，冷启动 < 1 秒，
安装包 ~5MB，内置代码编辑、查找替换、编码转换、JSON 工具、Diff 对比、Markdown 预览与文档预览。

![Windows 10/11](https://img.shields.io/badge/platform-Windows%2010%2F11-blue)
![Tauri](https://img.shields.io/badge/Tauri-v2-24C8DB)
![Rust](https://img.shields.io/badge/Rust-1.80+-DEA584)
![Vue](https://img.shields.io/badge/Vue-3.4-42b883)
![License MIT](https://img.shields.io/badge/license-MIT-yellow)

## 你会得到什么

| | 能力 |
|---|---|
| 编辑内核 | 多标签（每个标签独立撤销历史）、分屏、28 种语言高亮、代码折叠、括号配对着色、列块编辑、选中即高亮、右键菜单（撤销/剪切复制/行操作/大小写/排序/去重/Tab 空格/书签行/5 色标记）、行号点选整行、书签（gutter 点一下即标记）、转到行 |
| 文件树 | 懒展开、右键全套（新建文件/文件夹、重命名、删除、剪切/复制/粘贴、创建副本、复制路径、资源管理器显示）、树内拖拽移动到目标文件夹 |
| 编码 | 打开时探测（BOM 优先），状态栏一键切换，12 种编码互转；行尾独立切换，保存按原样写回 |
| 查找替换 | 本文件 `Ctrl+F` / `Ctrl+H`（实时计数、结果带行号、替换全部一次撤销），文件夹级 `Ctrl+Shift+F`（支持 glob 与超大文件跳过） |
| 视图器 | Markdown 三态（编辑/分屏/预览，Mermaid 图表 + KaTeX 公式 + 代码高亮 + 导出 HTML）、图片、日志（ERROR/WARN 着色）、十六进制、文档预览（xlsx / docx / pdf） |
| 工具箱 | JSON（格式化/校验/JSONPath/转结构体）、格式转换（JSON/YAML/TOML/XML 互转）、文本与目录 Diff、哈希、正则测试、编码转换、11 种行编辑、取色器、日志查看器 |
| 批量操作 | 目录级查找替换（先扫描预览再勾选应用）、批量重命名（三种模式，冲突项标红） |
| 菜单与命令 | 十个一级菜单（文件 / 编辑 / 查找 / 视图 / 编码 / 语言 / 工具 / 对比 + 设置 / 帮助），分组由命令表驱动：新命令注册后自动出现在菜单、快捷键设置页与冲突检测里 |
| 效率工具 | 宏录制回放、列编辑模式、小地图、剪贴板历史、最近文件（可钉住） |
| 壳层 | 单实例、托盘常驻、文件关联、会话恢复（含未保存草稿）、自动保存（仅已命名文件）、文件监控（外部改动静默重载、本地有改动时只提示）、拖入文件/文件夹即打开、备份导入导出 |
| 外观 | 浅/深/跟随系统三档主题，界面字号 80%–140% 缩放，全屏 `F11`，打印 `Ctrl+P` |

## 快速开始

### 环境要求

- **Rust** 1.80+（stable，MSVC 工具链）
- **Node.js** 18+
- **Windows 10 1809+ / 11**（系统已内置 WebView2）
- Tauri CLI：`cargo install tauri-cli --version "^2"`

### 开发

```bash
npm --prefix frontend install          # 首次
npm --prefix frontend run dev          # 浏览器预览（无后端，显示空态）
./frontend/node_modules/.bin/tauri dev   # 完整开发（Vite HMR + Rust 后端）
```

### 构建

```bash
./frontend/node_modules/.bin/tauri build            # 产物 target/release/NoteText.exe
./frontend/node_modules/.bin/tauri build --bundles nsis  # NSIS 安装包
```

### 本地验证

```bash
pwsh scripts/check.ps1                 # 快门：格式 → 编译 → 测试
pwsh scripts/check.ps1 -Full           # 全量门禁：+ i18n 审计 + clippy + 前端构建
node scripts/i18n-audit.mjs            # 单独查国际化
```

| 命令 | 作用 |
|---|---|
| `pwsh scripts/check.ps1` | rustfmt → cargo check → 测试 |
| `... -List` | 列出全部测试名 |
| `... -Detail` | 打印每个测试的名字与结果 |
| `... -Full` | + i18n 审计 + clippy `-D warnings` + 前端构建（提交前 / CI） |
| `node scripts/i18n-audit.mjs` | 查 key 引用、zh/en 对齐、模板裸中文、后端错误码覆盖 |

## 项目结构

```
NoteText/
├── src-tauri/
│   ├── src/
│   │   ├── commands/          # Tauri invoke 入口（薄）
│   │   ├── service/           # 业务编排：file / editor / tools / userdata
│   │   ├── repo/              # 一张表一个 repo
│   │   ├── domain/            # DO / REQ / VO（四层禁止内联结构体）
│   │   ├── db/                # SQLite 打开 + 迁移（WAL）
│   │   ├── app/               # 能力域：路径 / 装配 / 生命周期
│   │   ├── shell/             # 壳层：单实例 / 托盘 / 关闭策略
│   │   ├── platform/          # 平台相关：Win32 消息框、AppHandle
│   │   └── pkg/               # 叶子工具包：AppError / 路径 / 日志 / id
│   ├── capabilities/          # Tauri 权限清单
│   ├── icons/                 # 应用图标
│   └── tests/                 # 按域分文件：session / file / tools / batch
├── frontend/
│   └── src/
│       ├── api/               # invoke 封装（全站唯一出口）
│       ├── components/        # layout / editor / panels / dialogs / common
│       ├── composables/       # 命令 / 主题 / 语言 / 退出守卫 / 壳意图
│       ├── stores/            # Pinia：settings / editor / fileTree / commands / dialogs / toast
│       ├── locales/           # zh-CN / en
│       ├── utils/             # 纯函数
│       └── *.css              # themes / base / ep-theme / content / print 五个 CSS
├── assets/docs/               # 内置用户手册（帮助页渲染）
├── docs/ specs/               # 项目级文档与子系统规格
└── scripts/                   # 验证入口（check.ps1）
```

## 技术栈

| 层 | 选型 |
|---|---|
| 桌面壳 | Tauri 2（无边框 + 前端自绘标题栏，WebView2 渲染） |
| 后端 | Rust（rusqlite bundled · notify · tracing） |
| 前端 | Vue 3.4 + TypeScript 5.4 + Vite 5.2 |
| 状态管理 | Pinia 2.1 |
| 路由 | Vue Router 4.3（hash 模式） |
| UI | Element Plus 2.9（按需引入，变量接到 `--nt-*`） |
| 原子 CSS | UnoCSS 0.58（刻度接 `--nt-*` 令牌） |
| 代码编辑 | CodeMirror 6（31 种语言识别，28 种语法高亮：13 静态 + 15 按需加载） |
| Markdown | markdown-it + highlight.js + Mermaid + KaTeX（编辑 / 分屏 / 预览三态） |
| 格式转换 | js-yaml、@iarna/toml、fast-xml-parser（JSON / YAML / TOML / XML 互转） |
| 文档预览 | XLSX、docx-preview、vue-pdf-embed |
| 图表 | ECharts 5.5 |
| 国际化 | Vue I18n 9.10（zh-CN 默认 + en） |
| 数据库 | SQLite（WAL，rusqlite bundled 无 CGO） |

**不引入**：微前端（Qiankun）、图编排（AntV X6 / BPMN-JS）、Axios、WangEditor——
纯 Tauri invoke 通信，前端没有 HTTP 客户端；Markdown 走 markdown-it 一条链路，
不再挂第二个富文本编辑器运行时。

## 文档

| 文档 | 内容 |
|---|---|
| [AGENTS.md](AGENTS.md) | **工程规范（唯一权威）**：分层 / 依赖方向 / 错误码 / 测试 / 命名 |
| [DESIGN.md](DESIGN.md) | 视觉语言与交互原则 |
| [docs/OVERVIEW.md](docs/OVERVIEW.md) | **项目全景**：能力清单、模块地图、一次对话的链路、关键取舍 |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | 分层、启动时序、IPC 契约、前端状态归属 |
| [docs/DATA-MODEL.md](docs/DATA-MODEL.md) | 表结构、数据落在哪、会话恢复的形态 |
| [docs/API-CONTRACT.md](docs/API-CONTRACT.md) | invoke 命令表与出入参契约 |
| [docs/PAGE-STRUCTURE.md](docs/PAGE-STRUCTURE.md) | 页面与组件地图 |
| [docs/ERROR-CODES.md](docs/ERROR-CODES.md) | 错误码段位与前端分流 |
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) | 环境、验证入口、数据位置、改代码时看什么 |
| [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) | 构建、安装器、CI 与 Release |
| specs/ | 子系统规格（01–13，一个子系统一篇） |

内置用户手册在 `assets/docs/`，面向用户；`docs/` 与 `specs/` 面向开发，两者不重复维护同一份话术。

## 测试

23 个测试分 4 个文件，一个域一个文件；**每个测试函数名就是它守护的那件坏事**，
失败时报文直接指出坏在哪。全量 < 1 秒。

```bash
cargo test --manifest-path src-tauri/Cargo.toml
pwsh scripts/check.ps1 -List        # 列出全部测试名
```

| 文件 | 测试数 | 覆盖 |
|---|---|---|
| `session_chain.rs` | 5 | 首次启动写默认配置；会话顺序、活跃索引、窗口尺寸往返；关光标签仍留会话行；书签去重与切换删除；空列表序列化成 `[]` 而非 `null` |
| `file_chain.rs` | 8 | CRLF 归一但记住原样式；不指定行尾时原样写回；原子写不留临时文件、不替你造目录；BOM 剥离与补回；强制编码读取与退回；重命名不覆盖；拒绝相对路径与成环；删除逐个记账 |
| `tools_chain.rs` | 6 | 坏 JSON 给行列号；diff 增删计数；正则捕获组；三种哈希对已知向量；注释切换往返与半注释补齐；不支持行注释要报错 |
| `batch_chain.rs` | 4 | 扫描不改盘；应用只改勾选的；重命名遇冲突标记且整体拒绝；无冲突真的落地 |

不知道入口叫什么：`pwsh scripts/check.ps1 -List`。

## 许可证

[MIT](LICENSE)
