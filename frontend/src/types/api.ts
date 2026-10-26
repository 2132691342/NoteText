// 与后端 `domain/*` 的 json tag 一一对应。
// 铁律：跨边界字段一律 snake_case。改字段必须同步 `src-tauri/src/domain/` 与

/** 契约版本：后端改字段时 +1，前端 `api/index.ts` 在启动时校验。 */
export const CONTRACT_VERSION = 4

/** 后端统一错误：invoke reject 时的形状。 */
export interface AppError {
  code: string
  message: string
  details: string
}

// ---- 编辑器域 ----

export interface EditorPosition {
  line: number
  col: number
  selection_len: number
}

export type ViewType = 'code' | 'markdown' | 'richtext' | 'hex' | 'image' | 'log' | 'diff'

// ---- 文件域 ----

export type ReadMode =
  | { mode: 'full' }
  | { mode: 'head' }
  | { mode: 'range'; start_line: number; lines: number }

export interface DirEntry {
  name: string
  path: string
  is_dir: boolean
  size: number
  modified_at: number
  read_only: boolean
}

export interface FileMeta {
  path: string
  size: number
  modified_at: number
  read_only: boolean
  is_dir: boolean
  is_binary: boolean
}

export interface FileContentVO {
  path: string
  text: string
  encoding: string
  read_bytes: number
  truncated: boolean
  line_ending: 'crlf' | 'lf' | 'cr'
  has_bom: boolean
}

export interface WriteResultVO {
  path: string
  bytes: number
  modified_at: number
}

/** CSV / TSV 表格预览：后端已按探测编码解好，前端只画。 */
export interface TablePreviewVO {
  path: string
  encoding: string
  /** 分隔符原字符：`,` / `\t` / `;` / `|` */
  delimiter: string
  rows: string[][]
  total_rows: number
  total_cols: number
  truncated: boolean
  size: number
}

// ---- 编码域 ----

export interface EncodingInfo {
  id: string
  label: string
  bom_only: boolean
}

export interface DetectResultVO {
  encoding: string
  confidence: number
  low_confidence: boolean
}

export type CaseTarget = 'upper' | 'lower' | 'title' | 'invert'

// ---- 语言域 ----

export interface LanguageInfo {
  id: string
  label: string
  exts: string[]
  line_comment: string
  block_comment: [string, string]
}

// ---- 会话域 ----

export interface SessionDO {
  id: number
  active_index: number
  sidebar_open: boolean
  sidebar_tab: string
  window_width: number
  window_height: number
  window_x: number | null
  window_y: number | null
  window_maximized: boolean
  split_ratio: number
  updated_at: number
}

export interface TabDO {
  id: number
  ord: number
  path: string
  title: string
  language: string
  encoding: string
  line_ending: string
  has_bom: boolean
  cursor_line: number
  cursor_col: number
  scroll_ratio: number
  dirty: boolean
  split: boolean
  view_type: string
  updated_at: number
}

export interface SessionRestoreVO {
  session: SessionDO | null
  tabs: TabDO[]
  missing: string[]
}

export interface SessionSaveReq {
  session: SessionDO
  tabs: TabDO[]
}

// ---- 用户数据域 ----

export interface BookmarkDO {
  id: number
  path: string
  line: number
  label: string
  created_at: number
}

export interface BookmarkVO {
  id: number
  path: string
  file_name: string
  line: number
  label: string
  line_text: string
  created_at: number
}

export interface BookmarkAddReq {
  path: string
  line: number
  label?: string
}

export interface SnippetDO {
  id: number
  name: string
  language: string
  content: string
  abbrev: string
  created_at: number
  updated_at: number
}

export interface SnippetVO {
  id: number
  name: string
  language: string
  content: string
  abbrev: string
  lines: number
  created_at: number
  updated_at: number
}

export interface SnippetSaveReq {
  id?: number
  name: string
  language: string
  content: string
  abbrev: string
}

export type MacroStep =
  | { kind: 'text'; text: string }
  | { kind: 'key'; command: string }
  | { kind: 'pause' }

export interface MacroDO {
  id: string
  name: string
  steps: MacroStep[]
  created_at: number
  updated_at: number
}

