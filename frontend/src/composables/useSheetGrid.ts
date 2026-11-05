// XLSX 工作表网格：分页取数 + 虚拟滚动（空间压缩）+ 单元格补丁；保存走后端「原地改 XML」。
// 与大表格（CSV）同构，但单元格的值是领域对象（公式 / 数值 / 布尔）。
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch, type Ref } from 'vue'
import { api, normalizeError } from '@/api'
import { computeGridWindow, FALLBACK_VIEWPORT, MAX_SCROLL_PX, ROW_H } from '@/composables/gridScroll'
import type { CellValue, SheetCell, SheetInfoVO, WorkbookInfoVO } from '@/types/api'

export { ROW_H }
/** 一页多少行。 */
const PAGE_SIZE = 500
/** 预取页数。 */
const PREFETCH_PAGES = 3
/** 页缓存上限：丢最久没用过的页（LRU），开着大工作簿不无限吃内存。 */
const MAX_CACHED_PAGES = 80
/** 列宽：默认、下限、上限（像素）。 */
const COL_W_DEFAULT = 120
const COL_W_MIN = 56
const COL_W_MAX = 640

export interface SheetGridOptions {
  path: Ref<string>
  /** 吸顶表头的高度（像素）：数据行从它下面开始，不算会把前几行盖住。 */
  headerOffset?: Ref<number>
  onError?: (message: string) => void
  onMeta?: (info: WorkbookInfoVO) => void
}

