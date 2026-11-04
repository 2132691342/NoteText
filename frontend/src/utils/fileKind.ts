// 文件形态判定：决定一个文件进编辑器、预览器还是十六进制视图。
// 一处说了算——store 用它决定「读文本还是只建标签」，EditorArea 用它选组件。
import { extName } from '@/utils'

/**
 * 文件的渲染形态。
 *
 * `legacyDoc` 是「确实是文档、但内置渲染器不做」（.doc / .ppt / .odt）：
 * 它给的是「用外部程序打开」的卡片，而不是一屏十六进制。
 * `binary` 则是可执行文件这类根本不该当文档看的东西。
 */
export type FileKind =
  | 'text'
  | 'markdown'
  | 'html'
  | 'image'
  | 'sheet'
  | 'legacyDoc'
  | 'archiveLegacy'
  | 'binary'

const IMAGE = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'ico', 'avif'])

/** 表格：xlsx 库能读老 .xls，CSV/TSV 按同一条链路转成表。 */
const SHEET = new Set(['xlsx', 'xlsm', 'xls', 'csv', 'tsv', 'ods'])

/** HTML：可编辑的文本，预览走沙箱 iframe（不执行脚本）。 */
const HTML = new Set(['html', 'htm', 'xhtml'])

const MARKDOWN = new Set(['md', 'markdown', 'mdown', 'mkd', 'mdx'])

/**
 * 不做内置渲染的文档格式。
 *
 * Word / PDF / PPT / EPUB 都不在内置预览范围内：内嵌渲染器体积大、
 * 还原度有限，半成品预览不如一句明确的「用外部程序打开」。
 * Excel 与 CSV 例外——那类文件用户想看的就是表格本身。
 */
const LEGACY_DOC = new Set([
  'doc', 'docx', 'docm', 'dot', 'dotx', 'rtf', 'ppt', 'pptx', 'pps', 'pptm',
  'odt', 'odp', 'pages', 'key', 'epub', 'mobi', 'pdf',
])

/**
 * 没有预览器、打开即进压缩浏览器的归档格式。
 *
 * 这些格式现在由压缩子系统原生支持（specs/18）：点开直接列条目，
 * 不再给「十六进制」或「装个别的工具」。
 */
const ARCHIVE_LEGACY = new Set<string>([])

/**
 * 扩展名即可断定的二进制外壳。
 *
 * 为什么要列出来而不是全靠内容嗅探：嗅探只读前 8KB，
 * 一个以纯文本开头的压缩包 / 可执行文件嗅不出 NUL，会被当成文本打开，
 * 编辑器里就是一屏乱码，保存还会写坏原文件。
 */
const BINARY = new Set([
  'zip', '7z', 'gz', 'tgz', 'bz2', 'xz', 'tar', 'txz', 'tbz2', 'tzst', 'tlz4', 'tbr',
  'rar', 'cab', 'iso', 'lzh', 'lha', 'cpio', 'ar', 'deb', 'xar',
  'exe', 'dll', 'sys', 'msi', 'com', 'scr', 'app', 'apk', 'jar', 'war', 'class',
  'so', 'dylib', 'o', 'obj', 'lib', 'a', 'pdb', 'pyc', 'wasm',
  'db', 'sqlite', 'sqlite3', 'mdb', 'dat', 'pak', 'bin', 'dmg', 'vhd', 'vhdx',
  'mp3', 'wav', 'flac', 'aac', 'ogg', 'm4a',
  'mp4', 'avi', 'mkv', 'mov', 'wmv', 'flv', 'webm',
  'ttf', 'otf', 'woff', 'woff2', 'eot',
  'psd', 'ai', 'eps', 'sketch', 'fig',
])

/**
 * 判定文件形态。
 *
 * `isBinary` 来自后端 stat（前 8KB 含 NUL），只对「扩展名不认识」的文件生效：
 * 认识的扩展名一律按表走，避免大文件嗅探不到 NUL 时误开成文本。
 */
export function kindOf(path: string, isBinary = false): FileKind {
  const ext = extName(path)
  if (IMAGE.has(ext)) return 'image'
  if (SHEET.has(ext)) return 'sheet'
  if (MARKDOWN.has(ext)) return 'markdown'
  if (HTML.has(ext)) return 'html'
  if (LEGACY_DOC.has(ext)) return 'legacyDoc'
  if (BINARY.has(ext)) return 'binary'
  return isBinary ? 'binary' : 'text'
}

/** 是否专有归档（预览器要给「先用别的工具解压」的指引）。 */
export function isArchiveLegacy(path: string): boolean {
  return ARCHIVE_LEGACY.has(extName(path))
}

/** 该形态能不能按文本读进 CodeMirror。 */
export function isTextKind(kind: FileKind): boolean {
  return kind === 'text' || kind === 'markdown' || kind === 'html'
}

/** 该形态该交给 DocPreview（含「读不了但有出路」的老式文档）。 */
export function hasRichPreview(kind: FileKind): boolean {
  return (
    kind === 'sheet' || kind === 'image' || kind === 'legacyDoc' || kind === 'archiveLegacy'
  )
}

/**
 * 能不能分屏：只有 CodeMirror 承载的正文可以（纯文本 / Markdown / HTML）。
 * 表格、图片这些预览器没有「第二栏」的概念，按了不该假装生效。
 */
export function canSplitView(kind: FileKind, viewType: string): boolean {
  if (viewType === 'hex') return false
  return kind === 'text' || kind === 'markdown' || kind === 'html'
}
