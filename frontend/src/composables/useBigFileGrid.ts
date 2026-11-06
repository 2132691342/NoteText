// 大文件只读网格：分页取行 + 虚拟滚动（复用大表格的坐标系与页缓存骨架）。
// 数据源是 big_file_lines：跨块巨行后端已拼好，超宽行后端已截断打标。
// 索引的建立与进度 UI 留在组件层（openWithProgress），这里只管「拿到 meta 之后」。
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch, type Ref } from 'vue'
import { api, normalizeError } from '@/api'
import {
  computeGridWindow,
  FALLBACK_VIEWPORT,
  MAX_SCROLL_PX,
  ROW_H,
} from '@/composables/gridScroll'
import type { BigFileLineVO, BigFileMetaVO } from '@/types/api'

export { ROW_H }

/** 一页多少行：一次 IPC 取一屏的十几倍。 */
const PAGE_SIZE = 500
/** 预取页数：滚到某页之前先把它前后几页取来。 */
const PREFETCH_PAGES = 3
/** 页缓存上限：滚动很远时丢最久没用过的页（LRU）。 */
const MAX_CACHED_PAGES = 80
/** 单行显示的字符上限：与后端 `big_file_lines` 的 clip 缺省一致。 */
export const CLIP_CHARS = 1000

export interface BigFileGridOptions {
  path: Ref<string>
  /** 出错回调（stale / 索引丢失等）。 */
  onError?: (message: string) => void
}