export interface MacroVO {
  id: string
  name: string
  steps: MacroStep[]
  created_at: number
  updated_at: number
}

export interface MacroSaveReq {
  id?: string
  name: string
  steps: MacroStep[]
}

export interface DraftDO {
  id: string
  title: string
  content: string
  cursor_line: number
  cursor_col: number
  updated_at: number
}

export interface DraftVO {
  id: string
  path: string
  title: string
  content: string
  updated_at: number
}

export interface DraftSaveReq {
  path: string
  title: string
  content: string
}

export interface RecentFileDO {
  id: number
  path: string
  pinned: boolean
  opened_at: number
}

export interface RecentFileVO {
  path: string
  file_name: string
  dir: string
  exists: boolean
  pinned: boolean
  opened_at: number
}

// ---- 配置域 ----

export interface EditorConfig {
  font_size: number
  font_family: string
  editor_theme: string
  tab_size: number
  insert_spaces: boolean
  word_wrap: boolean
  line_numbers: boolean
  highlight_active_line: boolean
  bracket_matching: boolean
  bracket_pair_colors: boolean
  indent_guides: boolean
  show_whitespace: boolean
  show_eol: boolean
  fold_gutter: boolean
  minimap: boolean
  auto_save: boolean
  auto_save_interval: number
  highlight_selection: boolean
}

export interface FileConfig {
  default_encoding: string
  auto_detect_encoding: boolean
  new_line_ending: string
  ensure_final_newline: boolean
  trim_trailing_ws: boolean
  ignore_patterns: string[]
  remember_last_folder: boolean
  last_folder: string
}

export interface UIConfig {
  show_menu_bar: boolean
  show_toolbar: boolean
  show_status_bar: boolean
  show_sidebar: boolean
  sidebar_width: number
  toolbar_icon_size: number
  zoom_level: number
  close_to_tray: boolean
  restore_session: boolean
  recent_limit: number
  status_bar_items: Record<string, boolean>
  /** 侧栏当前页签：files / bookmarks / snippets / functions / monitor。 */
  sidebar_tab: string
}

export interface AppConfig {
  theme: 'light' | 'dark' | 'system'
  language: 'zh-CN' | 'en'
  editor: EditorConfig
  file: FileConfig
  ui: UIConfig
  keybindings: Record<string, string>
}

// ---- 工具箱域 ----

export type JsonFormat = 'pretty' | 'minify'

export interface JsonFormatReq {
  text: string
  format: JsonFormat
  indent?: number
  sort_keys: boolean
}

export interface JsonFormatVO {
  ok: boolean
  output: string
  error_line: number | null
  error_col: number | null
  error_message: string | null
}

export interface JsonQueryReq {
  text: string
  path: string
}

export interface JsonQueryVO {
  ok: boolean
  matches: unknown[]
  error_message: string | null
}

export type StructLang = 'go' | 'ts' | 'java' | 'python' | 'rust'

export interface JsonToStructReq {
  text: string
  lang: StructLang
  root_name: string
}

export interface JsonToStructVO {
  ok: boolean
  code: string
  error_message: string | null
}

export type JsonDiffItem =
  | { kind: 'added'; path: string; value: unknown }
  | { kind: 'removed'; path: string; value: unknown }
  | { kind: 'changed'; path: string; left: unknown; right: unknown }
  | { kind: 'type_changed'; path: string; left: string; right: string }

export interface JsonDiffReq {
  left: string
  right: string
  ignore_order: boolean
}

export interface JsonDiffVO {
  ok: boolean
  items: JsonDiffItem[]
  error_message: string | null
}

export interface TextDiffReq {
  left: string
  right: string
  left_title?: string
  right_title?: string
  ignore_ws: boolean
  ignore_case: boolean
  ignore_blank: boolean
}

export type DiffOp = 'equal' | 'added' | 'removed'

export interface DiffLineVO {
  op: DiffOp
  left_no: number
  right_no: number
  left_text: string
  right_text: string
  left_spans: Array<[number, number]>
  right_spans: Array<[number, number]>
}

export interface TextDiffVO {
  ok: boolean
  lines: DiffLineVO[]
  added: number
  removed: number
  changed: number
  error_message: string | null
}

