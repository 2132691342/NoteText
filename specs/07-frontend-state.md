# 07 · 前端状态与命令

## 定位

`frontend/src/composables/useCommands.ts`（命令注册）+
`useGlobalKeys.ts`（按键分发）+ `stores/commands.ts`（命令表）+
`stores/dialogs.ts`（浮窗状态）。

这一层保证**菜单、工具栏、快捷键三个入口走到同一个命令 id**。

## 设计

### 命令表

```ts
interface Command {
  id: string                    // 全局唯一，如 'file.save'
  group: MenuGroup              // 十一个分组之一（stores/commands.ts 的 MENU_GROUPS）
  labelKey: string              // i18n key
  key?: string                  // 默认组合键（Tauri accelerator 格式）
  run: () => void | Promise<void>
  enabled?: () => boolean       // 置灰判据
}
```

启动时 `registerCommands()` 一次性注册。菜单（`MenuBar.vue`）按 `group` 分组渲染，
工具栏取高频的几个，快捷键查表——**三处都不写死功能**。

反过来做（先在菜单写死一个动作）就会出现「菜单能用的功能按键按不了」。

当前 108 条命令：`file` 18 / `edit` 30 / `search` 5 / `view` 12 /
`encoding` 6 / `language` 2 / `archive` 6 / `tools` 24 / `compare` 3 / `help` 2。
`settings` 与 `help` 两组不进下拉（它们是路由目标，渲染成菜单条右侧入口）。

**键位表与命令表必须一致**：命令自身声明的 `key` 是真相源，
`DEFAULT_KEYBINDINGS` 只做设置页展示与兜底。两边漂移的后果是
「菜单里写着、按了没反应」——加命令时同一次提交里把键位也加上（或明确不加）。

### 按键分发

```
keydown → toAccelerator(e) → 查 keybindings 覆盖 → 查命令自身 key → 查 DEFAULT_KEYBINDINGS
        → 编辑器内？编辑器绑过的组合让给它 → 原生输入框只放行到不了文本的组合
        → preventDefault + commands.run(id)
```

| 细节 | 理由 |
|---|---|
| 用 `e.code` 不用 `e.key` | 同样按 `/`，不同键盘布局 `e.key` 可能不同；`Slash` 是物理位置，与默认键位表一致 |
| 覆盖优先于默认 | 用户在设置里改过键位 |
| 编辑器真绑过的组合让给编辑器 | 单键、`Shift+` 组合、`Ctrl+[` `Ctrl+]` 是 CodeMirror 的键位，抢过来会双发 |
| 原生输入框只放行带 Ctrl 的组合 | 否则侧栏搜索框打不了字 |
| 只读预览面放行 Ctrl+C/X/A/V | 命令会去操作看不见的编辑器选区，等于什么都没复制 |
| 必须 `preventDefault` | `Ctrl+S` 会被 WebView 当成「保存网页」 |
| `keybindings` 缺字段要兜底空对象 | `Object.keys(undefined)` 抛 TypeError 会让**全部**快捷键失灵 |

### 快捷键录制（设置页）

点击键位框 → `editing = id` → 捕获下一次按键作为新键位。

**冲突即提示，不静默覆盖**：同一组合键已分配给别的命令时，
`toast.warn` 给出冲突的那一个。静默覆盖会让用户「改了没反应」还不知道为什么。

### 浮窗状态

```ts
const dialogs = useDialogStore()
dialogs.open('json', seedText, extra)
```

**为什么用 store 而不是各处 `ref`**：工具栏、菜单、快捷键、右键菜单都要能开浮窗，
而浮窗本体挂在 `EditorView` 的最外层。中间隔了编辑器滚动容器，
只能靠共享状态。

`seedText` 让浮窗打开时带上编辑器里选中的内容——
「看到结果还要手动复制」是最烦的一步。**种子一律从注册表视图读选区**
（`selectionSeed()`），不读 DOM 焦点：右键菜单与浮窗按钮都握着焦点，
`focusedTabId()` 此刻是 null，按焦点读「选中→右键→查找」预填就是空的。

### 编辑器动作通道

命令层通过 `useEditorActions` 直接函数调用操作正文（内部用 `tabId` 从
`useEditorRegistry` 定位视图），**不经过事件总线**——同一个标签可能有
两个视图（分屏），按 tabId 定位才能落在正确的那个上。

组件向外的通知（状态栏 / 工具栏消费）走 document 自定义事件，
跨标签的都带 `tabId` 防止分屏与多标签串位
（`bookmarks-changed` / `nt-marks-changed` 是全局通知，不带 tabId）：

| 事件 | 方向 | 用途 |
|---|---|---|
| `nt-cursor` | 编辑器 → 状态栏 | 光标位置（行列 + 选区长度） |
| `nt-stats` | 编辑器 → 状态栏 | 行数 |
| `nt-history` | 编辑器 → 工具栏 | 撤销可用性 |
| `nt-editor-ready` | 编辑器 → 命令层 | 实例挂载完成 |
| `nt-reload-doc` | 命令层 → 编辑器 | 让组件替换正文（重新载入） |
| `bookmarks-changed` | 任意 → 书签面板 / gutter | 书签集合变了 |
| `nt-marks-changed` | 动作层 → 编辑器 | 5 色标记变了，重绘装饰 |
| `nt-drag-state` | `useFileDrop` → 编辑区 | 拖放提示层显隐 |

**事件名靠约定**，改事件名编译器不报错——所以事件只用于「组件 → 组件」的
松耦合通知，跨边界的动作一律走命令表或 `useEditorActions`。

## 约束

- 新增功能**先注册命令**，再加菜单 / 工具栏 / 快捷键入口；
- 命令的 `run` 里不写 UI 细节（不直接操作 DOM），交给事件或 store；
- 异步命令必须自己处理失败（`toast.bad`），不往上抛——
  抛出去会被 Vue 的 errorHandler 吞成「界面出现异常」；
- 命令表注册后不再改（`register` 只在启动时调一次）。

## 取舍

| 选择 | 优势 | 代价 |
|---|---|---|
| 不引 Pinia 之外的全局状态库（Vuex / 事件总线库） | store 就是模块级单例，跨组件通信自定义事件更直接 | 事件名靠约定，改事件名编译器不报错 |
| 快捷键用字符串而非 `KeyboardEvent.code` 枚举 | 可序列化（存进配置表）、可读（设置页直接显示）、可与 Tauri accelerator 互转 | 拼写要人工对：`Control+Shift+F` 写错不会报错，只是按不出来 |
| 命令表启动时一次注册、之后不改 | 三入口（菜单 / 工具栏 / 快捷键）读同一份，不存在漂移 | 动态命令（比如按当前文件类型变样）要自己判断 `enabled()` |
| 正文不进 store | 大文件不卡；保存与恢复只传元数据 | 需要 `useEditorRegistry` 这层间接，取正文要走 `getText(tabId)` |
