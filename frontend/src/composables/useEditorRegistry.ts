// 编辑器实例注册表：store 只存元数据，正文由这里按 tabId 取。
// 分开的原因：每次按键都把正文同步进 store，会让 Vue 的响应式系统
import { type EditorView } from '@codemirror/view'
import type { Text } from '@codemirror/state'
import { ref } from 'vue'
import { undoDepth, redoDepth } from '@codemirror/commands'

/** 一个标签的编辑状态：视图 + 保存基线。 */
interface Entry {
  view: EditorView
  /** 上次保存（或载入）时的正文快照：脏标记的判据。 */
  baseline: Text
}

const entries = new Map<number, Entry>()

/** 每个标签的正文版本号：computed 消费方先读它，才能跟着正文变化重算。 */
const docVersions = ref(new Map<number, number>())

/** 取某个标签的正文版本号。未挂载为 0。 */
export function docVersionOf(tabId: number): number {
  return docVersions.value.get(tabId) ?? 0
}

/** 标记某个标签的正文变了：注册、载入完成、每次文档变更都调。
 *  原地 set：reactive Map 的写入本身就会触发依赖，整表拷贝是每键 O(n)。 */
export function bumpDocVersion(tabId: number) {
  docVersions.value.set(tabId, (docVersions.value.get(tabId) ?? 0) + 1)
}

/** 卸载时清掉版本记录：集合不该跟着关闭的标签无限增长。 */
export function dropDocVersion(tabId: number) {
  docVersions.value.delete(tabId)
}

/** 挂载一个 tab 的编辑器视图。baseline 由组件按载入内容给出。 */
export function register(tabId: number, view: EditorView, baseline: Text) {
  entries.set(tabId, { view, baseline })
  bumpDocVersion(tabId)
}

/** 卸载。 */
export function unregister(tabId: number) {
  entries.delete(tabId)
  dropDocVersion(tabId)
}

// ---- 纯文本适配器（表格网格这类非 CodeMirror 的可编辑视图） ----
// 保存链路（saveTabById）只认 getText / 脏标记 / 基线：给它一个 provider，
// 表格编辑器的保存、另存为、退出确认就能原样复用，不必塞进一条假 CM 实例。

/** 非编辑器正文的读写适配：表格编辑器持有自己的缓冲，这里只暴露两端。 */
export interface TextProvider {
  getText(): string
  /** 把正文整体替换成 text（保存前的行尾整理会调）。 */
  replaceText(text: string): void
}

const textProviders = new Map<number, TextProvider>()
const providerBaselines = new Map<number, string>()

/** 注册一个 tab 的文本适配器，initialText 即保存基线。 */
export function registerTextProvider(tabId: number, provider: TextProvider, initialText: string) {
  textProviders.set(tabId, provider)
  providerBaselines.set(tabId, initialText)
  bumpDocVersion(tabId)
}

/** 卸载适配器。 */
export function unregisterTextProvider(tabId: number) {
  textProviders.delete(tabId)
  providerBaselines.delete(tabId)
}

/**
 * 路由离开编辑器视图时暂存的正文：设置页 / 帮助页是独立路由，
 * 回来时编辑器从磁盘重读，未保存的修改必须从这份暂存里找回。
 * 只存脏文档（干净的重读磁盘就是同一份）；关闭标签后的残留条目
 * 等下次同名暂存覆盖或进程退出，量级可控。
 */
const stashedDocs = new Map<number, string>()

/** 暂存一个标签的正文（卸载前调；同一标签的主 / 副视图内容一致，覆写无妨）。 */
export function stashDoc(tabId: number, text: string) {
  stashedDocs.set(tabId, text)
}

/** 取暂存的正文。不移除：分屏的主副视图挂载时各取一次，取完仍留在表里等覆写。 */
export function takeStashedDoc(tabId: number): string | null {
  return stashedDocs.get(tabId) ?? null
}

/** openPath 已读到的正文暂存：挂载时直接消费，截断标签也不会被全量重读拒绝。 */
const pendingContents = new Map<number, string>()

/** 存一份挂载时要用的正文（openPath 读到内容后调）。 */
export function stagePendingContent(tabId: number, text: string) {
  pendingContents.set(tabId, text)
}

/** 取走暂存的正文（一次性）：有暂存时挂载不再读文件。 */
export function consumePendingContent(tabId: number): string | null {
  const text = pendingContents.get(tabId)
  if (text != null) pendingContents.delete(tabId)
  return text ?? null
}

/** 取某个 tab 的视图。 */
export function get(tabId: number): EditorView | undefined {
  return entries.get(tabId)?.view
}

/** 取某个 tab 的全文（编辑器视图或文本适配器）。 */
export function getText(tabId: number): string {
  const e = entries.get(tabId)
  if (e) return e.view.state.doc.toString()
  return textProviders.get(tabId)?.getText() ?? ''
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
  if (e) {
    e.baseline = e.view.state.doc
    return
  }
  const p = textProviders.get(tabId)
  if (p) providerBaselines.set(tabId, p.getText())
}

/** 正文是否与基线不同。挂载前（无视图）一律视为干净。 */
export function isDirty(tabId: number): boolean {
  const e = entries.get(tabId)
  if (e) return !sameDoc(e.view.state.doc, e.baseline)
  const p = textProviders.get(tabId)
  if (p) return p.getText() !== (providerBaselines.get(tabId) ?? '')
  return false
}

/**
 * 统一的「把正文整体替换成 text」：编辑器视图走 dispatch，
 * 文本适配器走自己的缓冲。保存前的行尾整理（textForSave）用它，
 * 不关心正文住在哪种视图里。
 */
export function replaceDocText(tabId: number, text: string): void {
  const e = entries.get(tabId)
  if (e) {
    const v = e.view
    v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: text } })
    return
  }
  textProviders.get(tabId)?.replaceText(text)
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
