# API 契约

## 定位

前端 `api/index.ts` 与后端 `commands/` 的对照表：105 个 invoke 命令的入参出参。
**改字段必须两边同步**，并且把 `types/api.ts` 的 `CONTRACT_VERSION` +1。

## 通用约定

| 维度 | 规则 |
|---|---|
| 命名 | snake_case，与 Rust 函数名一致 |
| 出入参 | 一律 `domain` 的类型；两个以上参数用结构体而不是位置参数 |
| 成功 | 直接返回值（Tauri 的 invoke 成功态） |
| 失败 | `AppError { code, message, details }`，前端 `normalizeError` 归一 |
| 二进制 | 走 `read_binary`，出参是 base64 字符串（`Vec<u8>` 会被序列化成 `number[]`，体积约 4 倍） |
| 空数组 | 后端构造时初始化，契约说是数组就永远发 `[]`（守护见 [12](12-testing.md) 的 `empty_lists_serialize_to_array_not_null`） |

## 文件

| 命令 | 入参 | 出参 |
|---|---|---|
| `read_file` | `path`, `mode?`, `encoding?` | `FileContentVO` |
| `read_binary` | `path` | `string`（base64，上限 20MB） |
| `read_table_preview` | `path`, `encoding?` | `TablePreviewVO`（CSV / TSV，不受 20MB 上限） |
| `write_file` | `path`, `text`, `encoding?`, `line_ending?`, `bom?` | `WriteResultVO` |
| `save_file_as` | `text`, `suggested_name?`, `encoding?`, `line_ending?`, `bom?` | `WriteResultVO \| null` |
| `read_dir` | `dir` | `DirEntry[]` |
| `stat_file` | `path` | `FileMeta` |
| `open_file_dialog` | `suggested_dir?` | `string \| null` |
| `open_files_dialog` | `suggested_dir?` | `string[]`（多选，批量解压用） |
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

## 文件操作（文件树右键 / 拖拽）

域模型见 `domain/file_ops.rs`。所有写操作的拒绝标准统一在
`service::file_ops::validate_target`：**目标已存在即拒**（不覆盖）、
**目录不能进自己的子目录**、**路径必须绝对且不含 `..`**。

| 命令 | 入参 | 出参 |
|---|---|---|
| `create_file` | `req: MkdirReq` | `FileOpVO` |
| `create_dir` | `req: MkdirReq` | `FileOpVO` |
| `rename_path` | `req: RenameReq` | `FileOpVO` |
| `move_paths` | `req: MoveCopyReq` | `FileOpVO` |
| `copy_paths` | `req: MoveCopyReq` | `FileOpVO` |
| `delete_paths` | `req: DeleteReq` | `FileOpVO` |

`FileOpVO.results` 逐条记账（`succeeded` / `failed` 分开计数）：批量操作里
「部分成功」比「整体失败」有用，但失败项必须报出来。
`MoveCopyReq.auto_suffix` 为 true 时同名不拒绝，落到 `name (2).ext` 起的序号名。

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

## 归档 / 压缩

域模型见 `domain/archive.rs`（REQ/VO）、`domain/archive_format.rs`（格式能力）、
`domain/archive_task.rs`（任务快照）；子系统设计见 [18](18-archive.md)。

| 命令 | 入参 | 出参 |
|---|---|---|
| `open_archive` | `path`, `password?` | `OpenArchiveVO`（同步语义；标 async 线程池执行，防大包列条目冻窗） |
| `preview_archive_entry` | `req: PreviewEntryReq` | `PreviewEntryVO`（同上：固实 7z 预览要解整块） |
| `get_archive_formats` | — | `ArchiveFormatVO[]` |
| `create_archive` | `req: CreateArchiveReq` | `string`（任务 id） |
| `extract_archive` | `req: ExtractArchiveReq` | `string`（任务 id） |
| `test_archive` | `req: TestArchiveReq` | `string`（任务 id） |
| `convert_archive` | `req: ConvertArchiveReq` | `string`（任务 id） |
| `add_to_archive` | `req: AddEntriesReq` | `string`（任务 id） |
| `delete_archive_entries` | `req: DeleteEntriesReq` | `string`（任务 id） |
| `split_archive_file` | `req: SplitFileReq` | `string`（任务 id） |
| `join_archive_parts` | `req: JoinPartsReq` | `string`（任务 id） |
| `list_archive_tasks` | — | `ArchiveTaskVO[]` |
| `cancel_archive_task` | `id` | `()` |
| `pause_archive_task` | `id` | `()` |
| `resume_archive_task` | `id` | `()` |
| `clear_archive_tasks` | — | `()` |

