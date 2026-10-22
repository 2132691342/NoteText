// 编辑器动作层：所有「对正文做一件事」的操作都从这里进 CodeMirror。
//
// 为什么不用 document 上的自定义事件：分屏时同一个标签有两个视图，
// 事件会被两个视图同时收到；事件也没有返回值，调用方拿不到有没有成功。
// 直接按 tabId 取视图再操作，只有一个答案。
import type { EditorView } from '@codemirror/view'
import { EditorSelection, type ChangeSpec } from '@codemirror/state'
import {
  undo,
  redo,
  selectAll,
  deleteLine,
  moveLineUp,
  moveLineDown,
  copyLineDown,
  indentMore,
  indentLess,
  toggleComment,
  selectLine,
} from '@codemirror/commands'
import { get, getText, allViews } from '@/composables/useEditorRegistry'

/** 取目标视图：给了 tabId 用它，否则用当前聚焦的那个。 */
export function viewOf(tabId?: number | null): EditorView | undefined {
  if (tabId != null) return get(tabId)
  for (const [, v] of allViews()) {
    if (v.hasFocus) return v
  }
  return undefined
}

/** 撤销。返回是否真的撤销了一步（工具栏按它给反馈）。 */
export function doUndo(tabId?: number | null): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  return undo(v)
}

/** 重做。 */
export function doRedo(tabId?: number | null): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  return redo(v)
}

/** 全选。 */
export function doSelectAll(tabId?: number | null): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  return selectAll(v)
}

/** 删除当前行（多光标时逐行删）。 */
export function doDeleteLine(tabId?: number | null): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  return deleteLine(v)
}

/** 当前行上移 / 下移。 */
export function doMoveLine(tabId: number | null | undefined, dir: 'up' | 'down'): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  return dir === 'up' ? moveLineUp(v) : moveLineDown(v)
}

/** 复制当前行到下一行（Ctrl+D 的语义）。 */
export function doDuplicateLine(tabId?: number | null): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  return copyLineDown(v)
}

/** 选中当前行。 */
export function doSelectLine(tabId?: number | null): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  return selectLine(v)
}

/** 缩进 / 反缩进：无选区时按一个缩进单位缩当前行。 */
export function doIndent(tabId: number | null | undefined, out = false): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  return out ? indentLess(v) : indentMore(v)
}

/**
 * 在选区处插入文本（粘贴、片段、宏回放都走这里）。
 *
 * 插入是一次事务：撤销栈里是一步，用户按一次 Ctrl+Z 就能整体退回。
 */
export function doInsert(tabId: number | null | undefined, text: string): boolean {
  const v = viewOf(tabId)
  if (!v || !text) return false
  const { from, to } = v.state.selection.main
  v.dispatch({
    changes: { from, to, insert: text },
    selection: { anchor: from + text.length },
    scrollIntoView: true,
  })
  v.focus()
  return true
}

/** 删除当前选区（剪切用）。 */
export function doDeleteSelection(tabId: number | null | undefined): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  const { from, to } = v.state.selection.main
  if (from === to) return false
  v.dispatch({ changes: { from, to, insert: '' }, selection: { anchor: from } })
  v.focus()
  return true
}

/** 用新选区替换选区内容。 */
export function doReplaceRange(
  tabId: number | null | undefined,
  from: number,
  to: number,
  text: string,
): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  v.dispatch({
    changes: { from, to, insert: text },
    selection: { anchor: from, head: from + text.length },
    scrollIntoView: true,
  })
  v.focus()
  return true
}

/** 把整篇文档替换掉（工具箱「应用结果」）。 */
export function doReplaceDoc(tabId: number | null | undefined, text: string): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  v.dispatch({
    changes: { from: 0, to: v.state.doc.length, insert: text },
    scrollIntoView: true,
  })
  return true
}

/**
 * 批量替换整篇文档：changes 一次提交。
 *
 * 逐个 dispatch 会让撤销要按 N 次；一次提交才是「替换全部」该有的撤销粒度。
 */
export function doApplyChanges(
  tabId: number | null | undefined,
  changes: ChangeSpec[],
): boolean {
  const v = viewOf(tabId)
  if (!v || changes.length === 0) return false
  v.dispatch({ changes, scrollIntoView: true })
  return true
}

/** 选中区间并滚到中间。 */
export function doSelectRange(
  tabId: number | null | undefined,
  from: number,
  to: number,
): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  const len = v.state.doc.length
  const a = Math.max(0, Math.min(from, len))
  const b = Math.max(a, Math.min(to, len))
  v.dispatch({ selection: { anchor: a, head: b } })
  scrollPosToCenter(v, a)
  v.focus()
  return true
}

