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
  group: 'file' | 'edit' | 'view' | 'tools' | 'help'
  labelKey: string              // i18n key
  key?: string                  // 默认组合键（Tauri accelerator 格式）
  run: () => void | Promise<void>
  enabled?: () => boolean       // 置灰判据
}
```

启动时 `registerCommands()` 一次性注册。菜单（`MenuBar.vue`）按 `group` 分组渲染，
工具栏取高频的几个，快捷键查表——**三处都不写死功能**。

反过来做（先在菜单写死一个动作）就会出现「菜单能用的功能按键按不了」。

当前 55 条命令：`file` 10 / `edit` 17 / `view` 11 / `tools` 16 / `help` 1。

**键位表与命令表必须一致**：`DEFAULT_KEYBINDINGS` 里出现未注册的命令 id 时，
用户按下那组键什么都不会发生，且没有任何编译期或运行期报错。
两边的一致性靠人工核对——`useCommands` 是唯一注册点，加命令时同一次提交里
把键位也加上（或明确不加）。

### 按键分发

```
keydown → isTyping(target)? → 放行
        → toAccelerator(e) → 查 keybindings 覆盖 → 查 DEFAULT_KEYBINDINGS
        → preventDefault + commands.run(id)
```

| 细节 | 理由 |
|---|---|
| 输入框内不抢按键 | 用户在查找框里打字会触发编辑器命令 |
| `Esc` 永远放行 | 关闭浮窗不该被输入状态挡住 |
| 用 `e.code` 不用 `e.key` | 同样按 `/`，不同键盘布局 `e.key` 可能不同；`Slash` 是物理位置，与默认键位表一致 |
| 覆盖优先于默认 | 用户在设置里改过键位 |
| 必须 `preventDefault` | `Ctrl+S` 会被 WebView 当成「保存网页」 |

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
「看到结果还要手动复制」是最烦的一步。

### 状态归属

一条判据：**这个状态改了，需要通知后端吗？**

| 状态 | 归属 |
|---|---|
| 标签正文 | CodeMirror 实例 |
| 标签元数据 | `stores/editor` |
| 目录树展开状态与子项缓存 | `stores/fileTree` |
| 配置 | 后端 SQLite 为权威，`stores/settings` 是镜像 + 派生量 |
| 命令表 | `stores/commands`（注册一次后只读） |
| 当前浮窗 | `stores/dialogs` |
| 轻提示队列 | `stores/toast` |

### 编辑器命令通道

命令层不直接操作 CodeMirror（它拿不到实例），改用自定义事件：

```ts
document.dispatchEvent(new CustomEvent('editor-cmd', { detail: { cmd, payload } }))
```

| 事件 | 方向 | 用途 |
|---|---|---|
| `editor-cmd` | 命令 → 编辑器 | undo / redo / replaceSelection / duplicateLine |
| `editor-replace-doc` | 工具 → 编辑器 | 整体替换正文（JSON 格式化结果写回） |
| `editor-select-range` | 查找 → 编辑器 | 选中 `[from, to)` 并滚动到可见 |
| `editor-replace-range` | 查找 → 编辑器 | 替换一段 |
| `editor-goto-line` | 面板 → 编辑器 | 跳到某行（书签 / 函数列表 / 转到行） |
| `editor-save-as` | 编码工具 → 命令层 | 带目标编码写盘 |
| `editor-cursor-pos` | 编辑器 → 状态栏 | 光标位置 |
| `editor-stats` | 编辑器 → 状态栏 | 行数 |
| `editor-history` | 编辑器 → 工具栏 | 撤销可用性 |

**为什么用事件而不是 provide/inject**：命令层在 `composables/`，
编辑器在 `components/editor/`，两者没有父子关系。
事件总线是 Vue 应用里最松的耦合，且不引入额外依赖。

## 约束

- 新增功能**先注册命令**，再加菜单 / 工具栏 / 快捷键入口；
- 命令的 `run` 里不写 UI 细节（不直接操作 DOM），交给事件或 store；
- 异步命令必须自己处理失败（`toast.bad`），不往上抛——
  抛出去会被 Vue 的 errorHandler 吞成「界面出现异常」；
- 命令表注册后不再改（`register` 只在启动时调一次）。

## 取舍

**不引 Pinia 之外的全局状态库**（Vuex / mitt）：Pinia 的 store 就是模块级单例，
而跨组件通信用自定义事件更直接。引 mitt 只为了 `emit/on` 两个函数不值得。

**快捷键用字符串而不是 `KeyboardEvent.code` 枚举**：字符串可序列化
（存进配置表）、可读（设置页直接显示）、可与 Tauri 的 accelerator 格式互转。
