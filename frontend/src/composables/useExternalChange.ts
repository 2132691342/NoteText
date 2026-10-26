// 外部变更：文件被别的程序改了怎么办。
// 三种情况分开处理（这是编辑器最容易被骂的地方）：标签干净 → 静默重载；
import { onMounted, onUnmounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { useEditorStore } from '@/stores/editor'
import { useFileTreeStore } from '@/stores/fileTree'
import { useI18n } from 'vue-i18n'
import { useToastStore } from '@/stores/toast'
import { api, isDesktop, normalizeError } from '@/api'
import { doReplaceDoc } from '@/composables/useEditorActions'
import { getText } from '@/composables/useEditorRegistry'
import { baseName } from '@/utils'

export function useExternalChange() {
  const { t } = useI18n()
  const editor = useEditorStore()
  const tree = useFileTreeStore()
  const toast = useToastStore()

  if (!isDesktop) return

  let unlisten: (() => void) | null = null

  /**
   * 每个路径的处理去抖 + 加锁。
   *
   * 一次保存会从文件系统送来 3~5 个事件（临时文件创建、内容写、rename、
   * 属性变更）。逐个处理的结果是同一个文件被读 4 次、弹 4 条提示——
   * 用户会以为出了什么大事。600ms 窗口内的同一路径只处理一次。
   */
  const pendingTimers = new Map<string, number>()
  const inFlight = new Set<string>()

  function schedule(path: string) {
    const old = pendingTimers.get(path)
    if (old !== undefined) window.clearTimeout(old)
    const timer = window.setTimeout(() => {
      pendingTimers.delete(path)
      if (inFlight.has(path)) return
      inFlight.add(path)
      void handlePath(path).finally(() => inFlight.delete(path))
    }, 600)
    pendingTimers.set(path, timer)
  }

  onMounted(async () => {
    unlisten = await listen<string[]>('fs:changed', (ev) => {
      for (const path of ev.payload) schedule(path)
      // 文件树刷新：外部新建 / 删除文件要反映出来。
      if (tree.root) void tree.refresh(tree.root)
    })
  })

  onUnmounted(() => {
    unlisten?.()
    for (const timer of pendingTimers.values()) window.clearTimeout(timer)
    pendingTimers.clear()
  })

  async function handlePath(path: string) {
    const tab = editor.findByPath(path)
    if (!tab) return
    if (tab.dirty) {
      // 脏标签不覆盖：用户手里有未保存的修改，替用户做决定就是丢内容。
      toast.warn(t('toast.fileChangedDirty', { name: baseName(path) }))
      return
    }
    // 干净标签：必须真的把新正文推进编辑器，只改元数据会让编辑区停在旧内容。
    // 那时提示说文件变了、编辑区还是旧的，用户以为提示在撒谎。
    try {
      // 截断标签只有前段内容：全量读会被后端拒绝，外部变更就永远同步不过来
      const content = await api.readFile(path, undefined, tab.truncated ? { mode: 'head' } : undefined)
      const changed = getText(tab.id) !== content.text
      editor.applyContent(tab, content)
      if (changed) {
        // 只有正文真的不一样才重绘与提示：文件系统常送「属性变更」这类
        // 无关事件，每次都重绘会让光标位置丢失。
        doReplaceDoc(tab.id, content.text)
        toast.info(t('toast.fileReloaded', { name: baseName(path) }))
      }
    } catch (e) {
      toast.bad(t('toast.openFailed', { reason: normalizeError(e).message }))
    }
  }
}
