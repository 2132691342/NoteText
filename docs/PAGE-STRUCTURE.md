# 页面与组件地图

NoteText 是单窗口应用，只有一个常驻视图（编辑器）+ 两个路由视图（设置 / 帮助）。
所有工具箱都是挂在最外层的浮窗，不是路由。

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
EditorView.vue
├── TitleBar.vue          标题栏 32px：品牌 + 当前文件名 + 窗控
├── MenuBar.vue           菜单栏 32px：五个一级菜单（数据来自命令表）
├── ToolBar.vue           工具栏 34px：高频图标钮 + 「更多」下拉
├── SideBar.vue           侧栏：五个面板的容器，宽度可拖
├── TabBar.vue            标签栏 32px：标签 + 脏标记 + 关闭钮
├── EditorArea.vue        编辑区：按 view_type 分发 + 分屏 + 拖放提示
│   └── CodeEditor.vue    CodeMirror 容器（compartments）
└── StatusBar.vue         状态栏 26px：修改指示 / 光标 / 语言 / 编码 / 行尾 / 缩放
```

### 各层的数据来源

| 组件 | 读什么 store | 关键交互 |
|---|---|---|
| TitleBar | `editor`（活跃标签）、`settings` | 窗控走 `api.windowMinimize/ToggleMaximize/Close` |
| MenuBar | `commands`（分组清单）、`settings`（键位覆盖） | 已打开时滑过即切换；下拉 Teleport 到 body |
| ToolBar | `editor`（撤销可用性、脏标记）、`settings` | 「更多」下拉开工具箱浮窗 |
| SideBar | `settings`（宽度 / 页签）、`fileTree` | 页签切换只改 `ui.sidebar_tab`；宽度拖拽 180–480 |
| TabBar | `editor` | 中键关闭；右键菜单（关闭其他 / 已保存 / 复制路径） |
| EditorArea | `editor` | `Ctrl+\` 分屏；拖放打开走 `useFileDrop` |
| StatusBar | `editor`、`settings` | 光标 / 统计由编辑器组件发自定义事件推 |

### 为什么布局五层不合并

标题栏与菜单栏理论上能合成一行，但分开放有两个实在理由：

1. **标题栏要可拖拽**（`data-tauri-drag-region`），菜单栏不要——合并后
   整个菜单条都会变成拖拽区，点菜单要先「抢」回来；
2. 高度令牌分开（32 / 32），用户可以在设置里单独关掉菜单栏省一行，
   合并后只能一起关。

## 侧栏五个面板

| 面板 | 组件 | 数据 |
|---|---|---|
| 文件 | `panels/FileTreePanel.vue` | `read_dir` + 懒展开；递归组件自引用 |
| 书签 | `panels/BookmarkPanel.vue` | `list_bookmarks`（后端带该行正文） |
| 片段 | `panels/SnippetPanel.vue` | `list_snippets`（语言 + 关键字在后端过滤） |
| 函数 | `panels/FunctionPanel.vue` | 前端正则提取符号（不引 tree-sitter） |
| 监控 | `panels/MonitorPanel.vue` | `read_file` 行区间增量轮询（800ms） |

## 工具箱浮窗

全部挂 `dialogs/DialogHost.vue`，包 `common/NtModal.vue`（遮罩 / 焦点圈 / Esc / 滚动锁）。

| 浮窗 | 组件 | 特色 |
|---|---|---|
| JSON | `dialogs/JsonToolDialog.vue` | 三种模式（格式化 / JSONPath / 转结构体）；结果可一键写回编辑器 |
| 对比 | `dialogs/DiffToolDialog.vue` | 左右可交换；行内 span 转义后包 `<span class="nt-span">` |
| 哈希 | `dialogs/HashToolDialog.vue` | 默认算编辑器内容；大小写两份输出 |
| 正则 | `dialogs/RegexToolDialog.vue` | 输入防抖 150ms 实时重算；捕获组表格 |
| 编码 | `dialogs/EncodingToolDialog.vue` | 编码 + 行尾一起转；置信度低时警告 |
| 行编辑 | `dialogs/LineOpToolDialog.vue` | 11 种操作可预览后应用 |
| 查找 / 替换 | `dialogs/FindDialog.vue`（ReplaceDialog 是薄包装） | 本文件查找 + 结果列表 |
| 文件夹查找 | `dialogs/FindInFilesDialog.vue` | 并发上限 4；每 200 个文件让出一帧更新进度 |
| 转到行 | `dialogs/GotoLineDialog.vue` | 上限取当前行数，超范围按钮置灰 |
| 关于 | `dialogs/AboutDialog.vue` | 版本 / 数据目录 / 日志目录（排障第一步） |

**为什么查找与替换共用一个组件**：交互几乎相同（同一套选项、同一套跳转），
复制会让两处逻辑漂移。ReplaceDialog 只是把默认模式设成 `replace` 的薄包装。

**为什么文件夹查找单独一个大浮窗**：它要遍历文件、要进度、结果可能上万条，
塞进同一个小浮窗会让界面卡住。

## 公共组件

| 组件 | 用途 |
|---|---|
| `common/NtIcon.vue` | 24 网格内联 SVG 图标表（40+ 个 path） |
| `common/NtModal.vue` | 浮窗统一容器 |
| `common/ToastHost.vue` | 右下角轻提示（可选中复制、悬停不计时） |

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
├── compartments（每个关注点一份）
│   ├── themeComp      useEditorTheme（引用 CSS 变量，不写死色值）
│   ├── langComp       useEditorLanguage（静态包同步、大包动态）
│   ├── editableComp   只读 / 可编辑
│   ├── wrapComp       自动换行
│   ├── tabSizeComp    制表符宽度
│   ├── lineNumComp    行号
│   ├── foldComp       代码折叠
│   └── historyComp    历史
└── EditorView.updateListener
    ├── 脏标记按「与载入时快照不同」判
    ├── editor-cursor-pos → 状态栏
    ├── editor-stats      → 状态栏
    └── editor-history    → 工具栏撤销可用性
```

**切换标签不销毁实例**：那样会丢撤销历史。组件按 `tab.id` 缓存，
由 Vue 的 `v-for` + `key` 保证。

**恢复光标与滚动**：挂载后按 `tab.position` 选中、按 `scroll_ratio` 滚动，
下一帧再执行（等 DOM 布局完成）。
