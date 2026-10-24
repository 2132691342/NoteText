// 退出守卫：关窗时先检查未保存内容。
// 链路（specs/04）：前端点 X → 后端发 `app:before-close` → 逐个问未保存标签

import { onMounted, onUnmounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { askUnsaved } from '@/components/dialogs/UnsavedDialog.vue'
import { useEditorStore } from '@/stores/editor'
import { useSettingsStore } from '@/stores/settings'
import { useI18n } from 'vue-i18n'
import { useToastStore } from '@/stores/toast'
import { api, isDesktop } from '@/api'
import { commitBaseline, get, takeStashedDoc } from '@/composables/useEditorRegistry'

export function useQuitGuard() {
  const { t } = useI18n()
  const editor = useEditorStore()
  const settings = useSettingsStore()
  const toast = useToastStore()

  if (!isDesktop) return

  let unlisten: (() => void) | null = null
  /** 正在处理关窗：防止重复弹确认（托盘隐藏 + 点 X 会连发两次事件）。 */
  let busy = false

  onMounted(async () => {
    unlisten = await listen('app:before-close', async () => {
      if (busy) return
      busy = true
      try {
        await onBeforeClose()
      } finally {
        busy = false
      }
    })
  })

  onUnmounted(() => unlisten?.())

  /** 存会话 + 草稿：即使这次取消退出，下次启动的布局也是最新的。 */
  async function persistLayout() {
    try {
      await api.saveSession({
        session: {
          id: 1,
          active_index: Math.max(
            0,
            editor.tabs.findIndex((x) => x.id === editor.activeId),
          ),
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
      for (const tab of editor.tabs.filter((x) => x.path.startsWith('draft:'))) {
        await api.saveDraft({
          path: tab.path,
          title: tab.title,
          content: textOf(tab.id),
        })
      }
    } catch {
      /* 存不上也继续退出：拦着用户不让关更糟 */
    }
  }

  /**
   * 标签正文：视图在就用视图的；视图被路由卸了（设置页 / 帮助页退出）
   * 就退回卸载前的暂存。直接拿 getText 的空串写盘会把文件清空。
   */
  function textOf(tabId: number): string {
    const v = get(tabId)
    if (v) return v.state.doc.toString()
    return takeStashedDoc(tabId) ?? ''
  }

  async function onBeforeClose() {
    // 「关闭到托盘」开：不打扰未保存检查（内容还在内存里），存一份布局后收托盘。
    // 提示先给、窗口缓一拍再藏——否则用户只看到界面凭空消失。
    if (settings.config.ui.close_to_tray) {
      await persistLayout()
      toast.info(t('toast.minimizedToTray'))
      window.setTimeout(() => void api.hideToTray(), 1200)
      return
    }

    await persistLayout()

    // 逐个处理脏标签：保存 / 不保存 / 取消。
    for (const tab of [...editor.tabs]) {
      if (!tab.dirty) continue
      const name = tab.title || t('tab.untitled')
      const choice = await askUnsaved(name)
      if (choice === 'cancel') {
        await api.cancelQuit()
        return
      }
      if (choice === 'discard') continue
      // 保存：写盘失败就停在这里，不静默丢掉用户内容。
      if (!tab.path || tab.path.startsWith('draft:')) {
        const res = await api.saveFileAs(
          textOf(tab.id),
          name,
          tab.encoding,
          tab.line_ending,
          tab.has_bom,
        )
        if (!res) {
          await api.cancelQuit()
          return
        }
        tab.path = res.path
        tab.title = res.path.replace(/^.*[\\/]/, '')
        commitBaseline(tab.id)
        editor.markDirty(tab.id, false)
        continue
      }
      try {
        await api.writeFile(tab.path, textOf(tab.id), tab.encoding, tab.line_ending, tab.has_bom)
        commitBaseline(tab.id)
        editor.markDirty(tab.id, false)
      } catch {
        toast.bad(t('toast.saveOnExitFailed', { name: tab.title }))
        await api.cancelQuit()
        return
      }
    }

    // 再存一次布局：刚才的保存可能改了路径 / 标题。
    await persistLayout()
    await api.confirmQuit()
    await api.forceQuit()
  }
}
