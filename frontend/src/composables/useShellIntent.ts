// 壳意图：托盘「新建文档」、外部打开文件（双击关联 / 二次启动 / 冷启动参数）的落地。
// 必须挂在 App 根上而不是编辑器页：用户可能正停在设置页，事件来时编辑器已卸载。
import { onMounted, onUnmounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { useEditorStore } from '@/stores/editor'
import { useDialogStore } from '@/stores/dialogs'
import { useI18n } from 'vue-i18n'
import { useToastStore } from '@/stores/toast'
import { api, isDesktop } from '@/api'
import { useRouter } from 'vue-router'
import { baseName, isArchivePath } from '@/utils'

export function useShellIntent() {
  const { t } = useI18n()
  const editor = useEditorStore()
  const toast = useToastStore()
  const router = useRouter()
  const dialogs = useDialogStore()

  if (!isDesktop) return

  let unlistenOpen: (() => void) | null = null
  let unlistenNew: (() => void) | null = null

  onMounted(async () => {
    unlistenOpen = await listen<string>('app:open-file', (ev) => {
      void openFile(ev.payload)
    })
    unlistenNew = await listen('app:new-document', async () => {
      // 不在编辑器页就先切回去：新文档要在编辑器里看见。
      if (router.currentRoute.value.name !== 'editor') {
        await router.push('/')
      }
      editor.newTab()
    })

    // 冷启动意图（双击关联文件启动）：命令行参数在后端定格，
    // 这里拉取一次——比事件可靠，不怕挂载时序。
    try {
      const intent = await api.takeLaunchIntent()
      if (intent) await openFile(intent)
    } catch {
      // 拉不到不影响启动：宁可不开文件，不让启动卡住
    }
  })

  onUnmounted(() => {
    unlistenOpen?.()
    unlistenNew?.()
  })

  async function openFile(path: string) {
    // 压缩包走浏览浮窗而不是文本编辑：双击 .zip 关联打开的期望是目录树。
    if (isArchivePath(path)) {
      if (router.currentRoute.value.name !== 'editor') {
        await router.push('/')
      }
      dialogs.open('archiveBrowser', '', { path })
      void api.touchRecent(path)
      return
    }
    // 普通文件：不在编辑器页就先切回去，用户该看到文件。
    if (router.currentRoute.value.name !== 'editor') {
      await router.push('/')
    }
    const tab = await editor.openPath(path)
    if (!tab) {
      toast.bad(t('toast.openFailed', { name: baseName(path) }))
      return
    }
    void api.touchRecent(path)
  }
}
