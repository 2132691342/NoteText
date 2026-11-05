// 文件形态判定：决定一个文件进编辑器、预览器还是十六进制视图。
// 一处说了算——store 用它决定「读文本还是只建标签」，EditorArea 用它选组件。
import { extName } from '@/utils'

/** 文件的渲染形态：legacyDoc 给外部程序卡片，binary 进十六进制视图。 */
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
const SHEET = new Set(['xlsx', 'xlsm', 'xls', 'csv', 'tsv', 'ods', 'jsonl', 'ndjson'])

/** HTML：可编辑的文本，预览走沙箱 iframe（不执行脚本）。 */
const HTML = new Set(['html', 'htm', 'xhtml'])

const MARKDOWN = new Set(['md', 'markdown', 'mdown', 'mkd', 'mdx'])

/** 不做内置渲染的文档格式：Word / PDF / PPT / EPUB 等内嵌渲染体积大、还原度有限，统一给「用外部程序打开」出路。 */
const LEGACY_DOC = new Set([
  'doc', 'docx', 'docm', 'dot', 'dotx', 'rtf', 'ppt', 'pptx', 'pps', 'pptm',
  'odt', 'odp', 'pages', 'key', 'epub', 'mobi', 'pdf',
])

/** 没有内置解压能力的归档格式：预览器给「明确出路」卡片而非十六进制。 */
const ARCHIVE_LEGACY = new Set(['rar', 'lzh', 'lha', 'cab', 'iso', 'img'])

/** 扩展名即可断定的二进制外壳：头部嗅探认不出以纯文本开头的压缩包。 */
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

/** 判定文件形态：已知扩展名按表走，未知扩展名才用后端 NUL 嗅探兜底。 */
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

/** CSV / TSV：文本性质的表格，走可编辑网格（xlsx/ods 走另一条链路）。 */
export function isDelimitedSheet(path: string): boolean {
  const ext = extName(path)
  return ext === 'csv' || ext === 'tsv'
}

/**
 * JSONL / NDJSON：每行一个 JSON 对象，走可编辑表格网格。
 *
 * 顶层 JSON 数组也走这条：数组元素可以跨行，按行切不成立，
 * 但每个元素可以当一条记录。
 */
export function isJsonlSheet(path: string): boolean {
  const ext = extName(path)
  return ext === 'jsonl' || ext === 'ndjson'
}

/** 能不能走大表格网格（分页取数，不整篇进内存）。 */
export function isTableGrid(path: string): boolean {
  return isDelimitedSheet(path) || isJsonlSheet(path)
}

/** XLSX / XLSM 走「读单元格 → 改 XML 写回」网格，样式图表写回时原地保留；ODS 走只读预览，可另存为 xlsx 后再编辑。 */
export function isWorkbookSheet(path: string): boolean {
  const ext = extName(path)
  return ext === 'xlsx' || ext === 'xlsm'
}

/**
 * 只能只读预览的电子表格：老 .xls（BIFF）与 ODS。
 * 它们在预览器里可看、可另存为 xlsx，但不能原地改单元格。
 */
export function isReadonlySheet(path: string): boolean {
  const ext = extName(path)
  return ext === 'xls' || ext === 'ods'
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
