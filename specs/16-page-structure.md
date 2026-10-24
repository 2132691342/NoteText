# 16 · 页面与组件地图

NoteText 是单窗口应用，只有一个常驻视图（编辑器）+ 两个路由视图（设置 / 帮助）。
所有工具箱都是挂在最外层的浮窗，不是路由。

**标题栏挂在 `App.vue` 而不是编辑器页**：设置页与帮助页也要能最小化 / 关闭，
否则用户进了设置就没有鼠标出口（无边框窗口没有系统窗控）。

## 路由

| 路径 | 视图 | 说明 |
|---|---|---|
| `/` | `views/EditorView.vue` | 编辑器主界面，唯一的常驻视图 |
| `/settings/:tab?` | `views/SettingsView.vue` | 六个分页：general / appearance / editor / file / keybindings / about |
| `/help/:name?` | `views/HelpView.vue` | 内置手册（`assets/docs/*.md` 按需加载） |

**hash 模式**：Tauri 的 asset 协议下没有服务端做重写，history 模式刷新会 404。
设置用 `/settings/:tab` 而不是子路由，刷新与前进后退都能停在原处。

## 编辑器页（EditorView）

```
App.vue
└── TitleBar.vue          标题栏 32px：品牌 + 当前标题 + 窗控（全路由共用）
    └── EditorView.vue
├── MenuBar.vue           菜单栏 32px：十个一级菜单（数据来自命令表）
├── ToolBar.vue           工具栏 34px：高频图标钮 + 「更多」下拉
├── SideBar.vue           侧栏：六个面板的容器，宽度可拖
├── TabBar.vue            标签栏 32px：标签 + 脏标记 + 关闭钮
├── EditorArea.vue        编辑区：按 view_type 分发 + 分屏 + 拖放提示
│   └── CodeEditor.vue    CodeMirror 容器（compartments）
└── StatusBar.vue         状态栏 26px：修改指示 / 光标 / 语言 / 编码 / 行尾 / 缩放
```

### 各层的数据来源

