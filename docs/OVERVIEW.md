# 项目全景

这一篇回答四个问题：**NoteText 是什么、由哪些模块组成、数据怎么流、每个决定换来了什么**。
看完它再进各子系统规格（`specs/`）与契约文档（`docs/`），不需要先读代码。

## 一句话

NoteText 是一款轻量级 Windows 桌面文本编辑器。**内核是 CodeMirror 6 + Rust 文件域**，
本项目只做三件事：把编辑器的每一类操作接顺、把开发者高频的文本处理工具内置、
把「关窗再开回到原样」这件事做扎实。

```
编辑器负责「改文本」，NoteText 负责让「打开 / 保存 / 查找 / 转换 / 恢复」都不打断你。
```

## 能力清单

| 能力 | 落在哪 | 一句话 |
|---|---|---|
| 多标签与分屏 | 编辑器页 | 同一文件只有一个标签；`Ctrl+\` 分屏，比例可拖 |
| 语法高亮 | 编辑器页 | 30 种语言，按扩展名识别，静态包首屏、大语言包动态加载 |
| 编码与行尾 | 状态栏 + 编码工具 | 打开时探测（BOM 优先）；保存按「打开时的编码 + 行尾 + BOM」写回 |
| 查找替换 | 查找浮窗 | 本文件 `Ctrl+F`；文件夹级 `Ctrl+Shift+F`（glob + 超大文件跳过） |
| JSON 工具 | JSON 浮窗 | 格式化（缩进可调）/ 压缩 / 校验（带行列）/ JSONPath / 转 5 种结构体 |
| 对比 | 对比浮窗 | 文本 LCS 行级 + 行内高亮；目录一层逐文件 |
| 哈希 | 哈希浮窗 | MD5 / SHA-1 / SHA-256，大小写两种输出 |
| 正则 | 正则浮窗 | 实时匹配 + 捕获组表格，`i` `m` `s` `x` 四开关 |
| 编码转换 | 编码浮窗 | 24 种编码互转，可选写 BOM |
| 行编辑 | 行编辑浮窗 | 11 种行级变换，可预览后应用 |
| 书签 / 片段 / 宏 | 侧栏面板 | `F2` 加书签；片段双击插入；宏 `Ctrl+Shift+M` 录制、`Ctrl+Shift+P` 回放 |
| 函数列表 | 侧栏面板 | 按语言正则提取符号，点击跳转 |
| 列编辑 | 编辑器 | `Alt+C` 切列模式，矩形选区逐行同列插入 | 
| 小地图 | 编辑器 | 配置开关；canvas 只画轮廓不画字，虚拟滚动跟随 |
| 剪贴板历史 | 剪贴板浮窗 | 编辑器内复制 / 剪切攒 50 条，点击回插 |
| Hex 查看 | Hex 视图 | 16 字节一行（偏移 / hex / ASCII），只渲染可视区 |
| 文档预览 | 预览视图 | 按扩展名分流 xlsx / docx / pdf / 图片 / Markdown / 纯文本 |
| 批量查找替换 | 批量浮窗 | 目录 + glob，**先扫描预览再勾选应用**，不直接改磁盘 |
| 批量重命名 | 批量浮窗 | 查找替换 / 加序号 / 大小写三种模式，冲突项标红不可选 |
| 查找结果 | 侧栏面板 | 目录级查找命中常驻，点击跳文件 + 行 |
| 文件监控 | 侧栏面板 | `tail -f` 当前文件，只读增量 |
| 会话恢复 | 壳层 | 启动恢复标签 / 光标 / 窗口大小；未保存草稿也恢复 |
| 托盘与单实例 | 壳层 | 关闭到托盘；二次启动转交文件路径 |
| 备份 | 设置 → 关于 | 导出 / 导入书签、片段、宏、草稿、最近文件 |
| 最近文件 | 菜单 + 欢迎页 | `Ctrl+Shift+O`；可钉住、显示所在目录、移除、清空 |
| 大文件降级 | 编辑器页 | 超过 32MB 自动只读前段，状态栏标「已截断」，**拦截写回** |
| 全屏 | 菜单 | `F11`；走窗口 API，标题栏一起消失 |
| 打印 | 菜单 | `Ctrl+P`；只印正文，界面骨架不印，语法高亮转黑 |

**界面只做「用户每天都做的那一件事」**：不自研插件系统、不做多窗口、不做云同步。
这条边界让功能面收得住，代价是高级需求要靠外部工具。

## 模块地图

```
┌──────────────────────────────────────────────────────────────┐
│ 前端 Vue 3 + TS：布局五层 / Pinia store / CodeMirror          │
└───────────────────────────┬──────────────────────────────────┘
              invoke（无 HTTP、无端口）+ emit 事件
