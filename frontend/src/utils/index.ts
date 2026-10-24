// 通用工具函数：纯函数为主，可单测。
// 放这里的判据：不依赖 Vue、不依赖 store、输入输出都是基础类型。

import { api } from '@/api'

/** 延迟执行（防抖）。 */
export function debounce<A extends unknown[]>(
  fn: (...args: A) => void,
  ms: number,
): ((...args: A) => void) & { cancel: () => void } {
  let timer: number | null = null
  const wrapped = (...args: A) => {
    if (timer !== null) window.clearTimeout(timer)
    timer = window.setTimeout(() => {
      timer = null
      fn(...args)
    }, ms)
  }
  wrapped.cancel = () => {
    if (timer !== null) window.clearTimeout(timer)
    timer = null
  }
  return wrapped
}

/** 立即执行一次，随后 ms 内的重复调用被吞掉（节流）。 */
export function throttle<A extends unknown[]>(fn: (...args: A) => void, ms: number) {
  let last = 0
  return (...args: A) => {
    const now = Date.now()
    if (now - last < ms) return
    last = now
    fn(...args)
  }
}

/** 文件大小的人话形式：1.4 MB 而不是 1468006。 */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let v = bytes / 1024
  let i = 0
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024
    i += 1
  }
  // 一位小数：两位在密集的状态栏里太占宽度。
  return `${v.toFixed(v >= 10 ? 0 : 1)} ${units[i]}`
}

/** 毫秒时间戳 → 本地时间字符串。 */
export function formatTime(ms: number): string {
  if (!ms) return ''
  const d = new Date(ms)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** 路径中段省略：状态栏放不下完整路径。 */
export function ellipsisMiddle(path: string, max: number): string {
  if (path.length <= max) return path
  const head = Math.ceil((max - 1) * 0.45)
  const tail = max - 1 - head
  return `${path.slice(0, head)}…${path.slice(path.length - tail)}`
}

/** 取文件名。 */
export function baseName(path: string): string {
  const parts = path.replace(/\//g, '\\').split('\\')
  return parts[parts.length - 1] || path
}

/** 取目录。 */
export function dirName(path: string): string {
  const i = Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/'))
  return i > 0 ? path.slice(0, i) : ''
}

/** 取扩展名（不含点，小写）。 */
export function extName(path: string): string {
  const name = baseName(path)
  const i = name.lastIndexOf('.')
  return i > 0 ? name.slice(i + 1).toLowerCase() : ''
}

/** 拼接路径（Windows 语义：统一用反斜杠，不重复分隔符）。 */
export function joinPath(dir: string, name: string): string {
  if (!dir) return name
  const sep = dir.endsWith('\\') || dir.endsWith('/') ? '' : '\\'
  return `${dir}${sep}${name}`
}

/** 按 glob 匹配文件名：文件树忽略规则用。
 *  只支持 `*` 与 `?`，够「node_modules / *.log」这类常见规则。 */
export function globMatch(pattern: string, name: string): boolean {
  // 转义正则元字符，再把 glob 通配符换成分组。
  const re = new RegExp(
    '^' +
      pattern
        .replace(/[.+^${}()|[\]\\]/g, '\\$&')
        .replace(/\*/g, '.*')
        .replace(/\?/g, '.') +
      '$',
    'i',
  )
  return re.test(name)
}

/** 写剪贴板：优先走后端命令（WebView2 可能拒掉 writeText），浏览器开发态回退 Web API。 */
export async function copyText(text: string): Promise<boolean> {
  try {
    await api.writeClipboard(text)
    return true
  } catch {
    /* 无后端（浏览器 dev）或剪贴板被占：继续试 Web API */
  }
  try {
    await navigator.clipboard.writeText(text)
    return true
  } catch {
    try {
      const ta = document.createElement('textarea')
      ta.value = text
      ta.style.position = 'fixed'
      ta.style.opacity = '0'
      document.body.appendChild(ta)
      ta.select()
      const ok = document.execCommand('copy')
      document.body.removeChild(ta)
      return ok
    } catch {
      return false
    }
  }
}

/** 读剪贴板。读权限比写权限严（WebView2 直接不给渲染层），走后端命令；
 *  失败返回空串由调用方提示。 */
export async function readClipboard(): Promise<string> {
  try {
    return await api.readClipboard()
  } catch {
    /* 无后端（浏览器 dev）：继续试 Web API */
  }
  try {
    return await navigator.clipboard.readText()
  } catch {
    return ''
  }
}

/** 按字节偏移算行列（1 基）。与后端 `pkg::text::line_col_at` 同构。 */
export function lineColAt(text: string, byteOffset: number): [number, number] {
  let line = 1
  let col = 1
  for (let i = 0; i < byteOffset && i < text.length; i += 1) {
    if (text[i] === '\n') {
      line += 1
      col = 1
    } else {
      col += 1
    }
  }
  return [line, col]
}

/** 行号区间 → 文本：把 [from, to] 之间的行取出来。 */
export function sliceLines(text: string, from: number, to: number): string {
  const lines = text.split('\n')
  return lines.slice(Math.max(0, from - 1), to).join('\n')
}

/** 统计：与后端 `pkg::text::stats` 同构（字符数按 char 不是 byte）。 */
export function textStats(text: string) {
  const lines = text === '' ? 0 : text.split('\n').length
  const nonBlank = text.split('\n').filter((l) => l.trim() !== '').length
  return {
    lines,
    non_blank_lines: nonBlank,
    chars: [...text].length,
    words: text.split(/\s+/).filter(Boolean).length,
  }
}

/** 生成未命名标签的序号：从 1 开始，跳过已占用的。 */
export function nextUntitledIndex(taken: number[]): number {
  let i = 1
  while (taken.includes(i)) i += 1
  return i
}

/** 归档扩展名全集：与后端 `ArchiveFormat::extensions` 一一对应，改后端要同步这里。
 *  用于右键菜单分流（压缩包给「打开 / 解压」，普通文件给「压缩」）。 */
const ARCHIVE_EXTS = new Set([
  '7z',
  'zip',
  'tar',
  'tgz',
  'taz',
  'gz',
  'gzip',
  'bz2',
  'bzip2',
  'boz',
  'zst',
  'zstd',
  'tzst',
  'xz',
  'txz',
  'lz4',
  'br',
])

/** 是否为可打开的归档文件（按扩展名；后端 open 时还会做魔数复核）。 */
export function isArchivePath(path: string): boolean {
  return ARCHIVE_EXTS.has(extName(path))
}

/** 是否为分卷序列首卷（.001）：树右键对它给「合并分卷」。 */
export function isSplitFirstPart(path: string): boolean {
  return /^\d{3}$/.test(extName(path))
}