/** 跳到指定行（1 基），保留列位置不越界。 */
export function doGotoLine(tabId: number | null | undefined, line: number): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  const n = Math.min(Math.max(line, 1), v.state.doc.lines)
  const l = v.state.doc.line(n)
  v.dispatch({ selection: { anchor: l.from } })
  scrollPosToCenter(v, l.from)
  v.focus()
  return true
}

/**
 * 把某个位置滚到视口中部。
 *
 * 用 scrollDOM 直接算而不是 EditorView.scrollIntoView(effect)：
 * effect 的 y:'center' 在小文档里不生效，而这里需要「稳稳在中间」。
 */
function scrollPosToCenter(view: EditorView, pos: number) {
  const block = view.lineBlockAt(pos)
  const box = view.scrollDOM
  const delta = block.top - box.clientHeight / 2 + block.height / 2
  box.scrollTop = Math.max(0, delta)
}

/** 逐行多光标：每一行一个光标点（列编辑的轻量替代）。 */
export function doAddCursorPerLine(tabId?: number | null): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  const { from, to } = v.state.selection.main
  const startLine = v.state.doc.lineAt(from)
  const endLine = v.state.doc.lineAt(to)
  if (endLine.number <= startLine.number) return false
  // 列偏移从选区起点算：每行同一列，超出行长就贴在行尾。
  const column = from - startLine.from
  const ranges = []
  for (let n = startLine.number; n <= endLine.number; n += 1) {
    const line = v.state.doc.line(n)
    ranges.push(EditorSelection.cursor(Math.min(line.from + column, line.to)))
  }
  v.dispatch({ selection: EditorSelection.create(ranges) })
  v.focus()
  return true
}

/**
 * 切换行注释。
 *
 * 优先后端给出的注释语法（30 种语言一张表），失败时退回 CodeMirror 的
 * toggleComment（它有语言语法树，能处理块注释）。
 */
export async function doToggleComment(
  tabId: number | null | undefined,
  backendToggle: () => Promise<string>,
): Promise<boolean> {
  const v = viewOf(tabId)
  if (!v) return false
  const { from, to } = v.state.selection.main
  // 无选区时交给 CodeMirror：它按语法树处理整行 / 整块。
  if (from === to) return toggleComment(v)
  try {
    const out = await backendToggle()
    if (!out) return false
    v.dispatch({
      changes: { from, to, insert: out },
      selection: { anchor: from, head: from + out.length },
    })
    return true
  } catch {
    return toggleComment(v)
  }
}

/** 大小写转换：只改选区，没选区时改当前词。 */
export async function doConvertCase(
  tabId: number | null | undefined,
  convert: (text: string) => Promise<string>,
): Promise<boolean> {
  const v = viewOf(tabId)
  if (!v) return false
  const { from, to } = v.state.selection.main
  const selected = v.state.sliceDoc(from, to)
  const source = selected || wordAt(v)
  if (!source) return false
  const out = await convert(source)
  if (out === source) return true
  const targetFrom = selected ? from : v.state.selection.main.from
  v.dispatch({
    changes: { from: targetFrom, to: targetFrom + source.length, insert: out },
    selection: { anchor: targetFrom, head: targetFrom + out.length },
  })
  return true
}

/** 光标所在的词（找不到时返回整行去空白）。 */
function wordAt(view: EditorView): string {
  const pos = view.state.selection.main.head
  const line = view.state.doc.lineAt(pos)
  const col = pos - line.from
  const text = line.text
  const isWord = (c: string) => /[\w\u4e00-\u9fa5]/.test(c)
  let start = col
  let end = col
  while (start > 0 && isWord(text[start - 1])) start -= 1
  while (end < text.length && isWord(text[end])) end += 1
  return text.slice(start, end)
}

/** 当前标签的全文（方便调用方少一层 import）。 */
export function textOf(tabId: number | null | undefined): string {
  return tabId != null ? getText(tabId) : ''
}

/**
 * 选区所在的行区间（多光标时取第一个选区的首尾行）。
 *
 * 行操作统一走它：先算区间再一次 dispatch。逐行 dispatch 会让
 * 一次「排序选中行」进 N 步撤销栈，用户按一次 Ctrl+Z 只退回一行。
 */
function selectedLineRange(view: EditorView): { from: number; to: number; text: string } | null {
  const sel = view.state.selection.main
  const startLine = view.state.doc.lineAt(sel.from)
  const endLine = view.state.doc.lineAt(sel.to)
  const from = startLine.from
  const to = Math.min(sel.to === startLine.to ? startLine.to : endLine.to, view.state.doc.length)
  return { from, to, text: view.state.sliceDoc(from, to) }
}

/** 用一段新文本替换行区间，并把选区收在新文本两端。 */
function replaceRange(view: EditorView, from: number, to: number, text: string) {
  view.dispatch({
    changes: { from, to, insert: text },
    selection: { anchor: from, head: from + text.length },
  })
}

