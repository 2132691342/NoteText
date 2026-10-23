// 壳意图：托盘「新建文档」与「外部打开文件」的落地。
// 为什么需要：用户当时可能在设置页，事件早于视图挂载就丢了。
import { onMounted, onUnmounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { useEditorStore } from '@/stores/editor'
import { useI18n } from 'vue-i18n'
import { useToastStore } from '@/stores/toast'
import { api, isDesktop } from '@/api'
import { useRouter } from 'vue-router'
import { baseName } from '@/utils'

/** 待处理的意图：视图没挂载时先存这里。 */
let pendingOpen: string | null = null
let pendingNew = false

export function useShellIntent() {
  const { t } = useI18n()
  const editor = useEditorStore()
  const toast = useToastStore()
  const router = useRouter()

  if (!isDesktop) return

  let unlisten: (() => void) | null = null

  onMounted(async () => {
    unlisten = await listen<string>('app:open-file', async (ev) => {
      // 不在编辑器页就先切回去：用户在设置页时打开文件，该看到文件。
      if (router.currentRoute.value.name !== 'editor') {
        await router.push('/')
      }
      await openFile(ev.payload)
    })

    // 消费启动前收到的意图（事件早于本组件挂载）。
    if (pendingOpen) {
      const p = pendingOpen
      pendingOpen = null
      await openFile(p)
    }
    if (pendingNew) {
      pendingNew = false
      editor.newTab()
    }
  })

  onUnmounted(() => unlisten?.())

  async function openFile(path: string) {
    const tab = await editor.openPath(path)
    if (!tab) {
      toast.bad(t('toast.openFailed', { name: baseName(path) }))
      return
    }
    void api.touchRecent(path)
  }
}

/** 模块级兜底：组件还没挂载时壳层就发来了意图。 */
export function queueOpen(path: string) {
  pendingOpen = path
}

export function queueNew() {
  pendingNew = true
}
