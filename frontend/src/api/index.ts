// Tauri invoke 封装：全站唯一的「前端 → Rust」出口。
// 每个命令一个导出函数（函数名与 Rust 侧一致），错误统一转成 AppError。

import { invoke } from '@tauri-apps/api/core'
import { tr } from '@/i18n'
import type { AppError, AppInfoVO } from '@/types/api'

export { CONTRACT_VERSION } from '@/types/api'
export type { AppError } from '@/types/api'

/** 是否跑在 Tauri WebView 里。 */
export const isDesktop = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

/** 无后端时的统一错误：浏览器开发态用。 */
const NO_BACKEND: AppError = {
  code: 'no-backend',
  message: tr('toast.noBackend'),
  details: tr('toast.noBackendDetail'),
}

/**
 * 调一个 Rust command。
 *
 * Tauri v2 的 reject 形状是 `{ code, message, details }`（后端 AppError 序列化
 * 成 JSON 后由 invoke 原样带回）。解析失败时给一个 unknown 码的兜底——
 * 丢原始字符串会让用户看到一串 Rust 内部路径。
 */
export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isDesktop) {
    throw NO_BACKEND
  }
  try {
    return (await invoke<T>(cmd, args)) as T
  } catch (e) {
    throw normalizeError(e)
  }
}

/**
 * 把 invoke 的 reject 归一成 AppError。
 *
 * 后端 message 是写死的中文（Rust 侧没有界面语言这个概念），直接透给英文
 * 界面就是中英混排。这里按错误码取本地化文案，后端那句只当 details 用。
 */
export function normalizeError(e: unknown): AppError {
  if (typeof e === 'string') {
    // 老形状：纯字符串。按 JSON 试着解一次。
    try {
      const parsed = JSON.parse(e) as AppError
      if (parsed && typeof parsed.code === 'string') return localize(parsed)
    } catch {
      /* 不是 JSON：按未知错误处理 */
    }
    return { code: 'unknown', message: e, details: '' }
  }
  if (e && typeof e === 'object') {
    const o = e as Partial<AppError> & { message?: string }
    if (typeof o.code === 'string') {
      return localize({
        code: o.code,
        message: o.message ?? '',
        details: o.details ?? '',
      })
    }
    if (typeof o.message === 'string') {
      return { code: 'unknown', message: o.message, details: '' }
    }
  }
  return { code: 'unknown', message: tr('error.unknown', { reason: '' }), details: String(e) }
}

/** 按错误码换成当前语言的文案；码不认识时保留后端原句。 */
function localize(err: AppError): AppError {
  const key = 'errCode.' + err.code
  const text = tr(key)
  // vue-i18n 缺 key 时返回 key 本身：那等于没翻译，用原句。
  if (!text || text === key) return err
  return { code: err.code, message: text, details: err.details || err.message }
}

// ---- 按域分组的薄封装 ----

import type {
  BatchFindReq,
  BatchFindVO,
  BatchRenameReq,
  BatchRenameVO,
  BatchReplaceReq,
  BatchReplaceVO,
  BookmarkAddReq,
  BookmarkDO,
  BookmarkVO,
  CaseTarget,
  DeleteReq,
  DetectResultVO,
  DirDiffVO,
  DirEntry,
  DraftSaveReq,
  DraftVO,
  EncodingInfo,
  FileContentVO,
  FileMeta,
  FileOpVO,
  HashReq,
  HashVO,
  JsonDiffReq,
  JsonDiffVO,
  JsonFormatReq,
  JsonFormatVO,
  JsonQueryReq,
  JsonQueryVO,
  JsonToStructReq,
  JsonToStructVO,
  LanguageInfo,
  LineOp,
  MacroSaveReq,
  MacroVO,
  MkdirReq,
  MoveCopyReq,
  ReadMode,
  RecentFileVO,
  RegexTestReq,
  RegexTestVO,
  RenameApplyReq,
  RenameReq,
  SessionRestoreVO,
  SessionSaveReq,
  SnippetSaveReq,
  SnippetVO,
  TextDiffReq,
  TextDiffVO,
  WriteResultVO,
} from '@/types/api'
import type { AppConfig } from '@/types/api'

