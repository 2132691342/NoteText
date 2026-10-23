// 光标跳转历史：上一个位置 / 下一个位置。
// 为什么需要：用户在 3000 行的文件里跳到某个函数，改完想回到原来的地方，
import { ref } from 'vue'
import { get } from '@/composables/useEditorRegistry'

/** 单次位移小于这个值就不算跳转：方向键微调与打字都在此列。 */
const JUMP_THRESHOLD = 5

/** 每个标签一条栈：切标签不该把另一个文件的位置混进来。 */
interface Frame {
  from: number
  to: number
}
const back = ref(new Map<number, Frame[]>())
const fwd = ref(new Map<number, Frame[]>())

/** 取某个标签的回退栈。 */
function backStack(id: number): Frame[] {
  return back.value.get(id) ?? []
}

/** 取某个标签的前进栈。 */
function fwdStack(id: number): Frame[] {
  return fwd.value.get(id) ?? []
}

/** 每个标签的栈上限：再老的的位置用户也不会回去，留着只占内存。 */
const MAX_DEPTH = 100

/**
 * 记一次跳转。由编辑器在 selectionSet 且位移超阈值时调。
 *
 * `from` 是跳转前的位置，`to` 是跳转后的位置：回退要能回到 from。
 */
export function recordJump(tabId: number, from: number, to: number) {
  if (Math.abs(to - from) < JUMP_THRESHOLD) return
  const b = new Map(back.value)
  const stack = (b.get(tabId) ?? []).concat([{ from, to }])
  // 超深就丢最老的：用户回不去那么远，而栈深了每次回退都要扫。
  b.set(tabId, stack.length > MAX_DEPTH ? stack.slice(stack.length - MAX_DEPTH) : stack)
  back.value = b
  // 新跳转一发生，前进栈就作废：历史从这里拐弯了。
  const f = new Map(fwd.value)
  if (f.has(tabId)) {
    f.delete(tabId)
    fwd.value = f
  }
}

/** 回退到上一个位置。返回是否真的跳了。 */
export function goBack(tabId: number): boolean {
  const stack = backStack(tabId)
  if (stack.length === 0) return false
  const view = get(tabId)
  if (!view) return false
  const frame = stack[stack.length - 1]
  const b = new Map(back.value)
  b.set(tabId, stack.slice(0, -1))
  back.value = b
  // 当前去处进前进栈：这样「下一个位置」能回到刚离开的地方。
  const f = new Map(fwd.value)
  f.set(tabId, (f.get(tabId) ?? []).concat([{ from: frame.from, to: view.state.selection.main.head }]))
  fwd.value = f
  view.dispatch({ selection: { anchor: frame.from }, scrollIntoView: true })
  view.focus()
  return true
}

/** 前进到下一个位置。返回是否真的跳了。 */
export function goForward(tabId: number): boolean {
  const stack = fwdStack(tabId)
  if (stack.length === 0) return false
  const view = get(tabId)
  if (!view) return false
  const frame = stack[stack.length - 1]
  const f = new Map(fwd.value)
  f.set(tabId, stack.slice(0, -1))
  fwd.value = f
  const b = new Map(back.value)
  b.set(tabId, (b.get(tabId) ?? []).concat([{ from: frame.from, to: frame.to }]))
  back.value = b
  view.dispatch({ selection: { anchor: frame.to }, scrollIntoView: true })
  view.focus()
  return true
}

/** 清空某个标签的历史（标签关闭时调，避免 Map 无限增长）。 */
export function clearNavHistory(tabId: number) {
  const b = new Map(back.value)
  const f = new Map(fwd.value)
  if (b.delete(tabId)) back.value = b
  if (f.delete(tabId)) fwd.value = f
}

/** 供测试与调试：当前栈深。 */
export function navDepths(tabId: number): { back: number; fwd: number } {
  return { back: backStack(tabId).length, fwd: fwdStack(tabId).length }
}
