// 崩溃恢复快照：每 30 秒把脏标签的正文静默快照进草稿库。
// 异常终止后凭「last_exit_clean=false + 草稿快照」提示恢复。只快照不写盘：自动落盘是 auto_save 的职责。
import { onUnmounted } from 'vue'
import { useEditorStore } from '@/stores/editor'
import { api, isDesktop } from '@/api'
import { getText, getTableProvider } from '@/composables/useEditorRegistry'

/** 快照间隔（毫秒）。 */
const SNAPSHOT_INTERVAL = 30_000
/** 单份快照的正文上限：草稿库只收 2MB（repo/draft.rs 同值），超限静默跳过。 */
const MAX_SNAPSHOT_BYTES = 2 * 1024 * 1024

export function useCrashGuard() {
  const editor = useEditorStore()

  if (!isDesktop) return

  /** 上次快照内容：没变化就不重写草稿表，30 秒一次的空转全部免掉。 */
  const lastSnapshot = new Map<number, string>()

  async function tick() {
    for (const tab of editor.tabs) {
      // 分块 / 截断标签没有完整正文，快照一部分会误导恢复。
      if (!tab.dirty || tab.chunked || tab.truncated) continue
      // 大表格的正文在磁盘上，不在内存里：快照拿不到东西，跳过。
      // 它的未保存改动在覆盖层里，崩溃后靠「重新打开 + 重做」找回。
      if (getTableProvider(tab.id)) continue
      const text = getText(tab.id)
      if (text.length === 0 || text.length > MAX_SNAPSHOT_BYTES) continue
      if (lastSnapshot.get(tab.id) === text) continue
      lastSnapshot.set(tab.id, text)
      try {
        await api.saveDraft({ path: tab.path, title: tab.title, content: text })
      } catch {
        // 快照存不上不影响使用：下次tick再试。
      }
    }
    // 关掉的标签清掉记录：map 不该无限增长。
    if (lastSnapshot.size > editor.tabs.length * 2) {
      const alive = new Set(editor.tabs.map((t) => t.id))
      for (const id of lastSnapshot.keys()) {
        if (!alive.has(id)) lastSnapshot.delete(id)
      }
    }
  }

  const timer = window.setInterval(() => void tick(), SNAPSHOT_INTERVAL)
  onUnmounted(() => window.clearInterval(timer))
}
