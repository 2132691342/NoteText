// 文本输入确认：重命名 / 另存为这类「要用户敲一个名字」的场景。
// 与 UnsavedDialog 同一套机制：状态在模块里，命令层只 import 函数，

import { ref } from 'vue'

export interface PromptOptions {
  title: string
  /** 输入框上方的说明，可省。 */
  label?: string
  /** 预填值：重命名时把旧名字带上，用户改几个字就行。 */
  value?: string
  /** 占位提示。 */
  placeholder?: string
  okText?: string
  cancelText?: string
}

interface Pending {
  opts: PromptOptions
  resolve: (v: string | null) => void
}

/** 同一时刻只显示一个输入框，后来的排队。 */
export const current = ref<Pending | null>(null)
const queue: Pending[] = []

/** 弹一个输入框，等用户确认或取消。取消返回 null。 */
export function askText(opts: PromptOptions): Promise<string | null> {
  return new Promise((resolve) => {
    const item: Pending = { opts, resolve }
    if (current.value) queue.push(item)
    else current.value = item
  })
}

/** 当前是否有待处理的输入框：退出流程用它避免重复弹。 */
export function hasTextPrompt(): boolean {
  return current.value !== null
}

/**
 * 二选一确认：是 / 否。
 *
 * 走系统消息框而不是自绘浮窗：确认要的是「一个决定」，自绘浮窗会多一次
 * 渲染与焦点管理，而这里已经有一个跨平台的现成实现（platform 层）。
 * 返回 true = 用户选「是」。
 */
export async function confirmText(opts: {
  title: string
  message: string
}): Promise<boolean> {
  const { api } = await import('@/api')
  return api.showConfirm(opts.title, opts.message)
}

/** 组件卸载时把挂起的请求全部作废：不 resolve 会让命令层的 await 永远挂着。 */
export function cancelAllTextPrompts() {
  const all = current.value ? [current.value, ...queue] : [...queue]
  current.value = null
  queue.length = 0
  for (const p of all) p.resolve(null)
}

/**
 * 结算当前输入框并推进队列。
 *
 * 先摘当前项再 resolve：resolve 里可能又推一个新请求进来，
 * 顺序反了会把新项当成旧项的继续者处理掉。
 */
export function settleTextPrompt(value: string | null) {
  const cur = current.value
  if (!cur) return
  current.value = null
  cur.resolve(value)
  const next = queue.shift()
  if (next) current.value = next
}