export const api = {
  // 文件
  readFile: (path: string, encoding?: string, mode?: ReadMode) =>
    call<FileContentVO>('read_file', {
      path,
      encoding: encoding ?? null,
      mode: serializeReadMode(mode),
    }),
  /** 读二进制（预览 / Hex）。返回 base64：走 Vec<u8> 会被序列化成 number[]。 */
  readBinary: (path: string) => call<string>('read_binary', { path }),
  writeFile: (
    path: string,
    text: string,
    encoding?: string,
    lineEnding?: string,
    bom?: boolean,
  ) =>
    call<WriteResultVO>('write_file', {
      path,
      text,
      encoding: encoding ?? null,
      line_ending: lineEnding ?? null,
      bom: bom ?? null,
    }),
  saveFileAs: (
    text: string,
    suggestedName?: string,
    encoding?: string,
    lineEnding?: string,
    bom?: boolean,
  ) =>
    call<WriteResultVO | null>('save_file_as', {
      text,
      suggested_name: suggestedName ?? null,
      encoding: encoding ?? null,
      line_ending: lineEnding ?? null,
      bom: bom ?? null,
    }),
  readDir: (dir: string) => call<DirEntry[]>('read_dir', { dir }),
  statFile: (path: string) => call<FileMeta>('stat_file', { path }),
  // 文件树右键 / 拖拽：目标已存在一律拒绝（见 api.fileOps）
  fileOps: {
    createFile: (path: string) =>
      call<FileOpVO>('create_file', { req: { path } satisfies MkdirReq }),
    createDir: (path: string) =>
      call<FileOpVO>('create_dir', { req: { path } satisfies MkdirReq }),
    rename: (from: string, to: string) =>
      call<FileOpVO>('rename_path', { req: { from, to } satisfies RenameReq }),
    move: (paths: string[], destDir: string, autoSuffix = false) =>
      call<FileOpVO>(
        'move_paths',
        { req: { paths, dest_dir: destDir, auto_suffix: autoSuffix } satisfies MoveCopyReq },
      ),
    copy: (paths: string[], destDir: string, autoSuffix = false) =>
      call<FileOpVO>(
        'copy_paths',
        { req: { paths, dest_dir: destDir, auto_suffix: autoSuffix } satisfies MoveCopyReq },
      ),
    remove: (paths: string[]) =>
      call<FileOpVO>('delete_paths', { req: { paths } satisfies DeleteReq }),
  },
  openFileDialog: (suggestedDir?: string) =>
    call<string | null>('open_file_dialog', { suggested_dir: suggestedDir ?? null }),
  openFolderDialog: (suggestedDir?: string) =>
    call<string | null>('open_folder_dialog', { suggested_dir: suggestedDir ?? null }),
  saveFileDialog: (suggestedName?: string) =>
    call<string | null>('save_file_dialog', { suggested_name: suggestedName ?? null }),
  detectEncoding: (bytes: number[]) =>
    call<DetectResultVO>('detect_encoding', { bytes }),
  watchDir: (dir: string) => call<void>('watch_dir', { dir }),
  unwatchDir: (dir: string) => call<void>('unwatch_dir', { dir }),
  recentFiles: () => call<RecentFileVO[]>('get_recent_files'),

  // 编辑器
  languages: () => call<LanguageInfo[]>('get_languages'),
  toggleLineComment: (language: string, text: string) =>
    call<string>('toggle_line_comment', { language, text }),
  applyLineOp: (op: LineOp, text: string) => call<string>('apply_line_op', { op, text }),
  gotoLine: (text: string, line: number) => call<number>('goto_line', { text, line }),
  textStats: (text: string) =>
    call<{ lines: number; non_blank_lines: number; chars: number; words: number }>(
      'get_text_stats',
      { text },
    ),
  convertCase: (path: string, text: string, target: CaseTarget) =>
    call<string>('convert_case', { path, text, target }),
  convertLineEnding: (path: string, text: string, target: string) =>
    call<string>('convert_line_ending', { path, text, target }),

  // 工具箱
  jsonFormat: (req: JsonFormatReq) => call<JsonFormatVO>('json_format', { req }),
  jsonQuery: (req: JsonQueryReq) => call<JsonQueryVO>('json_query', { req }),
  jsonToStruct: (req: JsonToStructReq) => call<JsonToStructVO>('json_to_struct', { req }),
  jsonDiff: (req: JsonDiffReq) => call<JsonDiffVO>('json_diff', { req }),
  textDiff: (req: TextDiffReq) => call<TextDiffVO>('text_diff', { req }),
  dirDiff: (left: string, right: string) => call<DirDiffVO>('dir_diff', { left, right }),
  hash: (req: HashReq) => call<HashVO>('hash_text', { req }),
  regexTest: (req: RegexTestReq) => call<RegexTestVO>('regex_test', { req }),
  encodings: () => call<EncodingInfo[]>('get_encodings'),

  // 批量操作：scan 出预览，apply 才落盘（两段式见 domain/batch.rs）
  batchScanFind: (req: BatchFindReq) => call<BatchFindVO>('batch_scan_find', { req }),
  batchApplyReplace: (req: BatchReplaceReq) =>
    call<BatchReplaceVO>('batch_apply_replace', { req }),
  batchPlanRename: (req: BatchRenameReq) => call<BatchRenameVO>('batch_plan_rename', { req }),
  batchApplyRename: (req: RenameApplyReq) =>
    call<BatchReplaceVO>('batch_apply_rename', { req }),

  // 用户数据
  listBookmarks: () => call<BookmarkVO[]>('list_bookmarks'),
  listFileBookmarks: (path: string) => call<BookmarkDO[]>('list_file_bookmarks', { path }),
  addBookmark: (req: BookmarkAddReq) => call<BookmarkDO>('add_bookmark', { req }),
  toggleBookmark: (req: BookmarkAddReq) => call<boolean>('toggle_bookmark', { req }),
  removeBookmark: (id: number) => call<void>('remove_bookmark', { id }),
  clearBookmarks: (path?: string) => call<number>('clear_bookmarks', { path: path ?? null }),
  listSnippets: (language?: string, keyword?: string) =>
    call<SnippetVO[]>('list_snippets', { language: language ?? null, keyword: keyword ?? null }),
  saveSnippet: (req: SnippetSaveReq) => call<SnippetVO>('save_snippet', { req }),
  removeSnippet: (id: number) => call<void>('remove_snippet', { id }),
  snippetLanguages: () => call<string[]>('get_snippet_languages'),
  snippetByAbbrev: (abbrev: string) =>
    call<SnippetVO | null>('snippet_by_abbrev', { abbrev }),
  listMacros: () => call<MacroVO[]>('list_macros'),
  saveMacro: (req: MacroSaveReq) => call<MacroVO>('save_macro', { req }),
  removeMacro: (id: string) => call<void>('remove_macro', { id }),
  listDrafts: () => call<DraftVO[]>('list_drafts'),
  saveDraft: (req: DraftSaveReq) => call<boolean>('save_draft', { req }),
  removeDraft: (id: string) => call<void>('remove_draft', { id }),
  touchRecent: (path: string) => call<void>('touch_recent', { path }),
  setRecentPinned: (path: string, pinned: boolean) =>
    call<void>('set_recent_pinned', { path, pinned }),
  removeRecent: (path: string) => call<void>('remove_recent', { path }),
  clearRecent: () => call<number>('clear_recent'),
  exportBackup: () => call<unknown>('export_backup'),
  importBackup: (json: unknown) => call<number>('import_backup', { json }),
  loadConfig: () => call<AppConfig>('load_config'),
  saveConfig: (config: AppConfig) => call<void>('save_config', { config }),

  // 会话
  restoreSession: () => call<SessionRestoreVO>('restore_session'),
  saveSession: (req: SessionSaveReq) => call<void>('save_session', { req }),
  patchSession: (patch: {
    active_index?: number
    sidebar_open?: boolean
    sidebar_tab?: string
    split_ratio?: number
  }) =>
    call<void>('patch_session', {
      active_index: patch.active_index ?? null,
      sidebar_open: patch.sidebar_open ?? null,
      sidebar_tab: patch.sidebar_tab ?? null,
      split_ratio: patch.split_ratio ?? null,
    }),
  rememberWindow: () => call<void>('remember_window'),

  // 窗口与系统
  windowMinimize: () => call<void>('window_minimize'),
  windowToggleMaximize: () => call<void>('window_toggle_maximize'),
  windowClose: () => call<void>('window_close'),
  windowIsMaximized: () => call<boolean>('window_is_maximized'),
  confirmQuit: () => call<void>('confirm_quit'),
  cancelQuit: () => call<void>('cancel_quit'),
  forceQuit: () => call<void>('force_quit'),
  startDrag: () => call<void>('start_drag'),
  appInfo: () => call<AppInfoVO>('get_app_info'),
  showMessage: (title: string, message: string, kind?: string) =>
    call<void>('show_message', { title, message, kind: kind ?? null }),
  showConfirm: (title: string, message: string) =>
    call<boolean>('show_confirm', { title, message }),
  openPath: (path: string) => call<void>('open_path', { path }),
  // 剪贴板：WebView2 不给渲染层 clipboard-read 权限，读写都走壳层
  readClipboard: () => call<string>('read_clipboard'),
  writeClipboard: (text: string) => call<void>('write_clipboard', { text }),
}

/** ReadMode 序列化成 Rust 的 tagged enum（`#[serde(tag = "mode")]`）。 */
function serializeReadMode(mode?: ReadMode): unknown {
  if (!mode) return null
  switch (mode.mode) {
    case 'head':
      return { mode: 'head' }
    case 'range':
      return { mode: 'range', start_line: mode.start_line, lines: mode.lines }
    default:
      return { mode: 'full' }
  }
}
