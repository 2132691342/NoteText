# 06 · 编辑器域

## 定位

编辑器的正文在 CodeMirror 实例里（`composables/useEditorRegistry.ts`），
后端 `service/editor.rs` 提供「按语言给注释符」与「批处理一段文本」的纯逻辑。
两边共用同一份语言清单，不一致的后果是恢复出来的标签用错高亮。

## 设计

### 分工

| 谁 | 负责 |
|---|---|
| CodeMirror 实例 | 正文、撤销栈、选区、滚动 |
| `stores/editor` | 标签元数据（路径 / 编码 / 光标 / 脏标记 / 分屏） |
| `service/editor.rs` | 语言清单、注释切换、行编辑、行列换算 |
| `service/encoding.rs` | 编码与行尾转换 |

**为什么正文不进 store**：每次按键都同步进 store 会让 Vue 的响应式系统
处理整篇文档的 diff，几百 KB 的日志文件直接卡死。需要正文的地方
（保存、统计、工具箱）通过 `getText(tabId)` 取。

### 语言清单

30 种语言，每种带：id、显示名、关联扩展名、行注释符、块注释起止。
与前端 `stores/editor.ts` 的 `EXT_LANG` 表同构。

**为什么两边各一份**：后端按扩展名判语言用于会话恢复时记 `language` 字段，
前端按同一张表选 CodeMirror 高亮。合成一份要引入共享模块，
而两边的表示形式不同（Rust struct vs TS record），合成反而更难同步。

### 注释切换

```
toggle_line_comment(language, text)
  ├─ 该语言没有行注释符 → 报错（不是静默返回原文）
  ├─ 统计非空行数与已注释行数
  ├─ 全注释 → 去掉；否则 → 补上
  ├─ 已注释的行在「补」模式下跳过（半注释块补齐，不出 // // 双层）
  └─ 空行不动（注释空行只会让 diff 多噪音）
```

三个边界都有测试守护（`test_text_pipeline_chain`）：
全注释块去掉、半注释块补齐、空行不动。
**「未注释」的判定只看非空行**：空行没有注释可言，
把它算进分母会让「注释掉一大半 + 夹一个空行」的块被判成已全注释，
结果越点注释越多。

### 行编辑（11 种）

`apply_line_op(op, text)`：`remove_blank` / `trim_trailing_ws` / `to_upper` /
`to_lower` / `to_title` / `invert_case` / `squeeze_blank` / `reverse` /
`dedup` / `sort_asc` / `sort_desc`。

排序忽略大小写：全大写的配置项与小写的混在一起时，
大小写敏感排序会把它们分成两截，用户找不到东西。

去重保留首次出现顺序：按字典序去重会让用户看不出原来在哪。

### 行列换算

```
line_col_at(text, byte_offset) -> (line, col)     1 基
offset_at_line_col(text, line, col) -> Option<usize>
```

**按 `char_indices` 而不是字节索引**：多字节字符按字节算会把一个汉字算成三列，
用户报「光标位置不对」实际是这里错了。

`offset_at_line_col` 的目标列超出该行长度时返回行尾：
与编辑器的宽容行为一致（用户输 999 想到文件末尾，不该报错）。

### 脏标记

**按「与载入时快照不同」判**，不是按「有没有输入过」：
用户改了又改回来，标签该是干净的。`CodeEditor.vue` 的
`updateListener` 里比较 `vu.state.doc.toString() !== baseline`。

## 前端编辑器容器

### compartments

每个关注点一份，切换只 `reconfigure` 不销毁重建：

| compartment | 对应配置 |
|---|---|
| `themeComp` | `resolvedTheme`（引用 CSS 变量，不写死色值） |
| `langComp` | `tab.language` |
| `editableComp` | 分屏的副视图（只读） |
| `wrapComp` | `editor.word_wrap` |
| `tabSizeComp` | `editor.tab_size` |
| `lineNumComp` | `editor.line_numbers` |
| `foldComp` | `editor.fold_gutter` |

