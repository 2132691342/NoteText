// 轻提示 store：顶部 / 右下角的短消息。
//
// 与 ElMessage 的分工：ElMessage 是 EP 的现成件，但它的队列在多个
// 浮窗同时报错时会互相顶掉。这里自绘一个「可选中复制、悬停不计时」的宿主，
// 因为「复制错误码给开发者」是排障的高频动作。
import { defineStore } from 'pinia'
import { ref } from 'vue'

export type ToastKind = 'info' | 'ok' | 'warn' | 'bad'

export interface Toast {
  id: number
  kind: ToastKind
  text: string
  /** 悬停时暂停倒计时：长错误信息要能读完。 */
  duration: number
}

let seq = 0

export const useToastStore = defineStore('toast', () => {
  const items = ref<Toast[]>([])
  /** 悬停中的 toast id：计时器到期时若仍悬停则跳过本次关闭。 */
  const hovered = ref<number | null>(null)

  function push(kind: ToastKind, text: string, duration = 3500) {
    const id = ++seq
    items.value.push({ id, kind, text, duration })
    // 同屏最多 4 条：再多就挤掉正文，用户反而看不到最新的一条。
    if (items.value.length > 4) items.value.shift()
    scheduleClose(id, duration)
    return id
  }

  function scheduleClose(id: number, duration: number) {
    window.setTimeout(() => {
      if (hovered.value === id) {
        // 悬停中：再给一轮，用户移开鼠标后自然收起。
        const t = items.value.find((x) => x.id === id)
        if (t) scheduleClose(id, t.duration)
        return
      }
      dismiss(id)
    }, duration)
  }

  function dismiss(id: number) {
    const i = items.value.findIndex((x) => x.id === id)
    if (i >= 0) items.value.splice(i, 1)
  }

  return {
    items,
    hovered,
    info: (text: string, duration?: number) => push('info', text, duration),
    ok: (text: string, duration?: number) => push('ok', text, duration),
    warn: (text: string, duration?: number) => push('warn', text, duration),
    bad: (text: string, duration?: number) => push('bad', text, duration),
    dismiss,
  }
})
