// 与后端 `domain/*` 的 json tag 一一对应。
//
// 铁律：跨边界字段一律 snake_case。改字段必须同步 `src-tauri/src/domain/` 与
// `CONTRACT_VERSION`，否则编译不报错、运行时全 undefined。

/** 契约版本：后端改字段时 +1，前端 `api/index.ts` 在启动时校验。 */
export const CONTRACT_VERSION = 1

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
//
// 与后端 domain/batch.rs 一一对应。两段式（scan → apply）由前端驱动：
// 批量改磁盘没有撤销键，所以类型上也分成「预览」与「应用」两组。

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
