// 剪贴板历史：收集编辑器里的复制 / 剪切内容，可回插。
//
// 只读编辑器事件、不碰系统剪贴板：Webview 里读系统剪贴板要权限，
// 而用户真正需要的是「刚才复制过的那几段」，我们自己攒就够了。
import { ref } from 'vue'

export interface ClipEntry {
  text: string
  /** 毫秒时间戳：列表按最近在上。 */
  at: number
}

/** 上限：再多列表就翻不动了，用户也记不住。 */
const MAX = 50

const entries = ref<ClipEntry[]>([])

/** 记一条。空串与重复内容不入表。 */
export function pushClip(text: string) {
  if (!text || !text.trim()) return
  const at = Date.now()
  const hit = entries.value.findIndex((e) => e.text === text)
  if (hit >= 0) entries.value.splice(hit, 1)
  entries.value.unshift({ text, at })
  if (entries.value.length > MAX) entries.value.length = MAX
}

export function useClipboardHistory() {
  return { entries }
}

/** 清空。 */
export function clearClip() {
  entries.value = []
}
