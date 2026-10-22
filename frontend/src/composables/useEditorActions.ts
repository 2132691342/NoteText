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
