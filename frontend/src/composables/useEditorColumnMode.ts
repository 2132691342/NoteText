// 列编辑：按矩形选区逐行插入 / 删除。
//
// CodeMirror 的 `rectangularSelection` 只负责「能选出矩形」，
// 真正往每一行的同一列插东西（Alt+拖动之后的输入）要自己算：

import { ref } from 'vue'
import type { EditorView } from '@codemirror/view'
import { EditorSelection } from '@codemirror/state'

/** 列模式开关：工具栏与状态栏读它给指示。 */
export const columnMode = ref(false)

export function setColumnMode(v: boolean) {
  columnMode.value = v
}

/**
 * 在矩形选区的每一行同列位置插入文本。
 *
 * 从后往前改：先改后面的行，否则前面的插入会让后面的偏移量失效。
 * 这是所有「多行同时编辑」实现里最容易错的一点。
 */
export function insertAtColumn(view: EditorView, text: string): boolean {
  const ranges = columnRanges(view)
  if (!ranges.length) return false
  // 按行号倒序：偏移失效问题从源头避开。
  const sorted = [...ranges].sort((a, b) => b.line - a.line)
  const changes = sorted.map((r) => ({
    from: r.col,
    insert: text.split('\n').join(''),
  }))
  view.dispatch({ changes, scrollIntoView: true })
  return true
}

/** 删除矩形选区覆盖的列区间（每行删掉同一段宽度）。 */
export function deleteAtColumn(view: EditorView): boolean {
  const ranges = columnRanges(view)
  if (!ranges.length) return false
  const sorted = [...ranges].sort((a, b) => b.line - a.line)
  const changes = sorted
    .filter((r) => r.to > r.col)
    .map((r) => ({ from: r.col, to: r.to }))
  if (!changes.length) return false
  view.dispatch({ changes })
  return true
}

/** 取矩形选区里每一行的列位置。空选区返回空数组。 */
function columnRanges(view: EditorView): Array<{ line: number; col: number; to: number }> {
  const out: Array<{ line: number; col: number; to: number }> = []
  for (const r of view.state.selection.ranges) {
    if (r.empty) continue
    const line = view.state.doc.lineAt(r.from)
    out.push({
      line: line.number,
      col: r.from,
      to: Math.min(r.to, line.to),
    })
  }
  return out
}

/**
 * 把当前选区转成逐行选区（列选中的等价物）。
 *
 * 用于「选中一块 → 按列模式 → 输入」的前置步骤：让后续插入落在每行同列。
 */
export function selectLinesAsColumns(view: EditorView) {
  const { from, to } = view.state.selection.main
  if (from === to) return
  const startLine = view.state.doc.lineAt(from).number
  const endLine = view.state.doc.lineAt(to).number
  const ranges = []
  for (let n = startLine; n <= endLine; n++) {
    const line = view.state.doc.line(n)
    ranges.push(EditorSelection.range(line.from, Math.min(line.to, line.from + (to - from))))
  }
  view.dispatch({ selection: EditorSelection.create(ranges) })
}
