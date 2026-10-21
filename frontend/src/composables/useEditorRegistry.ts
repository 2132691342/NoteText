// 编辑器实例注册表：store 只存元数据，正文由这里按 tabId 取。
//
// 为什么分开：每次按键都把正文同步进 store 会让 Vue 的响应式系统
// 处理整篇文档的 diff，几百 KB 的日志文件直接卡死。所以 CodeMirror 的
// EditorState 由组件层持有，store 与外部代码通过这里的 getter 访问。
import { type EditorView } from '@codemirror/view'

const views = new Map<number, EditorView>()

/** 挂载一个 tab 的编辑器视图。 */
export function register(tabId: number, view: EditorView) {
  views.set(tabId, view)
}

/** 卸载。 */
export function unregister(tabId: number) {
  views.delete(tabId)
}

/** 取某个 tab 的视图。 */
export function get(tabId: number): EditorView | undefined {
  return views.get(tabId)
}

/** 取某个 tab 的全文。 */
export function getText(tabId: number): string {
  return views.get(tabId)?.state.doc.toString() ?? ''
}

/** 取某个 tab 的选中文本。 */
export function getSelection(tabId: number): string {
  const v = views.get(tabId)
  if (!v) return ''
  const { from, to } = v.state.selection.main
  return v.state.sliceDoc(from, to)
}

/** 取光标位置（1 基行列）。 */
export function getPosition(tabId: number): { line: number; col: number; selection_len: number } {
  const v = views.get(tabId)
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
  views.get(tabId)?.focus()
}

/** 当前聚焦的 tab id：命令要作用在「用户正在看的那个编辑器」上。 */
export function focusedTabId(): number | null {
  for (const [id, v] of views) {
    if (v.hasFocus) return id
  }
  return null
}
