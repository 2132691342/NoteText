# NoteText

**轻量级 Windows 桌面文本编辑器。** Rust 后端 + Tauri 2 壳 + Vue 3 前端，冷启动 < 1 秒，
安装包 10MB 以内（CI 有体积门禁），内置代码编辑、查找替换、编码转换、JSON 工具、
Diff 对比、Markdown 预览、文档预览与压缩归档。

![Windows 10/11](https://img.shields.io/badge/platform-Windows%2010%2F11-blue)
![Tauri](https://img.shields.io/badge/Tauri-v2-24C8DB)
![Rust](https://img.shields.io/badge/Rust-1.80+-DEA584)
![Vue](https://img.shields.io/badge/Vue-3.4-42b883)
![License MIT](https://img.shields.io/badge/license-MIT-yellow)

## 定位

内核是 CodeMirror 6 + Rust 文件域，项目只做四件事：
把编辑器的每一类操作接顺、把开发者高频的文本处理工具内置、
把「关窗再开回到原样」这件事做扎实、把常用压缩格式的打包解包内置进来。
不自研插件系统、不做多窗口、不做云同步——这条边界让功能面收得住，
代价是高级需求要靠外部工具。

## 能力全景

| 能力 | 一句话 |
|---|---|
| 多标签与分屏 | 同一文件只有一个标签；`Ctrl+\` 分屏，比例可拖 |
| 语法高亮 | 31 个语言识别、28 种实际高亮；按扩展名识别，常用语言静态内置，大语言包按需加载 |
| 编码与行尾 | 打开时探测（BOM 优先）；状态栏一键切换；保存按「打开时的编码 + 行尾 + BOM」写回 |
| 一键快操作 | 选中即浮出操作条：格式化 / 压缩 JSON、转 YAML、大小写、查找；`Ctrl+Shift+J` 格式化选中或全文，结果原地替换 |
| 查找替换 | 本文件 `Ctrl+F`（实时计数、结果带行号、预填上次查询词）；文件夹级 `Ctrl+Shift+F`（glob + 超大文件跳过） |
| JSON 工具 | 打开即执行：格式化（缩进可调）/ 压缩 / 校验（带行列）/ JSONPath / 转 5 种结构体 |
| 对比 | 文本 LCS 行级 + 行内高亮；目录一层逐文件；与剪贴板对比 |
| 哈希 | MD5 / SHA-1 / SHA-256，大小写两种输出 |
| 正则 | 实时匹配 + 捕获组表格，`i` `m` `s` `x` 四开关 |
| 编码转换 | 12 种编码互转，可选写 BOM；一键转 UTF-8 |
| 行编辑 | 11 种行级变换（排序 / 去重 / 大小写 / 去空白…），可预览后应用 |
| 书签 / 片段 / 宏 | 侧栏面板；`F2` 加书签；片段缩写展开；宏 `Ctrl+Shift+M` 录制、`Ctrl+Shift+P` 回放 |
| 函数列表 | 侧栏按语言正则提取符号，点击跳转 |
| 列编辑 | `Alt+C` 切列模式，矩形选区逐行同列插入 |
| 小地图 | 配置开关；canvas 只画轮廓不画字，虚拟滚动跟随 |
| 剪贴板历史 | 复制 / 剪切攒 50 条（编辑器事件与命令表都收集），点击回插 |
| Hex 查看 | 16 字节一行（偏移 / hex / ASCII），只渲染可视区 |
| 文档预览 | 按扩展名分流 xlsx / docx / pdf / 图片 / Markdown / HTML（沙箱静态预览，不执行脚本）/ 纯文本 |
| 批量查找替换 | 目录 + glob，**先扫描预览再勾选应用**，不直接改磁盘 |
| 批量重命名 | 查找替换 / 加序号 / 大小写三种模式，冲突项标红不可选 |
| 压缩 / 解压 | 菜单栏顶级「压缩」菜单：新建压缩包（加密 / 固实 / 分卷）/ 解压浮窗（条目勾选 + 密码 + 覆盖策略）/ 打开浏览（增删 / 预览 / 转换）/ 测试 / 分卷合并 / 任务面板；7z / zip / tar 全家族（tgz / txz / tbz2 / tzst / tlz4 / tbr）/ gz / bz2 / zst / xz / lz4 / br 十五种格式，长任务走队列，大包打开与解压不冻窗（specs/18） |
| 文件分卷 | 任意文件按大小切卷 / 合并还原；压缩包创建时可直接分卷 |
| 系统关联 | 设置 → 系统与关联：把 NoteText 挂进文本 / 压缩包的「打开方式」与默认应用候选，双击即开（specs/17） |
| 文件树 | 懒展开 + 右键全套（新建 / 重命名 / 删除 / 剪切 / 复制 / 粘贴 / 副本 / 资源管理器 / 压缩 / 解压到… / 一键解压）+ 树内拖拽移动 |
| Markdown 三态 | `.md` 默认三态：编辑 / 分屏 / 预览；Mermaid 图表 + KaTeX 公式 + 代码高亮 + 导出 HTML |
| 格式转换 | JSON / YAML / TOML / XML 互转，带选区进来即自动转换，「应用」写回选区 |
| 取色器 | 屏幕取色（不支持时手动输入），HEX / RGB / HSL 互拷 |
| 日志查看器 | 读应用日志，按级别着色与过滤，只取末 2000 行 |
| 文件监控 | 实时跟踪当前文件的新增内容，只读增量 |
| 会话恢复 | 启动恢复标签 / 光标 / 窗口大小；未保存草稿也恢复 |
| 托盘与单实例 | 关闭到托盘；二次启动转交文件路径 |
| 备份 | 导出 / 导入书签、片段、宏、草稿、最近文件 |
| 大文件降级 | 超过 32MB 自动只读前段，状态栏标「已截断」，**拦截写回** |

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

调试时隔离数据目录，不碰真实的 `%APPDATA%\NoteText`：

```powershell
$env:NOTETEXT_HOME = "$env:TEMP\nt-dev"
./frontend/node_modules/.bin/tauri dev
```

### 构建

```bash
./frontend/node_modules/.bin/tauri build            # 产物 target/release/NoteText.exe
./frontend/node_modules/.bin/tauri build --bundles nsis  # NSIS 安装包
```

### 本地验证

```bash
pwsh scripts/check.ps1                 # 日常：格式 → 编译 → 8 条链路
pwsh scripts/check.ps1 -Test archive   # 只跑名字含关键字的链路
pwsh scripts/check.ps1 -Full           # 门禁：+ i18n 审计 + clippy + 前端构建
node scripts/i18n-audit.mjs            # 单独查国际化
```

## 项目结构

```
NoteText/
├── src-tauri/
│   ├── src/
│   │   ├── commands/          # Tauri invoke 入口（薄）
│   │   ├── service/           # 业务编排：file / editor / tools / userdata / batch / session / archive
│   │   ├── repo/              # 一张表一个 repo
│   │   ├── domain/            # DO / REQ / VO（四层禁止内联结构体）
│   │   ├── db/                # SQLite 打开 + 迁移（WAL）
│   │   ├── app/               # 能力域：路径 / 装配 / 生命周期
│   │   ├── shell/             # 壳层：单实例 / 托盘 / 关闭策略
│   │   ├── platform/          # 平台相关：Win32 消息框、AppHandle
│   │   └── pkg/               # 叶子工具包：AppError / 路径 / 日志 / id / text
│   ├── capabilities/          # Tauri 权限清单
│   ├── icons/                 # 应用图标
│   └── tests/chain_tests.rs   # 8 条长链路集成测试（单文件，编译 lib 而不链接 WebView）
├── frontend/
│   └── src/
│       ├── api/               # invoke 封装（全站唯一出口）
│       ├── components/        # layout / editor / panels / dialogs / common
│       ├── composables/       # 命令 / 快操作 / 主题 / 语言 / 退出守卫 / 壳意图
│       ├── stores/            # Pinia：settings / editor / fileTree / commands / dialogs / toast
│       ├── locales/           # zh-CN / en
│       ├── utils/             # 纯函数（格式互转 / glob / 剪贴板 / 错误翻译）
│       └── *.css              # themes / base / ep-theme / content / print 五个 CSS
├── assets/docs/               # 内置用户手册（帮助页渲染）
├── specs/                     # 架构与子系统规格（00–18，一篇一个主题）
└── scripts/                   # 验证入口（check.ps1 / i18n-audit.mjs）
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
| 代码编辑 | CodeMirror 6（31 种语言识别，28 种语法高亮：13 静态 + 16 按需加载） |
| Markdown | markdown-it + highlight.js + Mermaid + KaTeX（编辑 / 分屏 / 预览三态） |
| 格式转换 | js-yaml、@iarna/toml、fast-xml-parser（JSON / YAML / TOML / XML 互转） |
| 文档预览 | XLSX、docx-preview、WebView2 内置 PDF 阅读器 |
| 图表 | ECharts 5.5 |
| 国际化 | Vue I18n 9.10（zh-CN 默认 + en） |
| 数据库 | SQLite（WAL，rusqlite bundled 无 CGO） |

**不引入**：微前端框架、图编排 / 流程图库、HTTP 客户端库、富文本编辑器——
纯 Tauri invoke 通信，前端没有 HTTP 客户端；Markdown 走 markdown-it 一条链路，
不再挂第二个富文本编辑器运行时。完整的依赖原则与替代方案见
[AGENTS.md §1.1](AGENTS.md)。

## 文档

阅读顺序：先 README（本篇）→ [specs/00 总架构](specs/00-architecture.md) → 按需进各篇。

| 文档 | 内容 |
|---|---|
| [AGENTS.md](AGENTS.md) | **工程规范（唯一权威）**：分层 / 依赖方向 / 错误码 / 测试 / 命名 / 注释 |
| [DESIGN.md](DESIGN.md) | 视觉语言与交互原则 |
| [specs/00](specs/00-architecture.md) | 总架构：分层、启动时序、通信、状态归属、排障速查 |
| [specs/01–05](specs/01-runtime-and-paths.md) | 运行时与路径 / 数据库 / 配置 / 桌面壳 / 文件域 |
| [specs/06–10](specs/06-editor-domain.md) | 编辑器域 / 前端状态与命令 / 用户数据 / 工具箱 / 会话恢复 |
| [specs/11–13](specs/11-visual-tokens.md) | 视觉令牌 / 测试与验证 / 批量操作 |
| [specs/14](specs/14-api-contract.md) | invoke 命令契约（前后端对照表） |
| [specs/15](specs/15-error-codes.md) | 错误码分段与处置动作 |
| [specs/16](specs/16-page-structure.md) | 页面与组件地图 |
| [specs/17](specs/17-deployment.md) | 构建与分发 |
| [specs/18](specs/18-archive.md) | 归档 / 压缩子系统：格式、任务队列、安全底线 |

每篇规格只写现状：定位 → 设计 → 契约 → 关键流程 → 约束 → 取舍。
内置用户手册在 `assets/docs/`，面向用户；`specs/` 面向开发，两者不重复维护同一份话术。

## 测试

8 个集成测试集中在 `src-tauri/tests/chain_tests.rs`，按六组排列
（文件 / 会话 / 批量 / 备份 / 压缩 / 系统关联）。**每个测试函数名就是它守护的那件坏事**，
失败时报文直接指出坏在哪。全量约 1 秒；同一域的相近故事合并成一条，
一条测试就是一条完整的用户故事。

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test chain_tests
pwsh scripts/check.ps1 -Test archive   # 只跑名字含关键字的链路
```

判据见 [AGENTS.md §3](AGENTS.md) 与 [specs/12](specs/12-testing.md)：
只留跨模块长链路、协议硬约束与安全护栏，单个纯函数的输入输出不入链。

| 分组 | 守护的坏事（摘） |
|---|---|
| 文件 | 行尾 / BOM 在读写链上无损；选错编码退回探测编码而非打不开；原子写不留临时文件；重命名拒绝覆盖 / 成环 / 路径逃逸；新建同名不清空已有内容 |
| 会话 | 首次启动写默认配置；关光标签仍留会话行（记窗口尺寸）；空列表序列化成 `[]` 而非 `null` |
| 批量 | 扫描阶段不落盘、应用只改勾选的；重命名遇冲突整体拒绝、无冲突真的落地 |
| 备份 | 五个用户数据表导出后导得回；不在备份里的本机数据不被抹掉 |
| 压缩 | 十五种格式压缩往返逐字节无损；改名后的 .tar.xz / .tar.br 照样认得出；追加 / 删除 / 分卷合并不丢内容；恶意条目整包拒绝；密码三态分得清；损坏的包测得出 |
| 系统关联 | 关联注册写对注册表三件套；注销不留悬空默认、不误伤另一组 |

## 许可证

[MIT](LICENSE)
