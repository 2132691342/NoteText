// XLSX 工作表网格：分页取数 + 虚拟滚动（空间压缩）+ 单元格补丁。
//
// 与大表格（CSV）同构，但单元格的值是领域对象（公式 / 数值 / 布尔），
// 保存走后端的「原地改 XML」：只重写命中的单元格，其余部件字节级原样。
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch, type Ref } from 'vue'
import { api, normalizeError } from '@/api'
import type { CellValue, SheetCell, SheetInfoVO, WorkbookInfoVO } from '@/types/api'

/** 行高：与 CSS 的行高保持一致。 */
export const ROW_H = 26
/** 滚动空间的上限：远低于 Chrome 的元素高度上限。 */
const MAX_SCROLL_PX = 20_000_000
/** 一页多少行。 */
const PAGE_SIZE = 500
/** 视口上下各多渲染几行。 */
const OVERSCAN = 12
/** 预取页数。 */
const PREFETCH_PAGES = 2

export interface SheetGridOptions {
  path: Ref<string>
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
  const scale = computed(() => {
    if (maxLogical.value <= 0) return 1
    const denom = scrollHeight.value - viewport.value
    return denom > 0 ? denom / maxLogical.value : 1
  })
  const logicalScroll = computed(() =>
    maxLogical.value <= 0 ? 0 : Math.min(scrollTop.value / scale.value, maxLogical.value),
  )
  const firstVisible = computed(() => Math.floor(logicalScroll.value / ROW_H))
  const windowStart = computed(() => Math.max(0, firstVisible.value - OVERSCAN))
  const windowEnd = computed(() => {
    const visible = Math.ceil(viewport.value / ROW_H) + OVERSCAN * 2 + 1
    return Math.min(totalRows.value, windowStart.value + visible)
  })
  const windowTop = computed(
    () => scrollTop.value - (firstVisible.value - windowStart.value) * ROW_H,
  )

  // ---- 取数 ----

  async function ensurePage(pageIdx: number) {
    if (!info.value || inflight.has(pageIdx)) return
    if (pages.value.has(pageIdx)) return
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
    for (let p = firstPage - 1; p >= firstPage - PREFETCH_PAGES; p -= 1) {
      if (p >= 0) void ensurePage(p)
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

  function onWheel(e: WheelEvent) {
    if (!info.value || totalRows.value <= 0) return
    e.preventDefault()
    const dir = e.deltaY > 0 ? 1 : e.deltaY < 0 ? -1 : 0
    if (dir === 0) return
    scrollToLogical(logicalScroll.value + dir * 3 * ROW_H)
  }

  function scrollToLogical(target: number) {
    const clamped = Math.max(0, Math.min(target, maxLogical.value))
    const px = clamped * scale.value
    scrollTop.value = px
    const host = scrollHost.value
    if (host && Math.abs(host.scrollTop - px) > 0.5) host.scrollTop = px
  }

  function scrollToRow(row: number) {
    scrollToLogical(Math.max(0, Math.min(row, totalRows.value - 1)) * ROW_H)
  }

  function onScroll() {
    const host = scrollHost.value
    if (!host || !info.value) return
    scrollTop.value = host.scrollTop
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
    version.value += 1
    scrollToLogical(0)
    ensureVisiblePages()
  }

  watch([windowStart, windowEnd, version, activeSheet], () => {
    ensureVisiblePages()
  })

  // ---- 尺寸观察 ----

  let observer: ResizeObserver | null = null
  onMounted(() => {
    viewport.value = scrollHost.value?.clientHeight ?? 0
    observer = new ResizeObserver(() => {
      viewport.value = scrollHost.value?.clientHeight ?? 0
    })
    if (scrollHost.value) observer.observe(scrollHost.value)
  })
  onBeforeUnmount(() => observer?.disconnect())

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