/** 大小写三种：大写 / 小写 / 首字母大写。有选区改选区，无选区改当前词。 */
export function doCaseTransform(
  tabId: number | null | undefined,
  kind: 'upper' | 'lower' | 'title',
): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  const { from, to } = v.state.selection.main
  const empty = from === to
  const source = empty ? wordAt(v) : v.state.sliceDoc(from, to)
  if (!source) return false
  const out =
    kind === 'upper'
      ? source.toUpperCase()
      : kind === 'lower'
        ? source.toLowerCase()
        : source.replace(/\b\p{L}+/gu, (w) => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase())
  if (out === source) return true
  const targetFrom = empty ? from - source.length : from
  v.dispatch({
    changes: { from: targetFrom, to: targetFrom + source.length, insert: out },
    selection: { anchor: targetFrom, head: targetFrom + out.length },
  })
  return true
}

/** 排序选中行：升序 / 降序。只按行文本比，不动缩进与空行。 */
export function doSortLines(
  tabId: number | null | undefined,
  dir: 'asc' | 'desc',
): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  const range = selectedLineRange(v)
  if (!range) return false
  const lines = range.text.split('\n')
  if (lines.length < 2) return false
  // 中文按 localeCompare：按码点排会把「变量」排到「ABC」后面，看不出顺序意义。
  const sorted = [...lines].sort((a, b) =>
    dir === 'asc' ? a.localeCompare(b, 'zh-Hans-CN') : b.localeCompare(a, 'zh-Hans-CN'),
  )
  if (sorted.join('\n') === range.text) return true
  replaceRange(v, range.from, range.to, sorted.join('\n'))
  return true
}

/**
 * 去重复行：保序去重（保留第一次出现的位置）。
 *
 * 保序是关键：用户拿它整理配置清单，顺序本身有意义；
 * 换成按字母排等于顺手改了他的语义。
 */
export function doDedupLines(tabId: number | null | undefined): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  const range = selectedLineRange(v)
  if (!range) return false
  const lines = range.text.split('\n')
  const seen = new Set<string>()
  const kept = lines.filter((l) => {
    const key = l.trim()
    if (key === '' || seen.has(key)) return false
    seen.add(key)
    return true
  })
  if (kept.length === lines.length) return true
  replaceRange(v, range.from, range.to, kept.join('\n'))
  return true
}

/** 去行尾空白：每行右侧的空白一律清掉。 */
export function doTrimTrailing(tabId: number | null | undefined): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  const range = selectedLineRange(v)
  if (!range) return false
  const out = range.text
    .split('\n')
    .map((l) => l.replace(/[ \t]+$/, ''))
    .join('\n')
  if (out === range.text) return true
  replaceRange(v, range.from, range.to, out)
  return true
}

/** 反转选中行的顺序。 */
export function doReverseLines(tabId: number | null | undefined): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  const range = selectedLineRange(v)
  if (!range) return false
  if (range.text.split('\n').length < 2) return false
  replaceRange(v, range.from, range.to, range.text.split('\n').reverse().join('\n'))
  return true
}

/**
 * Tab ↔ 空格互转：作用于选中行（无选区时是全文）。
 *
 * `leading` 只转行首的空白：正文里的制表符（比如 Markdown 表格）不该动，
 * 那是内容的一部分，不是缩进。
 */
export function doConvertIndent(
  tabId: number | null | undefined,
  dir: 'tab-to-space' | 'space-to-tab',
  tabSize: number,
  leadingOnly = false,
): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  const sel = v.state.selection.main
  const empty = sel.from === sel.to
  const range = empty
    ? { from: 0, to: v.state.doc.length, text: v.state.doc.toString() }
    : selectedLineRange(v)
  if (!range) return false

  const convertLine = (line: string): string => {
    if (dir === 'tab-to-space') {
      if (leadingOnly) return line.replace(/^\t+/, (m) => ' '.repeat(m.length * tabSize))
      return line.replace(/\t/g, ' '.repeat(tabSize))
    }
    // 空格转 Tab：按 tabSize 取整，不足以凑一整组的保留为空格
    // （硬转会把对齐结构打乱）。
    if (leadingOnly) {
      return line.replace(/^ +/, (m) => {
        const tabs = Math.floor(m.length / tabSize)
        return '\t'.repeat(tabs) + ' '.repeat(m.length - tabs * tabSize)
      })
    }
    return line.replace(/ {2,}/g, (m) => {
      const tabs = Math.floor(m.length / tabSize)
      return '\t'.repeat(tabs) + ' '.repeat(m.length - tabs * tabSize)
    })
  }

  const out = range.text.split('\n').map(convertLine).join('\n')
  if (out === range.text) return true
  v.dispatch({
    changes: { from: range.from, to: range.to, insert: out },
    selection: empty
      ? { anchor: sel.from }
      : { anchor: range.from, head: range.from + out.length },
  })
  return true
}

