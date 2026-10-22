# API 契约

这一篇是前端 `api/index.ts` 与后端 `commands/` 的对照表。**改字段必须两边同步**，
并且把 `types/api.ts` 的 `CONTRACT_VERSION` +1。

## 通用约定

| 维度 | 规则 |
|---|---|
| 命名 | snake_case，与 Rust 函数名一致 |
| 出入参 | 一律 `domain` 的类型；两个以上参数用结构体而不是位置参数 |
| 成功 | 直接返回值（Tauri 的 invoke 成功态） |
| 失败 | `AppError { code, message, details }`，前端 `normalizeError` 归一 |
| 二进制 | 走 `read_binary`，出参是 base64 字符串（`Vec<u8>` 会被序列化成 `number[]`，体积约 4 倍） |
| 空数组 | 后端构造时初始化，契约说是数组就永远发 `[]`（守护见 `test_bootstrap_chain`） |

## 文件

| 命令 | 入参 | 出参 |
|---|---|---|
| `read_file` | `path`, `mode?` | `FileContentVO` |
| `read_binary` | `path` | `string`（base64，上限 20MB） |
| `write_file` | `path`, `text`, `encoding?`, `line_ending?`, `bom?` | `WriteResultVO` |
| `save_file_as` | `text`, `suggested_name?`, `encoding?`, `line_ending?`, `bom?` | `WriteResultVO \| null` |
| `read_dir` | `dir` | `DirEntry[]` |
| `stat_file` | `path` | `FileMeta` |
| `create_file` | `path` | `()` |
| `open_file_dialog` | `suggested_dir?` | `string \| null` |
| `open_folder_dialog` | `suggested_dir?` | `string \| null` |
| `save_file_dialog` | `suggested_name?` | `string \| null` |
| `detect_encoding` | `bytes: number[]` | `DetectResultVO` |
| `watch_dir` | `dir` | `()` |
| `unwatch_dir` | `dir` | `()` |
| `get_recent_files` | — | `RecentFileVO[]` |

`ReadMode` 是 tagged enum：`{ mode: "full" }` / `{ mode: "head" }` /
`{ mode: "range", start_line, lines }`。

`FileContentVO` 的 `truncated` 是「或」语义：被截断**或**编码置信度低都为 true。
前端据此提示用户，不自己分辨是哪一种。

## 编辑器

| 命令 | 入参 | 出参 |
|---|---|---|
| `get_languages` | — | `LanguageInfo[]` |
| `toggle_line_comment` | `language`, `text` | `string` |
| `apply_line_op` | `op: LineOp`, `text` | `string` |
| `goto_line` | `text`, `line` | `usize`（字节偏移） |
| `get_text_stats` | `text` | `{ lines, non_blank_lines, chars, words }` |
| `convert_case` | `path`, `text`, `target` | `string` |
| `convert_line_ending` | `path`, `text`, `target` | `string` |

`LineOp`：`remove_blank` / `trim_trailing_ws` / `to_upper` / `to_lower` /
`to_title` / `invert_case` / `squeeze_blank` / `reverse` / `dedup` /
`sort_asc` / `sort_desc`。

## 工具箱

| 命令 | 入参 | 出参 |
|---|---|---|
| `json_format` | `JsonFormatReq` | `JsonFormatVO` |
| `json_query` | `JsonQueryReq` | `JsonQueryVO` |
| `json_to_struct` | `JsonToStructReq` | `JsonToStructVO` |
| `json_diff` | `JsonDiffReq` | `JsonDiffVO` |
| `text_diff` | `TextDiffReq` | `TextDiffVO` |
| `dir_diff` | `left`, `right` | `DirDiffVO` |
| `hash_text` | `HashReq` | `HashVO` |
| `regex_test` | `RegexTestReq` | `RegexTestVO` |
| `get_encodings` | — | `EncodingInfo[]` |

`JsonFormatVO` 失败时带 `error_line` / `error_col`（1 基）：
只说「格式错误」用户找不到在哪。

`DiffLineVO` 的 `left_spans` / `right_spans` 是 `[start, end)` **字节**区间，
前端按它包 `<span class="nt-span">`；文本先转义再插 span，避免注入。

`RegexMatchVO.groups` 的索引 0 是整段匹配，1.. 是编号捕获组；
未参与匹配的组给空串（与主流引擎一致，前端按固定列表渲染）。

## 批量操作

| 命令 | 入参 | 出参 |
|---|---|---|
| `batch_scan_find` | `BatchFindReq` | `BatchFindVO` |
| `batch_apply_replace` | `BatchReplaceReq` | `BatchReplaceVO` |
| `batch_plan_rename` | `BatchRenameReq` | `BatchRenameVO` |
| `batch_apply_rename` | `RenameApplyReq` | `BatchReplaceVO` |

**两段式是硬契约**：`scan_*` / `plan_*` 只算不写，`apply_*` 才落盘。
批量改磁盘没有撤销键，用户必须在写之前看见自己要改什么。

