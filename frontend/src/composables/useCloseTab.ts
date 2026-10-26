// 关闭标签的唯一入口：带未保存确认。
// 为什么单独成文件而不是塞在命令层：标签栏（点 X / 中键 / 右键）与命令层
import { i18n } from '@/i18n'
import { useEditorStore, languageOf } from '@/stores/editor'
import { useFileTreeStore } from '@/stores/fileTree'
import { useToastStore } from '@/stores/toast'
import { api, normalizeError } from '@/api'
import { getText, commitBaseline } from '@/composables/useEditorRegistry'
import { doReplaceDoc } from '@/composables/useEditorActions'
import { useSettingsStore } from '@/stores/settings'
import { askUnsaved } from '@/components/dialogs/UnsavedDialog.vue'
import { baseName } from '@/utils'

/**
 * 取文案函数。
 *
 * **不能用 `useI18n()`**：它依赖当前组件实例，而这些函数是由命令表
 * （菜单 / 快捷键 / 标签栏点击）调用的，调用栈里没有组件 setup——
 * 那样会抛 SyntaxError（内部去读组件实例的 i18n 元数据）。
 * 全局实例在 i18n.ts 里，这里直接用它。
 */
function tr(key: string, named: Record<string, unknown> = {}): string {
  return i18n.global.t(key, named)
}

/**
 * 写盘前的正文整理：去掉行尾空白、补末尾换行。
 *
 * 整理结果同时写回编辑器缓冲区——只改磁盘不改缓冲区会让「已保存」
 * 与屏幕上看到的内容不一致，下一次保存又会重复整理一遍。
 */
function textForSave(id: number): string {
  const file = useSettingsStore().config.file
  let text = getText(id)
  if (!text) return text
  if (file.trim_trailing_ws) {
    text = text
      .split('\n')
      .map((line) => line.replace(/[ \t]+$/, ''))
      .join('\n')
  }
  if (file.ensure_final_newline && !text.endsWith('\n')) text += '\n'
  if (text !== getText(id)) doReplaceDoc(id, text)
  return text
}

/** 保存一个标签。返回 true = 真的写盘成功。 */
export async function saveTabById(id: number): Promise<boolean> {
  const editor = useEditorStore()
  const tree = useFileTreeStore()
  const toast = useToastStore()

  const tab = editor.tabs.find((x) => x.id === id)
  if (!tab) return false
  // 未命名标签：先另存为拿到路径。
  if (!tab.path || tab.path.startsWith('draft:')) return saveTabAsById(id)
  // 截断的标签只有部分内容：写回就是把剩余部分删掉，直接拒绝。
  if (tab.truncated) {
    toast.warn(tr('toast.saveBlockedTruncated'))
    return false
  }
  try {
    await api.writeFile(tab.path, textForSave(id), tab.encoding, tab.line_ending, tab.has_bom)
    commitBaseline(id)
    editor.markDirty(id, false)
    tree.markDirty(tab.path, false)
    void api.touchRecent(tab.path)
    toast.ok(tr('toast.saved', { name: tab.title }))
    return true
  } catch (e) {
    toast.bad(tr('error.writeFailed', { reason: normalizeError(e).message }))
    return false
  }
}

/** 另存为。返回 false = 用户取消或写失败。 */
export async function saveTabAsById(id: number): Promise<boolean> {
  const editor = useEditorStore()
  const toast = useToastStore()

  const tab = editor.tabs.find((x) => x.id === id)
  if (!tab) return false
  try {
    const res = await api.saveFileAs(
      textForSave(id),
      tab.title || tr('tab.untitled'),
      tab.encoding,
      tab.line_ending,
      tab.has_bom,
    )
    if (!res) return false
    const oldDraft = tab.path.startsWith('draft:') ? tab.path.slice(6) : null
    tab.path = res.path
    tab.title = baseName(res.path)
    tab.language = languageOf(res.path)
    commitBaseline(id)
    editor.markDirty(id, false)
    if (oldDraft) void api.removeDraft(oldDraft)
    void api.touchRecent(res.path)
    toast.ok(tr('toast.saved', { name: tab.title }))
    return true
  } catch (e) {
    toast.bad(tr('error.writeFailed', { reason: normalizeError(e).message }))
    return false
  }
}

/**
 * 关闭一个标签（带未保存确认）。
 *
 * 返回 false = 用户取消或保存失败，调用方（关闭全部 / 退出）应中止。
 */
export async function closeTabById(id: number): Promise<boolean> {
  const editor = useEditorStore()
  const tab = editor.tabs.find((x) => x.id === id)
  if (!tab) return true

  if (tab.dirty) {
    // 先激活：确认框里的文件名要与用户看到的那个标签一致。
    editor.activate(id)
    const choice = await askUnsaved(tab.title || tr('tab.untitled'))
    if (choice === 'cancel') return false
    if (choice === 'save' && !(await saveTabById(id))) return false
  }
  // 草稿标签关掉时清掉后端草稿，否则下次启动又冒出来。
  if (tab.path.startsWith('draft:')) void api.removeDraft(tab.path.slice(6))
  editor.close(id, true)
  return true
}
