# 03 · 配置系统

## 定位

界面偏好的唯一真相源是 SQLite `settings` 表的一个 KV 键
（`app_config` → `AppConfig` 整块 JSON）。`stores/settings` 是它的镜像 + 派生量。

## 设计

### 为什么整块存一个键

| 方案 | 代价 |
|---|---|
| 每项一行 KV | 前端每次改动要算差集，后端要支持部分更新 |
| 整块 JSON（选用） | 配置总量几十项，整块 JSON 只有几 KB；前端防抖回写即可 |

配置量大几十倍时才需要拆，现在没有。

### 分层

```
config.json（预留，固定项）      启动期读，DB 之前就要有的东西
  └─ 目前没有内容：日志级别之类也在 DB 里，DB 打不开时本来就要 fail-fast

settings 表 app_config 键       界面偏好，改一项整块回写
```

### 派生量不落库

| 派生量 | 怎么算 |
|---|---|
| `resolvedTheme` | `theme === "system"` 时跟随 `prefers-color-scheme`，实时更新 |
| `--nt-fs-scale` | `ui.zoom_level / 100`，只写 CSS 变量 |
| `--nt-vw-scale` | 窗口宽 ÷ 1440，上下限 0.8~1.35 |

这三个都只写 CSS 变量，不进配置——进配置就要考虑「用户改了这个，
那个要不要跟着变」，而派生量的定义就是「跟着变」。

### 回写策略

```ts
function update(patch: Partial<AppConfig>) {
  config.value = { ...config.value, ...patch }   // 立即生效（响应式）
  applyTheme(); applyZoom()                       // 立即落地（CSS 变量）
  scheduleSave()                                  // 防抖 400ms 回写
}
```

**立即生效 + 防抖回写**：拖动字号滑块时不会每像素都过一次 IPC，
而界面已经跟着变了。`save()` 失败时弹提示，不静默——
用户改了设置却发现没保存，比「保存失败」更烦。

### 快捷键覆盖

`config.keybindings` 只存**覆盖**（`{ command_id: "Control+Alt+S" }），
默认值在前端 `stores/commands.ts` 的 `DEFAULT_KEYBINDINGS`。

后端不该知道键位：它是纯界面关注点。这样默认键位表可以在前端自由调整
（加命令、改默认），不需要动数据库 schema。

### 配置项全表

四组，共 44 项。**每一项都在设置界面有对应控件**——没有暗藏的开关。

#### EditorConfig（18 项）

| 项 | 类型 | 默认 | 作用 |
|---|---|---|---|
| `font_size` | u32 | 13 | 编辑器正文字号（px） |
| `font_family` | String | `Consolas` | 编辑器字体 |
| `editor_theme` | String | `auto` | 代码配色主题（`auto` 跟随界面主题） |
| `tab_size` | u32 | 4 | 缩进宽度 |
| `insert_spaces` | bool | true | 软制表符 |
| `word_wrap` | bool | false | 自动换行 |
| `line_numbers` | bool | true | 行号槽 |
| `highlight_active_line` | bool | true | 高亮当前行 |
| `bracket_matching` | bool | true | 括号配对着色 |
| `bracket_pair_colors` | bool | true | 配对括号分色 |
| `indent_guides` | bool | true | 缩进参考线 |
| `show_whitespace` | bool | false | 显示空白字符 |
| `show_eol` | bool | false | 行尾标记 |
| `fold_gutter` | bool | true | 折叠槽 |
| `minimap` | bool | true | 小地图 |
| `auto_save` | bool | false | 自动保存 |
| `auto_save_interval` | u32 | 60 | 自动保存间隔（秒） |
| `highlight_selection` | bool | true | 选中即高亮 |

#### FileConfig（8 项）

| 项 | 类型 | 默认 | 作用 |
|---|---|---|---|
| `default_encoding` | String | `auto` | 新建文件的编码（`auto` = 探测） |
| `auto_detect_encoding` | bool | true | 打开时探测编码 |
| `new_line_ending` | String | `system` | 新建文件的行尾（跟随平台） |
| `ensure_final_newline` | bool | false | 保存时补末尾换行 |
| `trim_trailing_ws` | bool | false | 保存时去行尾空白 |
| `ignore_patterns` | Vec<String> | `node_modules` `.git` `target` `dist` | 文件树忽略规则 |
| `remember_last_folder` | bool | true | 记住上次打开的文件夹 |
| `last_folder` | String | 空 | 上次的文件夹（由上一项决定是否更新） |

#### UIConfig（12 项）

| 项 | 类型 | 默认 | 作用 |
|---|---|---|---|
| `show_menu_bar` | bool | true | 菜单栏显隐 |
| `show_toolbar` | bool | true | 工具栏显隐 |
| `show_status_bar` | bool | true | 状态栏显隐 |
| `show_sidebar` | bool | true | 侧栏显隐 |
| `sidebar_width` | u32 | 260 | 侧栏宽度（px，180–480） |
| `toolbar_icon_size` | u32 | 24 | 工具栏图标尺寸 |
| `zoom_level` | u32 | 100 | 界面缩放百分比（50–200） |
| `close_to_tray` | bool | false | 关闭到托盘 |
| `restore_session` | bool | true | 启动恢复上次会话 |
| `recent_limit` | u32 | 20 | 最近文件保留条数 |
| `status_bar_items` | BTreeMap | 空 = 全显 | 状态栏各项显隐 |
| `sidebar_tab` | String | `files` | 侧栏当前页签 |

#### 顶层（6 项）

| 项 | 类型 | 默认 | 作用 |
|---|---|---|---|
| `theme` | String | `system` | `light` / `dark` / `system` |
| `language` | String | `zh-CN` | 界面语言 |
| `editor` | EditorConfig | — | 见上 |
| `file` | FileConfig | — | 见上 |
| `ui` | UIConfig | — | 见上 |
| `keybindings` | BTreeMap | 空 | 快捷键覆盖 |

`last_folder` 与 `sidebar_tab` 由用户操作写入：前者跟着「打开文件」，
后者跟着「切页签」。它们落在配置里是因为重启后要回到原处。

## 约束

- 读配置失败（首次启动 / 浏览器预览）时用默认值，**不当错误弹窗**：
  用户看到的是「设置是默认的」，而不是「应用坏了」；
- 反序列化失败（旧版本字段类型变了）时回退默认值并记 warn；
- 「恢复默认」只重置配置，不动 `keybindings` 里用户自己改的部分——
  那一份是用户显式改的，恢复默认时该保留。

## 取舍

**配置不分层存**（固定项 / 界面偏好）在 NoteText 里没有收益：
DB 打不开时本来就要 fail-fast，不存在「DB 没起来但日志要先配好」的场景。
真要分（比如日志级别要在 DB 之前生效），再拆 `config.json`。