┌───────────────────────────▼──────────────────────────────────┐
│ commands    薄 handler + 系统能力绑定（唯一 import tauri 的层） │
│ service     编排：file / editor / tools / userdata            │
│ repo        一张表一个 repo（rusqlite）                       │
│ domain      DO / REQ / VO（三层禁止内联结构体）               │
│ db          SQLite 打开 + 迁移（WAL）                         │
│ 能力域      app（路径 / 装配 / 生命周期）· shell（壳层）        │
│ pkg         叶子工具包（AppError / paths / log / id / text）  │
└──────────────────────────────────────────────────────────────┘
```

| 模块 | 负责什么 | 规格 |
|---|---|---|
| `src-tauri/src/app` | 路径解析、装配（组合根）、生命周期 | [04](specs/04-desktop-shell.md) |
| `src-tauri/src/shell` | 单实例、托盘、关闭策略 | [04](specs/04-desktop-shell.md) |
| `src-tauri/src/service/file.rs` | 读 / 写 / 目录树 / 编码探测 / 原子写 / 监控 | [05](specs/05-file-domain.md) |
| `src-tauri/src/service/editor.rs` | 语言清单、注释切换、行编辑、行列换算 | [06](specs/06-editor-domain.md) |
| `src-tauri/src/service/tools.rs` | JSON / Diff / 哈希 / 正则 / 迷你正则引擎 | [09](specs/09-tools.md) |
| `src-tauri/src/service/userdata.rs` | 书签 / 片段 / 宏 / 草稿 / 最近 / 配置 / 备份 | [08](specs/08-user-data.md) |
| `src-tauri/src/service/encoding.rs` | 13 种编码的探测 / 解码 / 编码 / BOM | [05](specs/05-file-domain.md) |
| `src-tauri/src/service/batch.rs` | 目录级查找替换 / 批量重命名（两段式） | [13](specs/13-batch-operations.md) |
| `frontend/src/components/editor` | CodeMirror 容器 + compartments | [06](specs/06-editor-domain.md) |
| `frontend/src/composables` | 命令表 / 主题 / 语言 / 宏 / 列编辑 / 退出守卫 | [07](specs/07-frontend-state.md) |

依赖方向是单向的：`commands → service → repo → domain → pkg`，
由 `cargo clippy` 与人工对表把关。**能力域之间不互引**，
要合作就在 `commands` 里编排——一互引，独立测试与独立演进同时作废。

## 两个真相源

| 根 | 谁写 | 内容 |
|---|---|---|
| 用户磁盘上的文件 | **用户 / 其他程序** | 正文、编码、行尾、mtime |
| `%APPDATA%\NoteText\` | **NoteText** | SQLite（书签 / 片段 / 宏 / 草稿 / 最近 / 配置）、日志、备份 |

**不能有第三本账**。文件内容是磁盘的，NoteText 只读它、按用户指令写它，
不在数据库里存一份正文副本（草稿快照除外，那是进程猝死的兜底）。
配置是数据库的，界面只读镜像 + 派生量。

「再存一份」的代价是两处需要失效，任何一处漏失效都会让界面与磁盘对不上——
表现为「明明保存了，重开却是旧内容」。

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

## 关键取舍

| 决定 | 换来了什么 | 代价 |
|---|---|---|
| Tauri 而不是 Wails / Electron | 前端是熟悉的 Vue 生态；包体 ~5MB | 要学 Tauri 的权限模型（capabilities） |
| 无边框 + 自绘标题栏 | 视觉统一、可放「当前文件名」在标题栏 | 要处理拖拽区、最大化态、双击标题栏 |
| 纯 invoke，不内嵌 HTTP 服务 | 无端口、无常驻服务、无 CSP 麻烦 | 浏览器里 `npm run dev` 只能看空态 |
| 正文归编辑器，元数据归后端 | 大文件不卡；保存 / 恢复只需元数据 | 需要 `useEditorRegistry` 这层间接 |
| rusqlite bundled | 用户机器不用装 SQLite，无 CGO | 二进制 +1.5MB |
| 手写编码表 + 迷你正则引擎 | 二进制小、行为可解释可测 | 只支持 13 种编码、正则不支持 `{n,m}` |
| 单窗口 + 多标签 | 没有「窗口跑哪去了」的问题 | 不能并排看两个文件夹 |
| 不做插件系统 | 功能面收得住，没有插件兼容性负担 | 高级需求靠外部工具 |
| 配置整块 JSON 存一个 KV 键 | 前端防抖回写即可，不用算差集 | 配置量大几十倍时才需要拆（现在没有） |