| 组件 | 读什么 store | 关键交互 |
|---|---|---|
| TitleBar | `editor`（活跃标签）、`settings` | 窗控走 `api.windowMinimize/ToggleMaximize/Close` |
| MenuBar | `commands`（分组清单）、`settings`（键位覆盖） | 已打开时滑过即切换；下拉 Teleport 到 body；空分组不进菜单条 |
| ToolBar | `editor`（撤销可用性、脏标记）、`settings` | 「更多」下拉开工具箱浮窗 |
| SideBar | `settings`（宽度 / 页签）、`fileTree` | 页签切换只改 `ui.sidebar_tab`；宽度拖拽 180–480 |
| TabBar | `editor` | 中键关闭；右键菜单（关闭其他 / 已保存 / 复制路径） |
| EditorArea | `editor` | `Ctrl+\` 分屏；拖放打开走 `useFileDrop`；
  每个标签一个常驻 CodeMirror 实例，只在首次激活时挂载 |
| StatusBar | `editor`、`settings` | 光标 / 统计由编辑器组件发自定义事件推 |

### 为什么布局五层不合并

标题栏与菜单栏理论上能合成一行，但分开放有两个实在理由：

1. **标题栏要可拖拽**（`data-tauri-drag-region`），菜单栏不要——合并后
   整个菜单条都会变成拖拽区，点菜单要先「抢」回来；
2. 高度令牌分开（32 / 32），用户可以在设置里单独关掉菜单栏省一行，
   合并后只能一起关。

## 菜单分组

十个一级菜单的顺序由 `stores/commands.ts` 的 `MENU_GROUPS` 定义，菜单条按它渲染。
分组不是装饰：命令注册时写 `group`，菜单、快捷键设置页、冲突检测都从这一个字段取。

| 分组 | 入口数 | 内容 |
|---|---|---|
| 文件 | 18 | 新建 / 打开 / 文件夹 / 最近文件 / 保存系列 / 关闭系列（含其他、左、右、已保存）/ 重命名 / 重新加载 / 草稿管理 / 打印 / 在资源管理器中显示 |
| 编辑 | 30 | 撤销重做、剪贴板、行操作、大小写、排序去重、缩进、注释、书签系列、光标跳转历史、转到匹配括号 |
| 查找 | 5 | 查找 / 替换 / 下一个 / 上一个 / 在文件中查找 |
| 视图 | 12 | 缩放、侧栏、状态栏、换行、小地图、分屏、主题、界面语言、全屏 |
| 编码 | 6 | 当前编码、行尾、转 UTF-8（带 / 不带 BOM）、以指定编码重新打开、编码转换器 |
| 语言 | 2 | 语法语言选择、按扩展名识别 |
| 工具 | 24 | JSON / 哈希 / 正则 / 行编辑 / 格式转换 / 剪贴板 / Hex / 取色 / 日志 / 批量操作 / 宏 / 一键快操作（格式化、压缩、转义、去转义、转 YAML/TOML/XML） |
| 对比 | 3 | 文本对比、目录对比、与剪贴板对比 |

`settings` 与 `help` 两组不进下拉：它们是路由目标，渲染成菜单条右侧的入口。

「查找」从「编辑」里拆出来、「编码」「语言」「对比」从「视图」「工具」里拆出来，
是为了让最高频的几类操作各自成家——一个菜单挂 30 项时，用户每次都要从头扫一遍。

## 侧栏六个面板

| 面板 | 组件 | 数据 |
|---|---|---|
| 文件 | `panels/FileTreePanel.vue` | `read_dir` + 懒展开；递归渲染在 `FileTreeNode.vue`（自引用）；**第一层是打开的文件夹本身**（与资源管理器一致），子项由 `open()` 预读 |
| 书签 | `panels/BookmarkPanel.vue` | `list_bookmarks`（后端带该行正文） |
| 片段 | `panels/SnippetPanel.vue` | `list_snippets`（语言 + 关键字在后端过滤） |
| 函数 | `panels/FunctionPanel.vue` | 前端正则提取符号（不引 tree-sitter） |
| 查找结果 | `panels/FindResultsPanel.vue` | 复用「在文件中查找」写入的 `findResults` store，浮窗关掉后结果仍在 |
| 监控 | `panels/MonitorPanel.vue` | `read_file` 行区间增量轮询（800ms） |

## 工具箱浮窗

全部挂 `dialogs/DialogHost.vue`，包 `common/NtModal.vue`（遮罩 / 焦点圈 / Esc / 滚动锁）。

| 浮窗 | 组件 | 特色 |
|---|---|---|
| JSON | `dialogs/JsonToolDialog.vue` | 三种模式（格式化 / JSONPath / 转结构体）；**打开即自动执行**，JSONPath 与目标语言是内联控件；pretty / minify 可切；「应用」写回打开时的选区 |
| 对比 | `dialogs/DiffToolDialog.vue` | 左右可交换；行内 span 转义后包 `<span class="nt-span">` |
| 哈希 | `dialogs/HashToolDialog.vue` | 默认算编辑器内容；大小写两份输出 |
| 正则 | `dialogs/RegexToolDialog.vue` | 输入防抖 150ms 实时重算；捕获组表格 |
| 编码 | `dialogs/EncodingToolDialog.vue` | 编码 + 行尾一起转；置信度低时警告 |
| 行编辑 | `dialogs/LineOpToolDialog.vue` | 11 种操作可预览后应用 |
| 查找 / 替换 | `dialogs/FindDialog.vue`（ReplaceDialog 是薄包装） | 五个页签（查找 / 替换 / 在文件中 / 标记 / 结果）；预填上次查询词；不可搜视图给说明条；替换全部一次事务提交 |
| 文件夹查找 | `dialogs/FindInFilesDialog.vue` | 并发上限 4；每 200 个文件让出一帧更新进度 |
| 转到行 | `dialogs/GotoLineDialog.vue` | 上限取当前行数，超范围按钮置灰 |
| 剪贴板历史 | `dialogs/ClipboardDialog.vue` | 编辑器内的复制 / 剪切攒 50 条，点击回插 |
| 十六进制 | `dialogs/HexViewerDialog.vue` | 主体复用 `editor/HexView.vue`，内嵌时不带遮罩 |
| 批量查找替换 | `dialogs/BatchFindReplaceDialog.vue` | 先扫描预览再勾选应用，扫完才允许写盘 |
| 批量重命名 | `dialogs/BatchRenameDialog.vue` | 三种模式；冲突项标红且不可选 |
| 最近文件 | `dialogs/RecentFilesDialog.vue` | 可钉住、显示所在目录、右键移除 / 显位置 |
| 草稿箱 | `dialogs/DraftsDialog.vue` | `draft:<id>` 伪路径的标签在这里找回正文 |
| 文本输入 / 下拉问答 | `dialogs/AskTextDialog.vue`、`dialogs/AskPickDialog.vue` | 命令层的 `askText` / `askPick` 共用浮窗（重命名 / 跳语言选择等） |
| 格式转换 | `dialogs/ConvertDialog.vue` | JSON / YAML / TOML / XML 互转；**带选区进来即自动载入并转换**；「应用」写回选区（没选区写回全文）；一键互换 |
| 目录对比 | `dialogs/DirDiffDialog.vue` | 一层目录逐文件比较；四态分类；差异排前；点行用系统程序打开 |
| 取色器 | `dialogs/ColorPickerDialog.vue` | EyeDropper 屏幕取色；HEX / RGB / HSL 三种表示分别可复制 |
| 日志查看器 | `dialogs/LogViewerDialog.vue` | 读应用日志；按级别着色与过滤；只取末 2000 行 |
| 关于 | `dialogs/AboutDialog.vue` | 版本 / 数据目录 / 日志目录（排障第一步） |

**为什么查找与替换共用一个组件**：交互几乎相同（同一套选项、同一套跳转），
复制会让两处逻辑漂移。ReplaceDialog 只是把默认模式设成 `replace` 的薄包装。

**为什么文件夹查找单独一个大浮窗**：它要遍历文件、要进度、结果可能上万条，
塞进同一个小浮窗会让界面卡住。

## 公共组件

| 组件 | 用途 |
|---|---|
| `common/NtIcon.vue` | 24 网格内联 SVG 图标表（50+ 个 path） |
| `common/NtModal.vue` | 浮窗统一容器（Esc 监听在 document 上，焦点跑到内部输入框也有效） |
| `common/ContextMenu.vue` | 右键菜单：编辑器 / 标签栏 / 文件树共用同一套条目与退场行为 |
| `common/ToastHost.vue` | 右下角轻提示（可选中复制、悬停不计时） |
| `dialogs/UnsavedDialog.vue` | 未保存确认：保存 / 不保存 / 取消 三选一，标签关闭与退出共用 |

## 设置页

左栏六个分页按钮 + 右栏表单。每个面板只管一类配置，
改动即时生效 + 防抖 400ms 回写（`stores/settings` 的 `scheduleSave`）。

| 面板 | 管什么 |
|---|---|
| `GeneralPanel` | 语言、恢复会话、关闭到托盘、最近文件数、记住文件夹 |
| `AppearancePanel` | 主题三档、界面字号缩放、界面字体 |
| `EditorPanel` | 字号 / 字体 / 制表符 / 11 个开关 / 自动保存 |
| `FilePanel` | 默认编码、行尾、保存行为、忽略规则（chip 列表） |
| `KeybindingsPanel` | 默认键位表 + 覆盖录制（冲突即提示，不静默覆盖） |
| `AboutPanel` | 版本 / 数据目录 / 日志目录 / 备份导入导出 |

**为什么设置用路由而不是弹窗**：设置内容多，弹窗装不下；
而且刷新与前进后退能停在原处，用户改到一半刷新不会丢。

## 编辑器容器（CodeEditor）

```
CodeEditor.vue
├── compartments（每个关注点一份，切换只 reconfigure 不重建视图）
│   ├── themeComp      useEditorTheme（引用 CSS 变量，不写死色值）
│   ├── langComp       useEditorLanguage（静态包同步、大包动态）
│   ├── editableComp   只读 / 可编辑
│   ├── wrapComp       自动换行
│   ├── tabSizeComp    制表符宽度
│   ├── lineNumComp    行号
│   ├── foldComp       代码折叠
│   ├── bookmarkComp   书签 gutter
│   ├── marksComp      5 色词标记装饰
│   ├── guidesComp     缩进参考线 / 括号配色 / 行尾标记
│   └── whitespaceComp 空白字符显示
└── EditorView.updateListener
    ├── 脏标记按「与载入时快照不同」判
    ├── nt-cursor（带 tabId）→ 状态栏光标
    ├── nt-stats  → 状态栏行数
    ├── nt-history → 工具栏撤销可用性
    └── 选区变化 → 选区浮条重定位（docChanged 分支也要刷，否则替换后浮条残留）
