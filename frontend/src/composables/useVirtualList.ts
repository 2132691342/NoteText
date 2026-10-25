// 定高虚拟列表：只把滚动窗口内的行交给渲染。几万条目的全量 v-for 会打爆
// WebView2 的 DOM——大压缩包浮窗打开即冻结。用法见两个归档浮窗。

import { computed, onBeforeUnmount, onMounted, ref, watch, type Ref } from 'vue'

/** 视口上下各多渲染几行，快速滚动不露白。 */
const OVERSCAN = 10

export function useVirtualList<T>(items: Ref<readonly T[]>, rowHeight: number) {
  const host = ref<HTMLElement | null>(null)
  const scrollTop = ref(0)
  const viewport = ref(0)

  const totalHeight = computed(() => items.value.length * rowHeight)
  const startIndex = computed(() =>
    Math.max(0, Math.floor(scrollTop.value / rowHeight) - OVERSCAN),
  )
  const endIndex = computed(() =>
    Math.min(
      items.value.length,
      Math.ceil((scrollTop.value + viewport.value) / rowHeight) + OVERSCAN,
    ),
  )
  const windowItems = computed(() => items.value.slice(startIndex.value, endIndex.value))
  /** 窗口首行的纵偏移：transform 平移比逐行定位便宜。 */
  const offset = computed(() => startIndex.value * rowHeight)

  function onScroll() {
    scrollTop.value = host.value?.scrollTop ?? 0
  }

  /** 数据源整体更换（换包 / 改过滤词）回到顶部，别停在旧位置对着新列表。 */
  watch(items, () => {
    if (host.value) host.value.scrollTop = 0
    scrollTop.value = 0
  })

  let observer: ResizeObserver | null = null
  onMounted(() => {
    viewport.value = host.value?.clientHeight ?? 0
    observer = new ResizeObserver(() => {
      viewport.value = host.value?.clientHeight ?? 0
    })
    if (host.value) observer.observe(host.value)
  })
  onBeforeUnmount(() => observer?.disconnect())

  return { host, totalHeight, windowItems, offset, onScroll, rowHeight }
}