- `BatchFindVO.hits` 每条带 `lines`（行号，最多 50 条）与 `sample`（首条命中正文）；
  `skipped` 是因过大 / 二进制 / 读失败跳过的文件数，不静默吞掉。
- `BatchReplaceVO.failures` 逐条记账：批量操作不因一个文件失败而整体回滚，
  但失败必须报出来，否则用户以为全改了。
- `RenamePlanItem.conflict` 表示目标已存在，前端标红且不可勾选；
  `apply_rename` 还会再预检一次，任一目标存在就整体拒绝（避免改一半）。
- 正则模式目前是**整行替换**：迷你引擎不支持按匹配段替换，
  前端在正则模式下提示用户。要完整语义需换 `regex` crate（AGENTS §1.1）。

## 用户数据

| 命令 | 入参 | 出参 |
|---|---|---|
| `list_bookmarks` | — | `BookmarkVO[]` |
| `list_file_bookmarks` | `path` | `BookmarkDO[]` |
| `add_bookmark` | `BookmarkAddReq` | `BookmarkDO` |
| `toggle_bookmark` | `BookmarkAddReq` | `bool`（true = 加上了） |
| `remove_bookmark` | `id` | `()` |
| `clear_bookmarks` | `path?` | `usize` |
| `list_snippets` | `language?`, `keyword?` | `SnippetVO[]` |
| `save_snippet` | `SnippetSaveReq` | `SnippetVO` |
| `remove_snippet` | `id` | `()` |
| `get_snippet_languages` | — | `string[]` |
| `snippet_by_abbrev` | `abbrev` | `SnippetDO \| null` |
| `list_macros` | — | `MacroVO[]` |
| `save_macro` | `MacroSaveReq` | `MacroVO` |
| `remove_macro` | `id` | `()` |
| `list_drafts` | — | `DraftVO[]` |
| `save_draft` | `DraftSaveReq` | `bool`（false = 超上限没存） |
| `remove_draft` | `id` | `()` |
| `touch_recent` | `path` | `()` |
| `set_recent_pinned` | `path`, `pinned` | `()` |
| `remove_recent` | `path` | `()` |
| `clear_recent` | — | `usize` |
| `export_backup` | — | `serde_json::Value` |
| `import_backup` | `json` | `usize` |
| `load_config` | — | `AppConfig` |
| `save_config` | `config` | `()` |

`save_draft` 返回 bool 而不是错误：存不上备份不该阻断关闭流程。
`clear_bookmarks(path: null)` 清空全部，带 path 清该文件的。

## 会话

| 命令 | 入参 | 出参 |
|---|---|---|
| `restore_session` | — | `SessionRestoreVO` |
| `save_session` | `SessionSaveReq` | `()` |
| `patch_session` | `active_index?`, `sidebar_open?`, `sidebar_tab?`, `split_ratio?` | `()` |
| `remember_window` | —（Tauri 注入 `WebviewWindow`） | `()` |

`save_session` 是**整块替换**：一个事务里换掉 `sessions` 与 `tabs`。
标签数量在个位数到几十之间，一次 DELETE + 批量 INSERT 比算差集简单，
且不会有「删了一半崩溃」的中间态。

`patch_session` 用于布局变化（侧栏开关 / 分屏），不必每次重传标签表。

## 窗口与系统

| 命令 | 入参 | 出参 |
|---|---|---|
| `window_minimize` | —（注入 `Window`） | `()` |
| `window_toggle_maximize` | — | `()` |
| `window_close` | — | `()`（只广播 `app:before-close`，不直接关） |
| `window_is_maximized` | — | `bool` |
| `confirm_quit` | — | `()` |
| `cancel_quit` | — | `()` |
| `force_quit` | —（注入 `AppHandle`） | `()` |
| `start_drag` | — | `()` |
| `get_app_info` | — | `AppInfoVO` |
| `show_message` | `title`, `message`, `kind?` | `()` |
| `show_confirm` | `title`, `message` | `bool` |
| `open_path` | `path` | `()` |

`window_close` **不直接调 `close()`**：那会绕过「关闭到托盘」与「未保存检查」。
它只广播事件，真正的关在 `shell::close_policy` 的 `CloseRequested` 里决定。

## 事件（后端 → 前端）

| 事件 | 载荷 | 触发点 |
|---|---|---|
| `app:ready` | — | `app::lifecycle::on_ready` |
| `app:before-close` | — | `commands::window::window_close` |
| `app:open-file` | `string`（路径） | 单实例转交、命令行参数 |
| `app:new-document` | — | 托盘「新建文档」 |
| `fs:changed` | `string[]`（路径） | `service::file` 的 notify watcher |

`fs:changed` 只报 `is_modify` / `is_create` / `is_remove` 三类，
访问时间之类的噪音事件丢掉。