```

**切换标签不销毁实例**：那样会丢撤销历史。组件按 `tab.id` 缓存，
由 Vue 的 `v-for` + `key` 保证。

**恢复光标与滚动**：挂载后按 `tab.position` 选中、按 `scroll_ratio` 滚动，
下一帧再执行（等 DOM 布局完成）。

## 前端组合式函数

| 文件 | 职责 |
|---|---|
| `composables/useCommands.ts` | 命令表（菜单 / 工具栏 / 快捷键 / 右键菜单的唯一来源） |
| `composables/useGlobalKeys.ts` | 组合键 → 命令 id。**编辑器内也生效**：把 CodeMirror 真绑过的组合、以及剪贴板 / 全选 / 撤销（Ctrl+C/X/V/A/Z/Y）放行给 WebView 原生行为（paste 事件自带数据，不经需要权限的 `clipboard.readText`）；原生输入框（查找 / 重命名）里同样放行文本编辑组合，但 CodeMirror 正文不算输入框——功能键（F3/F2/F1/F11）、Alt+Z/C、Alt+左右在编辑器里照走命令表 |
| `composables/useEditorActions.ts` | 对正文的原子动作（撤销 / 行操作 / 插入 / 替换 / 跳转），按 tabId 定位视图 |
| `composables/useEditorRegistry.ts` | tabId → EditorView + 保存基线；脏标记按「与基线是否相同」判 |
| `composables/useCloseTab.ts` | 关闭标签的唯一入口：保存 / 另存为 / 未保存确认 |
| `composables/useAutoSave.ts` | 按配置间隔自动保存「已命名的脏标签」（未命名的不弹另存为） |
| `composables/useExternalChange.ts` | 外部变更：干净标签重载正文，脏标签只提示（每路径 600ms 去抖） |
| `composables/useFileDrop.ts` | 拖入文件开标签、拖入目录设为工作区。走 WebView 原生 `drop` 事件（`dragDropEnabled=false`，Tauri 接管会连页内 HTML5 拖拽一起掐掉）：路径取 `File.path`（Chromium 扩展），拿不到就降级读文本开无路径标签；`drop` 必拦，否则 WebView 把文件当导航目标顶掉整个界面 |
| `composables/useQuitGuard.ts` | 退出前存会话 + 草稿，逐个处理未保存标签 |
| `composables/useEditorTheme.ts` | CodeMirror 主题与语法高亮（色值一律引用 `--nt-*`） |
| `composables/useEditorLanguage.ts` | 语言包：静态 + 动态按需加载 |
| `composables/useEditorMacro.ts` | 宏录制与回放 |
| `composables/useEditorColumnMode.ts` | 列块编辑 |
| `composables/useViewportScale.ts` | 按窗口宽写 `--nt-vw-scale` |
| `composables/useMarkdown.ts` | Markdown 三态的渲染管线（HTML → Mermaid → KaTeX，递增 token 防旧图覆盖） |
| `composables/useNavHistory.ts` | 光标跳转历史（Alt+左 / 右在位置间回退前进） |
| `composables/useEditorGuides.ts` | 缩进参考线 / 括号配色 / 行尾标记三件视觉辅助 |
| `composables/useLastQuery.ts` | 上次查找条件：F3 / Shift+F3 复用，浮窗预填 |
| `composables/useQuickActions.ts` | 一键快操作：选中即用、结果原地替换（格式化 / 压缩 / 转义 / 格式互转） |
| `composables/useShellIntent.ts` | 托盘 / 二次启动的文件打开意图 |
| `composables/useClipboardHistory.ts` | 编辑器内复制文本的本地历史 |

## 取舍

| 选择 | 优势 | 代价 |
|---|---|---|
| 正文归 CodeMirror 实例、store 只存元数据 | 每次按键不再让响应式系统 diff 整篇文档，几百 KB 日志不卡 | 要正文的地方必须经 `getText(tabId)`，不能顺手从 store 取 |
| 命令（菜单 / 工具栏 / 快捷键）收敛到一处表单畸变 | 三个入口走同一份命令 id，不会「菜单能用按键按不了」 | 新功能必须先注册命令再画按钮，反过来做就会漏 |
| 布局五层各自独立（菜单 / 工具 / 标签 / 侧栏头 / 状态栏） | 每层可单独隐藏，适配小窗 | 不合并意味着多写一层容器 |
| 语言包按需动态 import | 首屏不背 30 个语言包的体积 | 首次切到某种语言有短暂加载 |
| 组件里禁止写死色值，一律 `var(--nt-*)` | 换主题、缩放整套跟随，不改代码 | 写样式时不能直接写 hex，多一层间接 |
