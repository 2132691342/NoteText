// 自动保存：按配置的间隔把已命名的脏标签写盘。
//
// 只自动存「已有路径」的标签：未命名的标签没有落盘位置，自动存会弹出
// 另存为对话框——用户正在打字时弹原生框是最糟的打断方式。这类标签
// 靠草稿机制保命（见 useQuitGuard 的 saveDraft）。
import { onUnmounted, watch } from 'vue'
import { useEditorStore } from '@/stores/editor'
import { useSettingsStore } from '@/stores/settings'
import { saveTabById } from '@/composables/useCloseTab'

export function useAutoSave() {
  const editor = useEditorStore()
  const settings = useSettingsStore()

  let timer: number | null = null
  /** 正在保存的标签：避免上一轮还没写完又叠一轮。 */
  const busy = new Set<number>()

  function stop() {
    if (timer !== null) {
      window.clearInterval(timer)
      timer = null
    }
  }

  function start() {
    stop()
    const cfg = settings.config.editor
    if (!cfg.auto_save) return
    // 下限 5 秒：再短就会在用户连续输入时反复写盘，磁盘与光标都要抖。
    const interval = Math.max(5, cfg.auto_save_interval) * 1000
    timer = window.setInterval(() => void tick(), interval)
  }

  async function tick() {
    for (const tab of editor.tabs) {
      if (!tab.dirty) continue
      // 未命名 / 草稿 / 截断：不自动写盘（理由见文件头）。
      if (!tab.path || tab.path.startsWith('draft:') || tab.truncated) continue
      if (busy.has(tab.id)) continue
      busy.add(tab.id)
      try {
        await saveTabById(tab.id)
      } finally {
        busy.delete(tab.id)
      }
    }
  }

  watch(
    () => [settings.config.editor.auto_save, settings.config.editor.auto_save_interval],
    () => start(),
    { immediate: true },
  )

  onUnmounted(stop)
}
