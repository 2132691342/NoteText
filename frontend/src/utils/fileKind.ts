// 文件形态判定：决定一个文件进编辑器、预览器还是十六进制视图。
// 一处说了算——store 用它决定「读文本还是只建标签」，EditorArea 用它选组件。
import { extName } from '@/utils'

/**
 * 文件的渲染形态。
 *
 * `legacyDoc` 是「确实是文档、但内置渲染器读不了」（.doc / .ppt / .odt）：
 * 它给的是「用外部程序打开」的卡片，而不是一屏十六进制。
 * `binary` 则是可执行文件 / 压缩包这类根本不该当文档看的东西。
 */
export type FileKind = 'text' | 'markdown' | 'html' | 'image' | 'sheet' | 'docx' | 'pdf' | 'legacyDoc' | 'binary'

const IMAGE = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'ico', 'avif'])

/** 表格：xlsx 库能读老 .xls，CSV/TSV 按同一条链路转成表。 */
const SHEET = new Set(['xlsx', 'xlsm', 'xls', 'csv', 'tsv', 'ods'])

/** HTML：可编辑的文本，预览走沙箱 iframe（不执行脚本）。 */
const HTML = new Set(['html', 'htm', 'xhtml'])

/** docx-preview 只认 OOXML；老 .doc 是 OLE 复合文档，读不了。 */
const DOCX = new Set(['docx', 'docm', 'dotx'])

const PDF = new Set(['pdf'])

const MARKDOWN = new Set(['md', 'markdown', 'mdown', 'mkd', 'mdx'])

/** 老式 / 非 OOXML 文档：没有内嵌渲染器，但仍然是「用户点开想看内容」的文件。 */
const LEGACY_DOC = new Set([
  'doc', 'dot', 'rtf', 'ppt', 'pptx', 'pps', 'odt', 'odp', 'ods',
  'pages', 'key', 'numbers', 'epub', 'mobi',
])

/**
 * 扩展名即可断定的二进制外壳。
 *
 * 为什么要列出来而不是全靠内容嗅探：嗅探只读前 8KB，
 * 一个以纯文本开头的压缩包 / 可执行文件嗅不出 NUL，会被当成文本打开，
 * 编辑器里就是一屏乱码，保存还会写坏原文件。
 */
const BINARY = new Set([
  'zip', '7z', 'rar', 'gz', 'tgz', 'bz2', 'xz', 'tar', 'cab', 'iso', 'img',
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
  if (DOCX.has(ext)) return 'docx'
  if (PDF.has(ext)) return 'pdf'
  if (MARKDOWN.has(ext)) return 'markdown'
  if (HTML.has(ext)) return 'html'
  if (LEGACY_DOC.has(ext)) return 'legacyDoc'
  if (BINARY.has(ext)) return 'binary'
  return isBinary ? 'binary' : 'text'
}

/** 该形态能不能按文本读进 CodeMirror。 */
export function isTextKind(kind: FileKind): boolean {
  return kind === 'text' || kind === 'markdown' || kind === 'html'
}

/** 该形态该交给 DocPreview（含「读不了但有出路」的老式文档）。 */
export function hasRichPreview(kind: FileKind): boolean {
  return kind === 'docx' || kind === 'pdf' || kind === 'sheet' || kind === 'image' || kind === 'legacyDoc'
}