export function useSheetGrid(opts: SheetGridOptions) {
  const info = ref<WorkbookInfoVO | null>(null)
  const loading = ref(false)
  const error = ref('')
  const activeSheet = ref(0)

  /** 页缓存：页号 → 行数组。 */
  const pages = shallowRef(new Map<number, SheetCell[][]>())
  const version = ref(0)
  /** 正在请求的页。 */
  const inflight = new Set<number>()
  /** 取页顺序：超出上限时先丢最早取的。 */
  const pageOrder: number[] = []

  // ---- 列宽 ----

  /** 列宽覆盖层：列号 → 像素（用户拖过 / 双击自适应后落在这里）。 */
  const colWidths = shallowRef(new Map<number, number>())
  /** 后端给的列宽（字符宽度 → 像素）。 */
  const autoWidths = shallowRef<number[]>([])

  function colWidth(col: number): number {
    const manual = colWidths.value.get(col)
    if (manual != null) return manual
    const auto = autoWidths.value[col]
    return auto ?? COL_W_DEFAULT
  }

  function setColWidth(col: number, width: number) {
    const next = new Map(colWidths.value)
    next.set(col, Math.round(Math.min(COL_W_MAX, Math.max(COL_W_MIN, width))))
    colWidths.value = next
  }

  const totalWidth = computed(() => {
    let w = 0
    for (let c = 0; c < totalCols.value; c += 1) w += colWidth(c)
    return w
  })

  /** 待写回的改动：`工作表下标` → (`行:列` → 值)。 */
  const pendingBySheet = shallowRef(new Map<number, Map<string, CellValue | null>>())

  const dirtyCount = computed(() => {
    let n = 0
    for (const m of pendingBySheet.value.values()) n += m.size
    return n
  })

  // ---- 形状 ----

  const sheets = computed<SheetInfoVO[]>(() => info.value?.sheets ?? [])
  const sheet = computed<SheetInfoVO | null>(() => sheets.value[activeSheet.value] ?? null)
  const totalRows = computed(() => sheet.value?.total_rows ?? 0)
  const totalCols = computed(() => sheet.value?.total_cols ?? 0)

  /** 当前表的待写回改动。 */
  const pending = computed<Map<string, CellValue | null>>(
    () => pendingBySheet.value.get(activeSheet.value) ?? new Map(),
  )

  // ---- 滚动坐标 ----

  const scrollHost = ref<HTMLElement | null>(null)
  const scrollTop = ref(0)
  const viewport = ref(0)

  const logicalHeight = computed(() => totalRows.value * ROW_H)
  const maxLogical = computed(() => Math.max(0, logicalHeight.value - viewport.value))
  const scrollHeight = computed(() => Math.min(logicalHeight.value, MAX_SCROLL_PX))
  /** 渲染窗口：一次算完，滚动坐标与窗口位置同源。 */
  const grid = computed(() =>
    computeGridWindow(
      totalRows.value,
      viewport.value,
      scrollTop.value,
      opts.headerOffset?.value ?? 0,
    ),
  )
  const scale = computed(() => grid.value.scale)
  const logicalScroll = computed(() => grid.value.logicalScroll)
  const windowStart = computed(() => grid.value.windowStart)
  const windowEnd = computed(() => grid.value.windowEnd)
  const windowTop = computed(() => grid.value.windowTop)
  /** 视口高度：量不到时用兜底值，别让窗口退化成几行。 */
  const effectiveViewport = computed(() =>
    viewport.value > 0 ? viewport.value : FALLBACK_VIEWPORT,
  )

  // ---- 取数 ----

  async function ensurePage(pageIdx: number) {
    if (!info.value || inflight.has(pageIdx)) return
    if (pages.value.has(pageIdx)) {
      const at = pageOrder.indexOf(pageIdx)
      if (at >= 0 && at !== pageOrder.length - 1) {
        pageOrder.splice(at, 1)
        pageOrder.push(pageIdx)
      }
      return
    }
    if (pageIdx < 0 || pageIdx * PAGE_SIZE >= totalRows.value) return
    inflight.add(pageIdx)
    try {
      const res = await api.sheet.page({
        path: opts.path.value,
        sheet_index: activeSheet.value,
        start_row: pageIdx * PAGE_SIZE,
        rows: PAGE_SIZE,
      })
      const next = new Map(pages.value)
      next.set(pageIdx, res.rows)
      // 页缓存封顶：丢最久没用过的页（LRU）。
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
      inflight.delete(pageIdx)
    }
  }

  function ensureVisiblePages() {
    if (!info.value) return
    const first = windowStart.value
    const last = Math.max(first, windowEnd.value - 1)
    const firstPage = Math.floor(first / PAGE_SIZE)
    const lastPage = Math.floor(last / PAGE_SIZE)
    for (let p = firstPage; p <= lastPage; p += 1) void ensurePage(p)
    // 前后各预取几页，滚过去时不用等。
    for (let p = firstPage - PREFETCH_PAGES; p < firstPage; p += 1) {
      if (p >= 0) void ensurePage(p)
    }
    for (let p = lastPage + 1; p <= lastPage + PREFETCH_PAGES; p += 1) {
      void ensurePage(p)
    }
  }

  /** 渲染窗口里的每一行。 */
  const windowRows = computed(() => {
    const out: Array<{ index: number; cells: (CellValue | null)[] }> = []
    const start = windowStart.value
    const end = windowEnd.value
    const width = totalCols.value
    const edits = pending.value
    for (let r = start; r < end; r += 1) {
      const pageIdx = Math.floor(r / PAGE_SIZE)
      const page = pages.value.get(pageIdx)
      const rowCells = page?.[r - pageIdx * PAGE_SIZE] ?? []
      // 本行的单元格：先按后端给的顺序填，再补到列宽。
      const cells: (CellValue | null)[] = new Array(width).fill(null)
      for (const c of rowCells) {
        if (c.col < width) cells[c.col] = c.value
      }
      // 覆盖层优先。
      for (const [key, v] of edits) {
        const [rr, cc] = key.split(':').map(Number)
        if (rr === r && cc < width) cells[cc] = v
      }
      out.push({ index: r, cells })
    }
    return out
  })

  /** 某格是否被改过。 */
  function isDirtyCell(row: number, col: number): boolean {
    return pending.value.has(`${row}:${col}`)
  }

  /** 改一格。 */
  function setCell(row: number, col: number, value: CellValue | null) {
    const sheetIdx = activeSheet.value
    const cur = pendingBySheet.value.get(sheetIdx)
    const map = new Map(cur ?? [])
    map.set(`${row}:${col}`, value)
    const next = new Map(pendingBySheet.value)
    next.set(sheetIdx, map)
    pendingBySheet.value = next
    version.value += 1
  }

  /** 撤销全部改动。 */
  function resetEdits() {
    const next = new Map(pendingBySheet.value)
    next.delete(activeSheet.value)
    pendingBySheet.value = next
    version.value += 1
  }

  // ---- 滚动 ----

  // 滚轮与滚动条事件 rAF 合帧：一帧之内只应用最新状态（与大表格网格同一档）。
  let scrollRaf = 0
  let wheelRaf = 0
  let wheelAcc = 0

  function onWheel(e: WheelEvent) {
    if (!info.value || totalRows.value <= 0) return
    e.preventDefault()
    const dir = e.deltaY > 0 ? 1 : e.deltaY < 0 ? -1 : 0
    if (dir === 0) return
    wheelAcc += dir * 3 * ROW_H
    if (!wheelRaf) {
      wheelRaf = requestAnimationFrame(() => {
        wheelRaf = 0
        const d = wheelAcc
        wheelAcc = 0
        if (d !== 0) scrollToLogical(logicalScroll.value + d)
      })
    }
  }

  function scrollToLogical(target: number) {
    const clamped = Math.max(0, Math.min(target, maxLogical.value))
    const px = clamped * scale.value
    scrollTop.value = px
    const host = scrollHost.value
    if (host && Math.abs(host.scrollTop - px) > 0.5) host.scrollTop = px
  }

  /** 滚到某行并尽量居中。 */
  function scrollToRow(row: number) {
    const header = opts.headerOffset?.value ?? 0
    const vp = Math.max(ROW_H, effectiveViewport.value - header)
    const center = row * ROW_H - vp / 2 + ROW_H / 2
    scrollToLogical(Math.max(0, center))
  }

  /** 滚动条被拖动（rAF 合帧）。 */
  function onScroll() {
    if (scrollRaf) return
    scrollRaf = requestAnimationFrame(() => {
      scrollRaf = 0
      const host = scrollHost.value
      if (!host || !info.value) return
      scrollTop.value = host.scrollTop
    })
  }

  // ---- 打开 ----

  /** 读工作簿摘要（只给表名与行列数，不读单元格）。 */
  async function open() {
    const path = opts.path.value
    if (!path) return
    loading.value = true
    error.value = ''
    try {
      const meta = await api.sheet.info(path)
      info.value = meta
      pages.value = new Map()
      activeSheet.value = 0
      version.value += 1
      opts.onMeta?.(meta)
      // 列宽：后端按采样给字符宽度，转成像素；没给就留空走默认档。
      autoWidths.value = []
      ensureVisiblePages()
    } catch (e) {
      error.value = normalizeError(e).message
      opts.onError?.(error.value)
    } finally {
      loading.value = false
    }
  }

  /** 切工作表：清缓存，重新取数。 */
  function selectSheet(index: number) {
    if (activeSheet.value === index) return
    activeSheet.value = index
    pages.value = new Map()
    pageOrder.length = 0
    version.value += 1
    scrollToLogical(0)
    ensureVisiblePages()
  }

  watch([windowStart, windowEnd, version, activeSheet], () => {
    ensureVisiblePages()
  })

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

  // scrollHost 是模板 ref：组件挂载后才赋值，v-if / 条件渲染下 onMounted
  // 可能量不到元素，这里补上观察，否则 viewport 永远停在 0。
  watch(scrollHost, (el, old) => {
    if (old) observer?.unobserve(old)
    if (el) {
      observer?.observe(el)
      measure()
    }
  })

  onBeforeUnmount(() => {
    observer?.disconnect()
    if (scrollRaf) cancelAnimationFrame(scrollRaf)
    if (wheelRaf) cancelAnimationFrame(wheelRaf)
  })

  return {
    info,
    loading,
    error,
    sheets,
    sheet,
    activeSheet,
    totalRows,
    totalCols,
    pending,
    pendingGroups: pendingBySheet,
    dirtyCount,
    scrollHost,
    scrollHeight,
    windowTop,
    windowRows,
    colWidths,
    totalWidth,
    colWidth,
    setColWidth,
    logicalScroll,
    open,
    selectSheet,
    onWheel,
    onScroll,
    scrollToLogical,
    scrollToRow,
    setCell,
    isDirtyCell,
    resetEdits,
    ensureVisiblePages,
  }
}