**长任务只回任务 id**：进度经 `archive:task-update` / `archive:task-finished`
两个事件推（见下），前端不再轮询。`CreateArchiveVO` / `ExtractArchiveVO` /
`TestArchiveVO` / `ConvertArchiveVO` / `UpdateArchiveVO` / `JoinPartsVO` /
分卷清单作为事件 `result` 负载到达，结构化字段供完成提示与「打开目标」使用。

`ArchiveTaskVO.progress` 为 `-1` 表示总量未知（前端画不确定态）；终态时队列把它
收敛到 1。`ArchiveInfoVO.entries` 恒为数组（空数组契约对归档条目同样生效）。

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
| `snippet_by_abbrev` | `abbrev` | `SnippetVO \| null` |
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
| `hide_to_tray` | — | `()`（「关闭到托盘」路径的落点，前端提示后调用） |
| `window_is_maximized` | — | `bool` |
| `confirm_quit` | — | `()` |
| `cancel_quit` | — | `()` |
| `force_quit` | —（注入 `AppHandle`） | `()` |
| `start_drag` | — | `()` |
| `get_app_info` | — | `AppInfoVO` |
| `take_launch_intent` | — | `Option<string>`（冷启动命令行文件路径，拉取一次） |
| `show_message` | `title`, `message`, `kind?` | `()` |
| `show_confirm` | `title`, `message` | `bool` |
| `open_path` | `path` | `()` |
| `get_file_assoc_status` | — | `FileAssocStatusVO`（两组扩展各自在不在「打开方式」/ 是不是默认） |
| `set_file_assoc_group` | `req: FileAssocSetReq` | `()`（注册 / 注销一组，见 specs/17） |
| `open_default_apps_settings` | — | `()`（拉起 ms-settings:defaultapps） |
| `read_clipboard` | — | `string`（非文本内容为空串） |
| `write_clipboard` | `text` | `()` |

`read_clipboard` / `write_clipboard` 存在的原因：WebView2 不给渲染层
`clipboard-read` 权限，粘贴链路必须在壳层做（`arboard`）。
非文本内容（图片等）不算错误——返回空串，前端按「剪贴板为空」提示。

`window_close` **不直接调 `close()`**：那会绕过「关闭到托盘」与「未保存检查」。
它只广播事件，真正的关在 `shell::close_policy` 的 `CloseRequested` 里决定。

## 事件（后端 → 前端）

| 事件 | 载荷 | 触发点 |
|---|---|---|
| `app:ready` | — | `app::lifecycle::on_ready` |
| `app:before-close` | — | `commands::window::window_close` |
| `app:open-file` | `string`（路径） | 单实例转交（冷启动走 `take_launch_intent` 拉取，不发事件） |
| `app:new-document` | — | 托盘「新建文档」 |
| `fs:changed` | `string[]`（路径） | `service::file` 的 notify watcher |
| `archive:task-update` | `ArchiveTaskVO[]`（全量快照） | 队列状态变化 + 250ms 心跳 |
| `archive:task-finished` | `{ task: ArchiveTaskVO }` | 任务到达终态（done/failed/cancelled） |

`fs:changed` 只报 `is_modify` / `is_create` / `is_remove` 三类，
访问时间之类的噪音事件丢掉。

## 取舍

| 选择 | 优势 | 代价 |
|---|---|---|
| 两个以上入参一律用结构体而非位置参数 | 加字段不用改顺序、不会记错位置 | 每个命令多写一个 REQ 类型 |
| 码值是数字字符串而非 int | 前端靠 code 分流时不会被误读成 HTTP 状态码 | 不能直接数值比较大小 |
| 二进制出参用 base64 而非 `Vec<u8>` | invoke 会把 `Vec<u8>` 序列化成 `number[]`，体积膨胀约 4 倍 | 前后端各多一次编解码 |
| 列表字段构造时一律初始化为空数组 | Rust 的 nil slice 会序列化成 `null`，前端按数组读就白屏 | 每次构造都要记得 `Vec::new()` |
| 前端不挂 fs 插件、二进制读取经 `read_binary` | 不给渲染层整块磁盘权限 | 每次预览要多一次 invoke 往返 |
| `read_file` 的 `encoding` 可选而非另开一个命令 | 强制编码读取与自动探测读是同一条链路，只差一个参数 | 调用方要传 `undefined` 占位才能给到第三个参数 |
| 强制编码解不动时退回探测编码 | 用户选错编码也打得开文件，比乱码可恢复 | 返回的编码不是用户选的那个，状态栏要能反映这一点 |
