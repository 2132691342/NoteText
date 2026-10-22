// 退出守卫：关窗时先检查未保存内容。
//
// 链路（specs/04）：前端点 X → 后端发 `app:before-close` → 弹确认 →
// 用户确认 → 调 `confirm_quit` + `force_quit` → 后端放行 CloseRequested。

import { onMounted, onUnmounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { useEditorStore } from '@/stores/editor'
import { useSettingsStore } from '@/stores/settings'
import { useI18n } from 'vue-i18n'
import { useToastStore } from '@/stores/toast'
import { api, isDesktop } from '@/api'
import { getText } from '@/composables/useEditorRegistry'

export function useQuitGuard() {
  const { t } = useI18n()
  const editor = useEditorStore()
  const settings = useSettingsStore()
  const toast = useToastStore()

  if (!isDesktop) return

  let unlisten: (() => void) | null = null

  onMounted(async () => {
    unlisten = await listen('app:before-close', async () => {
      await onBeforeClose()
    })
  })

  onUnmounted(() => unlisten?.())

  async function onBeforeClose() {
    // 关窗前先把会话存了：即使这次退出，下次启动的布局也是最新的。
    try {
      await api.saveSession({
        session: {
          id: 1,
          active_index: Math.max(0, editor.tabs.findIndex((t) => t.id === editor.activeId)),
          sidebar_open: settings.config.ui.show_sidebar,
          sidebar_tab: settings.config.ui.sidebar_tab,
          window_width: window.innerWidth,
          window_height: window.innerHeight,
          window_x: Math.round(window.screenX),
          window_y: Math.round(window.screenY),
          window_maximized: false,
          split_ratio: editor.splitRatio,
          updated_at: Date.now(),
        },
        tabs: editor.toDOs(),
      })
      // 草稿正文另存一份：新建未保存标签的内容不该丢。
      for (const tab of editor.tabs.filter((t) => t.path.startsWith('draft:'))) {
        await api.saveDraft({
          path: tab.path,
          title: tab.title,
          content: getText(tab.id),
        })
      }
    } catch {
      /* 存不上也继续退出：拦着用户不让关更糟 */
    }

    if (editor.dirtyCount === 0) {
      await api.confirmQuit()
      await api.forceQuit()
      return
    }

    const ok = await api.showConfirm(
      t('app.quitConfirmTitle'),
      t('app.quitConfirmBody', { count: editor.dirtyCount }),
    )
    if (!ok) {
      // 用户取消：什么都不做，窗口保持打开。
      await api.cancelQuit()
      return
    }

    // 全部保存：逐个写盘，失败的留着自己处理（不阻断退出）。
    for (const tab of editor.tabs.filter((x) => x.dirty && x.path && !x.path.startsWith('draft:'))) {
      try {
        await api.writeFile(tab.path, getText(tab.id), tab.encoding, tab.line_ending, tab.has_bom)
        editor.markDirty(tab.id, false)
      } catch (e) {
        toast.bad(t('toast.saveOnExitFailed', { name: tab.title }))
      }
    }
    await api.confirmQuit()
    await api.forceQuit()
  }
}
