// 编辑器实例注册表：store 只存元数据，正文由这里按 tabId 取。
//
// 分开的原因：每次按键都把正文同步进 store，会让 Vue 的响应式系统
// 处理整篇文档的 diff，几百 KB 的日志文件直接卡死。
import { type EditorView } from '@codemirror/view'
import type { Text } from '@codemirror/state'
import { undoDepth, redoDepth } from '@codemirror/commands'

/** 一个标签的编辑状态：视图 + 保存基线。 */
interface Entry {
  view: EditorView
  /** 上次保存（或载入）时的正文快照：脏标记的判据。 */
  baseline: Text
}

const entries = new Map<number, Entry>()

/** 挂载一个 tab 的编辑器视图。baseline 由组件按载入内容给出。 */
export function register(tabId: number, view: EditorView, baseline: Text) {
  entries.set(tabId, { view, baseline })
}

/** 卸载。 */
export function unregister(tabId: number) {
  entries.delete(tabId)
}

/** 取某个 tab 的视图。 */
export function get(tabId: number): EditorView | undefined {
  return entries.get(tabId)?.view
}

/** 取某个 tab 的全文。 */
export function getText(tabId: number): string {
  return entries.get(tabId)?.view.state.doc.toString() ?? ''
}

/** 取某个 tab 的选中文本。 */
export function getSelection(tabId: number): string {
  const v = entries.get(tabId)?.view
  if (!v) return ''
  const { from, to } = v.state.selection.main
  return v.state.sliceDoc(from, to)
}

/** 取光标位置（1 基行列）。 */
export function getPosition(tabId: number): { line: number; col: number; selection_len: number } {
  const v = entries.get(tabId)?.view
  if (!v) return { line: 1, col: 1, selection_len: 0 }
  const { from, to } = v.state.selection.main
  const line = v.state.doc.lineAt(from)
  return {
    line: line.number,
    col: from - line.from + 1,
    selection_len: to - from,
  }
}

/** 聚焦某个 tab 的编辑器。 */
export function focus(tabId: number) {
  entries.get(tabId)?.view.focus()
}

/** 当前聚焦的 tab id：命令要作用在「用户正在看的那个编辑器」上。 */
export function focusedTabId(): number | null {
  for (const [id, e] of entries) {
    if (e.view.hasFocus) return id
  }
  return null
}

/**
 * 把当前正文提交为新基线：保存成功后调用。
 *
 * 不调这一句的后果是「保存成功，标签却一直是脏的」——用户会以为没存上，
 * 于是反复按 Ctrl+S。
 */
export function commitBaseline(tabId: number) {
  const e = entries.get(tabId)
  if (e) e.baseline = e.view.state.doc
}

/** 正文是否与基线不同。挂载前（无视图）一律视为干净。 */
export function isDirty(tabId: number): boolean {
  const e = entries.get(tabId)
  if (!e) return false
  return !sameDoc(e.view.state.doc, e.baseline)
}

/**
 * 两份文档是否相同。
 *
 * 先比长度：打字会改长度，绝大多数按键在第一步就返回，不必扫全文。
 * 长度相同才逐段比（CodeMirror 的 eq 在首个差异处提前返回）。
 */
function sameDoc(a: Text, b: Text): boolean {
  return a.length === b.length && a.eq(b)
}

/** 撤销 / 重做栈深度：工具栏与菜单按它判可用性（0 即置灰）。 */
export function historyDepth(tabId: number): { undo: number; redo: number } {
  const e = entries.get(tabId)
  if (!e) return { undo: 0, redo: 0 }
  return { undo: undoDepth(e.view.state), redo: redoDepth(e.view.state) }
}

/** 所有已挂载的视图（id → view）。命令层找「当前聚焦的视图」用它。 */
export function allViews(): Array<[number, EditorView]> {
  return [...entries].map(([id, e]) => [id, e.view])
}