export function useBigFileGrid(opts: BigFileGridOptions) {
  const meta = ref<BigFileMetaVO | null>(null)
  const error = ref('')

  /** 页缓存：页号 → 行数组（行号连续，页内第一行的全局行号 = 页号 × PAGE_SIZE + 1）。 */
  const pages = shallowRef(new Map<number, BigFileLineVO[]>())
  /** 取页顺序：超出上限时先丢最早取的。 */
  const pageOrder: number[] = []
  /** 数据版本号：页缓存变化时 +1。 */
  const version = ref(0)
  /** 正在请求的页。 */
  const pending = new Set<number>()

  const totalLines = computed(() => {
    const m = meta.value
    if (!m) return 0
    // 空文件没有块：按 0 行画，给占位提示而不是一根永远取不到数据的行。
    return m.block_count === 0 ? 0 : m.total_lines
  })
  /** 文件是否一行都没有（空文件）：网格画不出东西，给个占位提示。 */
  const isEmpty = computed(() => meta.value != null && totalLines.value === 0)

  // ---- 滚动坐标（与 useTableGrid 同构；无表头偏移） ----

  const scrollHost = ref<HTMLElement | null>(null)
  /** 滚动条空间的滚动位置（压缩后的像素）。 */
  const scrollTop = ref(0)
  /** 视口高度（像素）。 */
  const viewport = ref(0)

  const logicalHeight = computed(() => totalLines.value * ROW_H)
  const maxLogical = computed(() => Math.max(0, logicalHeight.value - viewport.value))
  const scrollHeight = computed(() => Math.min(logicalHeight.value, MAX_SCROLL_PX))
  const grid = computed(() =>
    computeGridWindow(totalLines.value, viewport.value, scrollTop.value, 0),
  )
  const scale = computed(() => grid.value.scale)
  const logicalScroll = computed(() => grid.value.logicalScroll)
  const firstVisible = computed(() => grid.value.firstVisible)
  const windowStart = computed(() => grid.value.windowStart)
  const windowEnd = computed(() => grid.value.windowEnd)
  const windowTop = computed(() => grid.value.windowTop)
  const effectiveViewport = computed(() =>
    viewport.value > 0 ? viewport.value : FALLBACK_VIEWPORT,
  )

  /** 渲染窗口里的每一行。`cached` 为 false 表示页还没到：画占位行。 */
  const windowRows = computed(() => {
    const out: Array<{
      index: number
      lineNo: number
      text: string
      clipped: boolean
      cached: boolean
    }> = []
    for (let d = windowStart.value; d < windowEnd.value; d += 1) {
      const pageIdx = Math.floor(d / PAGE_SIZE)
      const row = pages.value.get(pageIdx)?.[d - pageIdx * PAGE_SIZE]
      out.push({
        index: d,
        lineNo: d + 1,
        text: row?.text ?? '',
        clipped: row?.clipped ?? false,
        cached: row != null,
      })
    }
    return out
  })

  // ---- 取数 ----

  /** 取一页（已在缓存里就跳过，命中时顺手把它挪到 LRU 队尾）。 */
  async function ensurePage(pageIdx: number) {
    if (!meta.value || pending.has(pageIdx)) return
    if (pages.value.has(pageIdx)) {
      const at = pageOrder.indexOf(pageIdx)
      if (at >= 0 && at !== pageOrder.length - 1) {
        pageOrder.splice(at, 1)
        pageOrder.push(pageIdx)
      }
      return
    }
    const startLine = pageIdx * PAGE_SIZE + 1
    if (pageIdx < 0 || startLine > totalLines.value) return
    pending.add(pageIdx)
    try {
      const res = await api.bigFile.lines({
        path: opts.path.value,
        start_line: startLine,
        line_count: PAGE_SIZE,
        clip_chars: CLIP_CHARS,
      })
      const next = new Map(pages.value)
      next.set(pageIdx, res.lines ?? [])
      pageOrder.push(pageIdx)
      while (pageOrder.length > MAX_CACHED_PAGES) {
        const old = pageOrder.shift()
        if (old == null || old === pageIdx) continue
        next.delete(old)
      }
      pages.value = next
      version.value += 1
    } catch (e) {
      opts.onError?.(normalizeError(e).message)
    } finally {
      pending.delete(pageIdx)
    }
  }

  /** 保证渲染窗口覆盖的页都在缓存里（含前后预取）。 */
  function ensureVisiblePages() {
    if (!meta.value) return
    const first = windowStart.value
    const last = Math.max(first, windowEnd.value - 1)
    const firstPage = Math.floor(first / PAGE_SIZE)
    const lastPage = Math.floor(last / PAGE_SIZE)
    for (let p = firstPage; p <= lastPage; p += 1) void ensurePage(p)
    for (let p = firstPage - PREFETCH_PAGES; p < firstPage; p += 1) {
      if (p >= 0) void ensurePage(p)
    }
    for (let p = lastPage + 1; p <= lastPage + PREFETCH_PAGES; p += 1) {
      void ensurePage(p)
    }
  }

  // ---- 滚动 ----

  // rAF 合帧：滚轮与滚动条事件一帧之内只应用最新状态。
  let scrollRaf = 0
  let wheelRaf = 0
  let wheelAcc = 0

  /** 滚轮：步进跟随 deltaY 幅度——标准滚轮一格 ±120 就是 3 行，
   *  触控板的连续小增量按比例缩，快滑快、慢滑慢。 */
  function onWheel(e: WheelEvent) {
    if (totalLines.value <= 0) return
    e.preventDefault()
    const dir = e.deltaY > 0 ? 1 : e.deltaY < 0 ? -1 : 0
    if (dir === 0) return
    const mag = Math.min(Math.abs(e.deltaY), 120)
    wheelAcc += (dir * (mag / 120)) * 3 * ROW_H
    if (!wheelRaf) {
      wheelRaf = requestAnimationFrame(() => {
        wheelRaf = 0
        const d = wheelAcc
        wheelAcc = 0
        if (d !== 0) scrollToLogical(logicalScroll.value + d)
      })
    }
  }

  /** 设到某个逻辑位置（并同步滚动条）。 */
  function scrollToLogical(target: number) {
    const clamped = Math.max(0, Math.min(target, maxLogical.value))
    const px = clamped * scale.value
    scrollTop.value = px
    const host = scrollHost.value
    if (host && Math.abs(host.scrollTop - px) > 0.5) {
      host.scrollTop = px
    }
  }

  /** 滚到某个全局行号（1-based），尽量放视口中部。 */
  function scrollToLine(line: number) {
    const target = Math.max(0, Math.min(line - 1, totalLines.value - 1))
    scrollToLogical(target * ROW_H - effectiveViewport.value / 2 + ROW_H / 2)
  }

  /** 滚动条被拖动：按比例换算回逻辑位置（rAF 合帧）。 */
  function onScroll() {
    if (scrollRaf) return
    scrollRaf = requestAnimationFrame(() => {
      scrollRaf = 0
      const host = scrollHost.value
      if (!host || !meta.value) return
      scrollTop.value = host.scrollTop
    })
  }

  /** 视口顶部当前的全局行号（1-based）：状态栏与「编辑本块」锚点用它。 */
  function currentLine(): number {
    return firstVisible.value + 1
  }

  onBeforeUnmount(() => {
    if (scrollRaf) cancelAnimationFrame(scrollRaf)
    if (wheelRaf) cancelAnimationFrame(wheelRaf)
  })

  watch([windowStart, windowEnd, version], () => {
    ensureVisiblePages()
  })

  // ---- 元数据进出 ----

  /** 索引建好后交给网格：重置页缓存，回到指定行（会话恢复的位置）。 */
  function setMeta(m: BigFileMetaVO, atLine = 1) {
    meta.value = m
    pages.value = new Map()
    pageOrder.length = 0
    version.value += 1
    if (atLine > 1) scrollToLine(atLine)
    else {
      scrollTop.value = 0
      if (scrollHost.value) scrollHost.value.scrollTop = 0
    }
    ensureVisiblePages()
  }

  /** 块写回之后索引被重建：总行数可能变了，页缓存全部作废。 */
  function resetPages(m: BigFileMetaVO | null, atLine: number) {
    if (m) meta.value = m
    pages.value = new Map()
    pageOrder.length = 0
    version.value += 1
    scrollToLine(atLine)
    ensureVisiblePages()
  }

  // ---- 尺寸观察 ----

  let observer: ResizeObserver | null = null
  function measure() {
    viewport.value = scrollHost.value?.clientHeight ?? 0
  }

  onMounted(() => {
    measure()
    observer = new ResizeObserver(measure)
    if (scrollHost.value) observer.observe(scrollHost.value)
  })

  // scrollHost 是模板 ref，挂载时机由组件决定（v-show 切换）时在这里补观察。
  watch(scrollHost, (el, old) => {
    if (old) observer?.unobserve(old)
    if (el) {
      observer?.observe(el)
      measure()
    }
  })

  onBeforeUnmount(() => observer?.disconnect())

  return {
    meta,
    error,
    totalLines,
    isEmpty,
    scrollHost,
    scrollHeight,
    windowTop,
    windowRows,
    logicalScroll,
    firstVisible,
    onWheel,
    onScroll,
    scrollToLogical,
    scrollToLine,
    currentLine,
    setMeta,
    resetPages,
    ensureVisiblePages,
  }
}
