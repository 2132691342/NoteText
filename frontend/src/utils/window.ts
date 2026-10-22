// 窗口级命令：全屏切换。
//
// 全屏走 Tauri 的窗口 API（WebView2 的原生全屏），不是浏览器 F11——
// 前者能让标题栏一起消失，后者只作用于内容区。
import { getCurrentWindow } from '@tauri-apps/api/window'
import { isDesktop } from '@/api'

/** 当前是否全屏。桌面外壳内查真实窗口，浏览器预览时查 CSS 全屏。 */
export async function isFullscreen(): Promise<boolean> {
  if (isDesktop) {
    try {
      return await getCurrentWindow().isFullscreen()
    } catch {
      return false
    }
  }
  return !!document.fullscreenElement
}

/**
 * 切换全屏。
 *
 * 返回切换后的状态：调用方据此更新工具栏按钮的选中态，
 * 而不是自己猜「刚才是不是全屏」——猜错一次按钮就和实际状态相反。
 */
export async function toggleFullscreen(): Promise<boolean> {
  if (isDesktop) {
    const win = getCurrentWindow()
    const next = !(await win.isFullscreen().catch(() => false))
    await win.setFullscreen(next).catch(() => undefined)
    return next
  }
  if (document.fullscreenElement) {
    await document.exitFullscreen().catch(() => undefined)
    return false
  }
  await document.documentElement.requestFullscreen().catch(() => undefined)
  return true
}