export interface DirDiffEntryVO {
  rel_path: string
  status: 'same' | 'diff' | 'left_only' | 'right_only'
  left_size: number
  right_size: number
}

export interface DirDiffVO {
  ok: boolean
  entries: DirDiffEntryVO[]
  error_message: string | null
}

export type HashAlgo = 'md5' | 'sha1' | 'sha256'

export interface HashReq {
  path?: string
  text?: string
  algo: HashAlgo
  upper: boolean
}

export interface HashVO {
  ok: boolean
  hex: string
  algo: HashAlgo
  bytes: number
  error_message: string | null
}

export interface RegexTestReq {
  pattern: string
  text: string
  ignore_case: boolean
  multiline: boolean
  dot_all: boolean
  extended: boolean
}

export interface RegexMatchVO {
  whole: string
  start: number
  end: number
  groups: string[]
  named: Array<[string, string]>
}

export interface RegexTestVO {
  ok: boolean
  matches: RegexMatchVO[]
  truncated: boolean
  error_message: string | null
}

export type LineOp =
  | 'remove_blank'
  | 'trim_trailing_ws'
  | 'to_upper'
  | 'to_lower'
  | 'to_title'
  | 'invert_case'
  | 'squeeze_blank'
  | 'reverse'
  | 'dedup'
  | 'sort_asc'
  | 'sort_desc'

// ---- 批量操作域 ----
// 与 domain/batch.rs 对应；两段式（scan → apply）由前端驱动：批量改磁盘没有撤销键。

export type FindMode = 'plain' | 'regex'

export interface BatchFindReq {
  dir: string
  glob: string
  find: string
  mode?: FindMode
  case_sensitive?: boolean
  recursive?: boolean
}

export interface BatchFileHits {
  path: string
  count: number
  lines: number[]
  sample: string
}

export interface BatchFindVO {
  hits: BatchFileHits[]
  total_count: number
  skipped: number
  truncated: boolean
}

export interface BatchReplaceReq {
  dir: string
  glob: string
  find: string
  replace: string
  mode?: FindMode
  case_sensitive?: boolean
  recursive?: boolean
  /** 勾选的文件；空数组表示全部命中文件。 */
  files: string[]
}

export interface BatchFailure {
  path: string
  message: string
}

export interface BatchReplaceVO {
  changed: number
  replacements: number
  failures: BatchFailure[]
}

export type RenameMode = 'replace' | 'sequence' | 'case'

export interface BatchRenameReq {
  files: string[]
  mode?: RenameMode
  find: string
  replace: string
  start?: number
  prefix?: boolean
}

export interface RenamePlanItem {
  from: string
  to: string
  conflict: boolean
  unchanged: boolean
}

export interface BatchRenameVO {
  items: RenamePlanItem[]
  applicable: number
}

export interface RenamePair {
  from: string
  to: string
}

export interface RenameApplyReq {
  items: RenamePair[]
}

// ---- 文件操作域（文件树右键 / 拖拽） ----
// 与 domain/file_ops.rs 对应：目标已存在一律拒绝，路径必须绝对且不含 `..`。

export interface RenameReq {
  from: string
  to: string
}

export interface MoveCopyReq {
  paths: string[]
  dest_dir: string
  /** 同名时追加序号而不是拒绝。默认 false。 */
  auto_suffix?: boolean
}

export interface DeleteReq {
  paths: string[]
}

export interface MkdirReq {
  path: string
}

export interface FileOpResultVO {
  from: string
  /** 操作后的路径；删除时为空串。 */
  to: string
  ok: boolean
  message: string
}

export interface FileOpVO {
  results: FileOpResultVO[]
  succeeded: number
  failed: number
}

// ---- 归档域（2.0 压缩子系统，specs/18）----
// 与 domain/archive*.rs 对应；长任务提交队列只回任务 id，进度走事件。

export interface ArchiveEntryVO {
  name: string
  is_dir: boolean
  size: number
  packed_size: number
  crc: number | null
  modified: number | null
  method: string | null
  encrypted: boolean
  ratio: number | null
}

