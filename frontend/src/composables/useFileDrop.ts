// 文件拖放：把外部文件拖进窗口就打开。
//
// Tauri 的 `dragDropEnabled: false`（见 tauri.conf.json）关掉了 WebView 自己的
// 拖放导航——开着的话拖进文件会让整个界面跳到那个文件。关掉之后路径只能从
// Tauri 的 `onDragDropEvent` 拿：WebView 的 DragEvent 里 `File.path` 已废弃。
import { onMounted, onUnmounted } from 'vue'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { useEditorStore } from '@/stores/editor'
import { useFileTreeStore } from '@/stores/fileTree'
import { useToastStore } from '@/stores/toast'
import { i18n } from '@/i18n'
import { api, isDesktop, normalizeError } from '@/api'
import { baseName } from '@/utils'

export function useFileDrop() {
  const editor = useEditorStore()
  const tree = useFileTreeStore()
  const toast = useToastStore()
  const t = (k: string, n: Record<string, unknown> = {}) => i18n.global.t(k, n)

  if (!isDesktop) return

  let unlisten: (() => void) | null = null
  // 拖放进入 / 离开会连续触发多次：用计数避免提示层闪烁。
  let depth = 0

  function signal(active: boolean) {
    document.dispatchEvent(new CustomEvent('nt-drag-state', { detail: { active } }))
  }

  onMounted(async () => {
    unlisten = await getCurrentWebview().onDragDropEvent((ev) => {
      const p = ev.payload
      if (p.type === 'enter' || p.type === 'over') {
        depth += 1
        signal(true)
        return
      }
      if (p.type === 'leave') {
        depth = 0
        signal(false)
        return
      }
      if (p.type !== 'drop') return
      depth = 0
      signal(false)
      void handleDrop(p.paths ?? [])
    })
  })

  onUnmounted(() => unlisten?.())

  /** 打开拖进来的东西：文件开标签，目录设为工作区。 */
  async function handleDrop(paths: string[]) {
    if (paths.length === 0) return
    for (const path of paths) {
      try {
        // 目录 → 当工作区打开（拖文件夹的诉求通常是「在这个目录里干活」）。
        // 判据用 readDir 能不能成功：stat 对目录也成功，没法靠它区分。
        const entries = await api.readDir(path).catch(() => null)
        if (entries) {
          await tree.open(path)
          toast.ok(t('toast.folderOpened', { name: baseName(path) }))
          continue
        }
        const tab = await editor.openPath(path)
        if (tab) {
          void api.touchRecent(path)
          toast.ok(t('toast.opened', { name: baseName(path) }))
        }
      } catch (e) {
        toast.bad(t('toast.openFailed', { reason: normalizeError(e).message }))
      }
    }
  }
}