### 语言加载

| 语言 | 加载方式 |
|---|---|
| js / ts / json / html / css / xml / markdown / sql / yaml / vue | 静态引入（常见且体积小） |
| rust / go / java / python / cpp / php | 动态 `import()`（语言包较大） |
| csharp / ruby / kotlin / shell / powershell / ini / toml / diff | legacy modes（`StreamLanguage`） |

首屏用 `syncExtensions()` 拿静态部分，异步包加载完再 `reconfigure`——
不会因为等语言包而白屏。

### 切换标签不销毁

组件按 `tab.id` 缓存（`v-for` + `key`），切回来保留实例与撤销历史。
挂载后按 `tab.position` 选中、按 `scroll_ratio` 滚动（下一帧执行，等 DOM 布局）。

### 宏录制与回放

`composables/useEditorMacro.ts`。录制期间收集两类步骤：

| 步骤 | 来源 |
|---|---|
| `text` | 文档变更时插入的文本（连续输入合并成一步） |
| `key` | 录制期执行的命令 id |

**光标移动不录**：回放时每一步都会重算光标，录进去只会让宏变脆。

回放期间置 `playing` 守卫，阻断录制钩子——否则回放产生的变更会被自己录进去。

### 列编辑模式

`composables/useEditorColumnMode.ts`。`Alt+C` 切换，开启时把当前选区
转成逐行选区，后续输入落在每行同列。

**从后往前改**：先改行号大的，否则前面的插入会让后面所有行的偏移失效。
这是所有「多行同时编辑」实现里最容易错的一点。

### 小地图

`components/editor/Minimap.vue`。开关接 `editor.minimap`。

只画「每行非空内容的缩进与长度」，不渲染文字——几万行的日志用 canvas
画字必掉帧，而缩略图本来就只看形状。刷新按 rAF 节流，
滚动事件每秒几十次，每次重画没有意义。

### 剪贴板历史

`composables/useClipboardHistory.ts`。从编辑器的 `copy` / `cut` 事件收集，
上限 50 条，**不碰系统剪贴板**（Webview 里读系统剪贴板要权限，
而用户要的是「刚才复制过的那几段」，自己攒就够了）。

### 视图分流

`EditorArea.vue` 按 `view_type` 与扩展名分发，不让二进制进 CodeMirror：

| 条件 | 容器 |
|---|---|
| `view_type = 'hex'` | `HexView.vue`（虚拟滚动，只渲染可视区） |
| 扩展名属于 xlsx / docx / pdf / 图片 / md | `DocPreview.vue`（动态 import 对应预览器） |
| 其余 | `CodeEditor.vue` |

预览器全部动态 `import`：一个只写纯文本的人不该付 xlsx 的钱。

## 约束

- **CodeMirror 的主题必须引用 CSS 变量**：引 `@codemirror/theme-one-dark`
  会写死色值，而我们的纪律是「色值只在 `themes.css` 定义」；
- 编辑器里只允许四处高亮（见 [DESIGN.md](../DESIGN.md)）：
  选中、当前匹配、其余匹配、选中即高亮。书签行是第五处（警告色竖条）；
- 查找高亮用 `.cm-searchMatch` 系列类名，不用 `Decoration` 自己画——
  那会让 CodeMirror 的搜索面板与我们的高亮不一致。

## 取舍

**不引 tree-sitter 做符号提取**：真正的 AST 解析器体积大且要 wasm，
而「跳到函数」这个需求用每种语言的正则就够。误判的代价是列表里多几项，
用户点一下发现不对就换一个——可接受。函数列表面板因此标着
「这个文件里没有识别到函数」而不是「解析失败」。

**不引 Vim / Emacs 键位模式**：目标用户是 Windows 桌面用户，
 modal editing 的学习成本与收益不成比例。