export interface ArchiveInfoVO {
  path: string
  format: string
  entries: ArchiveEntryVO[]
  total_size: number
  packed_size: number
  file_count: number
  dir_count: number
  encrypted: boolean
  encrypted_names: boolean
  comment: string | null
  is_split: boolean
  solid: boolean
  ratio: number | null
}

export interface OpenArchiveVO {
  info: ArchiveInfoVO
  parts: string[]
}

export interface CreateArchiveReq {
  sources: string[]
  dest: string
  format: string
  method?: string | null
  level?: number | null
  password?: string | null
  encrypt_filenames: boolean
  solid: boolean
  split_to?: number | null
  exclude: string[]
  comment?: string | null
}

export interface ExtractArchiveReq {
  archive: string
  dest: string
  entries?: string[] | null
  password?: string | null
  /** overwrite | skip | rename */
  overwrite: string
  create_subdir: boolean
}

export interface TestArchiveReq {
  archive: string
  password?: string | null
}

export interface AddEntriesReq {
  archive: string
  sources: string[]
  password?: string | null
  exclude: string[]
}

export interface DeleteEntriesReq {
  archive: string
  entries: string[]
  password?: string | null
}

export interface ConvertArchiveReq {
  archive: string
  dest: string
  format: string
  password?: string | null
  new_password?: string | null
  level?: number | null
}

export interface SplitFileReq {
  file: string
  dest_dir: string
  part_size: number
}

export interface JoinPartsReq {
  parts: string[]
  dest: string
}

export interface PreviewEntryReq {
  archive: string
  entry: string
  password?: string | null
  max_bytes?: number
}

export interface CreateArchiveVO {
  dest: string
  file_count: number
  total_size: number
  packed_size: number
  parts: string[]
}

export interface ExtractArchiveVO {
  dest: string
  file_count: number
  total_size: number
  skipped: number
}

export interface TestArchiveVO {
  ok: boolean
  tested: number
  failures: string[]
}

export interface UpdateArchiveVO {
  archive: string
  file_count: number
  added: number
  removed: number
}

export interface ConvertArchiveVO {
  dest: string
  file_count: number
  total_size: number
  packed_size: number
}

export interface JoinPartsVO {
  dest: string
  total_size: number
}

export interface PreviewEntryVO {
  name: string
  size: number
  /** text | image | other */
  kind: string
  content: string | null
  data_url: string | null
  truncated: boolean
  too_large: boolean
}

export interface FormatCapabilities {
  read: boolean
  write: boolean
  container: boolean
  encryption: boolean
}

export interface ArchiveFormatVO {
  id: string
  extensions: string[]
  capabilities: FormatCapabilities
  default_method: string
}

export type ArchiveTaskStatus =
  | 'queued'
  | 'running'
  | 'paused'
  | 'done'
  | 'failed'
  | 'cancelled'

export type ArchiveTaskKind =
  | 'create'
  | 'extract'
  | 'test'
  | 'convert'
  | 'add'
  | 'delete'
  | 'split'
  | 'join'

export interface ArchiveTaskVO {
  id: string
  kind: ArchiveTaskKind
  title: string
  status: ArchiveTaskStatus
  /** 0..1；-1 表示总量未知（前端画不确定态动画）。 */
  progress: number
  processed: number
  total: number
  current: string | null
  speed: number | null
  error: string | null
  result: unknown
  created_at: number
  finished_at: number | null
}

export interface ArchiveTaskFinishedVO {
  task: ArchiveTaskVO
}

// ---- 系统域 ----

export interface AppInfoVO {
  name: string
  version: string
  data_dir: string
  log_dir: string
  os: string
  arch: string
  debug_build: boolean
}

// ---- 系统文件关联（specs/17）----

export interface FileAssocExtVO {
  /** 扩展名，不带点。 */
  extension: string
  /** 是否已进「打开方式」列表。 */
  in_open_with: boolean
  /** 是否已是系统默认。 */
  is_default: boolean
}

export interface FileAssocGroupVO {
  /** 分组标识：text / archive。 */
  group: string
  extensions: FileAssocExtVO[]
}

export interface FileAssocStatusVO {
  groups: FileAssocGroupVO[]
}
