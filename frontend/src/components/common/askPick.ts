// 列表选择浮窗：从一组选项里挑一个。
// 与 AskTextDialog 同一套机制（状态在模块里，命令层只 import 函数）。

import { ref } from 'vue'

export interface PickItem {
  value: string
  label: string
  /** 副标题：扩展名、备注等。 */
  hint?: string
}

export interface PickOptions {
  title: string
  items: PickItem[]
  /** 当前选中项：列表打开时把它滚到可见处并高亮。 */
  current?: string
  /** 顶部搜索框的占位提示。 */
  placeholder?: string
}

interface Pending {
  opts: PickOptions
  resolve: (v: string | null) => void
}

/** 同一时刻只显示一个选择器，后来的排队。 */
export const current = ref<Pending | null>(null)
const queue: Pending[] = []

/** 弹一个选择器，等用户选或取消。取消返回 null。 */
export function askPick(opts: PickOptions): Promise<string | null> {
  return new Promise((resolve) => {
    const item: Pending = { opts, resolve }
    if (current.value) queue.push(item)
    else current.value = item
  })
}

/** 当前是否有待处理的选择器。 */
export function hasPick(): boolean {
  return current.value !== null
}

/** 结算并推进队列。先摘当前项再 resolve：resolve 里可能又推新请求进来。 */
export function settlePick(value: string | null) {
  const cur = current.value
  if (!cur) return
  current.value = null
  cur.resolve(value)
  const next = queue.shift()
  if (next) current.value = next
}

/** 组件卸载时把挂起的请求全部作废。 */
export function cancelAllPicks() {
  const all = current.value ? [current.value, ...queue] : [...queue]
  current.value = null
  queue.length = 0
  for (const p of all) p.resolve(null)
}