/**
 * 书签行操作：对「有书签的行」整批做一件事。
 *
 * 行号集合由调用方给（gutter 存在 CodeEditor 里，动作层不持有 UI 状态）。
 * 从后往前改：从前往后改会让后续行号全部失效。
 */
export type BookmarkLineOp = 'copy' | 'cut' | 'delete' | 'delete-unmarked'

export function doBookmarkLineOp(
  tabId: number | null | undefined,
  op: BookmarkLineOp,
  markedLines: Set<number>,
): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  const total = v.state.doc.lines
  const lines: string[] = []
  for (let n = 1; n <= total; n += 1) lines.push(v.state.doc.line(n).text)

  let changed = false
  if (op === 'copy' || op === 'cut') {
    const picked = lines.filter((_, i) => markedLines.has(i + 1))
    if (picked.length === 0) return false
    bookmarkClipboard = picked.join('\n')
    if (op === 'copy') return true
    changed = true
  }

  // 删除与剪切都要把行剔掉：删除只剔书签行，剪切剔书签行，
  // 「删非书签行」反过来剔。三种在 forEach 里一次走完。
  const kept: string[] = []
  lines.forEach((l, i) => {
    const marked = markedLines.has(i + 1)
    if (op === 'delete' && marked) return
    if (op === 'delete-unmarked' && !marked) return
    if (op === 'cut' && marked) return
    kept.push(l)
  })
  if (!changed) return false
  replaceRange(v, 0, v.state.doc.length, kept.join('\n'))
  return true
}

/** 书签行剪贴板：模块级单例，与书签集合一样不跨标签共享。 */
let bookmarkClipboard = ''

/** 取书签行剪贴板（粘贴书签行用）。 */
export function bookmarkClip(): string {
  return bookmarkClipboard
}

/** 在光标处插入书签行剪贴板。 */
export function doPasteBookmarkLines(tabId: number | null | undefined): boolean {
  const v = viewOf(tabId)
  if (!v || !bookmarkClipboard) return false
  const pos = v.state.selection.main.head
  const line = v.state.doc.lineAt(pos)
  const insert = bookmarkClipboard + '\n'
  v.dispatch({
    changes: { from: line.to, insert },
    selection: { anchor: line.to + insert.length },
  })
  return true
}

/**
 * 5 色标记：给选中的词 / 选区加一层背景色装饰。
 *
 * 用 ViewPlugin 的装饰而不是改正文：标记是「看法」不是「内容」，
 * 写进正文会让用户保存出一份带颜色代码的文件。
 */
export type MarkColor = 1 | 2 | 3 | 4 | 5

/** 当前所有标记：颜色 → 词集合。 */
const marks = new Map<MarkColor, Set<string>>()

/** 取某个颜色的标记词集合（空集合表示没有）。 */
export function markWords(color: MarkColor): ReadonlySet<string> {
  return marks.get(color) ?? new Set<string>()
}

/** 所有标记的并集：用来决定要不要挂装饰插件。 */
export function allMarkWords(): Set<string> {
  const all = new Set<string>()
  for (const set of marks.values()) for (const w of set) all.add(w)
  return all
}

/** 给选区 / 光标处的词上标记。 */
export function doMarkSelection(
  tabId: number | null | undefined,
  color: MarkColor,
): boolean {
  const v = viewOf(tabId)
  if (!v) return false
  const { from, to } = v.state.selection.main
  const word = from === to ? wordAt(v) : v.state.sliceDoc(from, to)
  if (!word.trim()) return false
  if (!marks.has(color)) marks.set(color, new Set())
  marks.get(color)!.add(word.trim())
  notifyMarksChanged()
  return true
}

/** 清一个颜色的全部标记。 */
export function doClearMarks(color?: MarkColor): boolean {
  if (color == null) marks.clear()
  else marks.delete(color)
  notifyMarksChanged()
  return true
}

/**
 * 标记变了要重绘所有视图：装饰在 ViewPlugin 里，插件本身不订阅这里的状态。
 *
 * 用一个自定义事件而不是 import 各组件：动作层不能反向依赖组件层
 * （依赖方向是组件 → composable）。
 */
function notifyMarksChanged() {
  window.dispatchEvent(new CustomEvent('nt-marks-changed'))
}

/** 标记载体：把标记词集合传进来的渲染函数。由 CodeEditor 提供。 */
let marksRenderer: ((words: Set<string>) => void) | null = null

export function setMarksRenderer(fn: ((words: Set<string>) => void) | null) {
  marksRenderer = fn
}

export function renderMarks() {
  marksRenderer?.(allMarkWords())
}
