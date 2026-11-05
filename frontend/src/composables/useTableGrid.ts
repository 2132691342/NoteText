// 大表格网格：分页取数 + 虚拟滚动（空间压缩）+ 覆盖层编辑。
// 坐标系与窗口计算在 gridScroll.ts。
// 关键不变量：**滚动容器的 DOM 必须挂到 scrollHost 上**——量不到视口时窗口只剩几行。
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch, type Ref } from 'vue'
import { api, normalizeError } from '@/api'
import { computeGridWindow, FALLBACK_VIEWPORT, MAX_SCROLL_PX, ROW_H } from '@/composables/gridScroll'
import type { TableMetaVO, TableOp } from '@/types/api'

export { ROW_H }

/** 一页多少行：一次 IPC 取一屏的十几倍。 */
const PAGE_SIZE = 500
/** 预取页数：滚到某页之前先把它前后几页取来。 */
const PREFETCH_PAGES = 2
/** 页缓存上限：滚动很远时丢最早取的页，避免开着大表吃满内存。 */
const MAX_CACHED_PAGES = 48
/** 列宽：初值、下限、上限（像素）。 */
export const COL_W_DEFAULT = 160
const COL_W_MIN = 56
const COL_W_MAX = 640

export interface TableGridOptions {
  path: Ref<string>
  /** 编码：不指定时由后端探测。 */
  encoding?: Ref<string | undefined>
  /** 分隔符：不指定时由后端嗅探。 */
  delimiter?: Ref<string | undefined>
  /** 索引建好后回调（供状态栏刷新读数）。 */
  onMeta?: (meta: TableMetaVO) => void
  /** 出错回调。 */
  onError?: (message: string) => void
}

export function useTableGrid(opts: TableGridOptions) {
  const meta = ref<TableMetaVO | null>(null)
  const loading = ref(false)
  const error = ref('')

  /** 页缓存：页号 → 行数组。整体替换 Map，避免深度响应式。 */
  const pages = shallowRef(new Map<number, string[][]>())
  /** 取页顺序：超出上限时先丢最早取的。 */
  const pageOrder: number[] = []
  /** 数据版本号：页缓存与覆盖层变化时 +1。 */
  const version = ref(0)
  /** 正在请求的页。 */
  const pending = new Set<number>()

  /** 单元格覆盖层：原始行号 → (列号 → 新值)。 */
  const cellEdits = shallowRef(new Map<number, Map<number, string>>())
  /** 被删的原始行号。 */
  const deletedRows = shallowRef(new Set<number>())
  /** 插入的记录：锚点原始行号 → 记录列表（插在该行之后）。 */
  const insertedRows = shallowRef(new Map<number, string[][]>())

  const editCount = computed(
    () =>
      [...cellEdits.value.values()].reduce((n, m) => n + m.size, 0) +
      deletedRows.value.size +
      [...insertedRows.value.values()].reduce((n, v) => n + v.length, 0),
  )

  // ---- 形状 ----

  const hasHeader = computed(() => meta.value?.has_header ?? false)
  /** 数据起始的原始行号：有表头时第 0 行是表头。 */
  const dataStart = computed(() => (hasHeader.value ? 1 : 0))
  const totalRows = computed(() => meta.value?.total_rows ?? 0)
  const totalCols = computed(() => meta.value?.total_cols ?? 0)
  const header = computed(() => meta.value?.header ?? [])

  /** 显示行数：数据行减去删除的，加上插入的。 */
  const displayRows = computed(() => {
    let n = Math.max(0, totalRows.value - dataStart.value)
    n -= deletedRows.value.size
    for (const list of insertedRows.value.values()) n += list.length
    return n
  })

  // ---- 列宽 ----

  /** 列宽覆盖层：列号 → 像素。用户拖过或双击自适应后落在这里。 */
  const colWidths = shallowRef(new Map<number, number>())
  /** 自动列宽（后端按采样估的）：打开时一次性写入。 */
  const autoWidths = shallowRef<number[]>([])

  /** 某列的实际宽度。 */
  function colWidth(col: number): number {
    const manual = colWidths.value.get(col)
    if (manual != null) return manual
    const auto = autoWidths.value[col]
    return auto ?? COL_W_DEFAULT
  }

  /** 设列宽（拖拽结束时调）。 */
  function setColWidth(col: number, width: number) {
    const next = new Map(colWidths.value)
    next.set(col, Math.round(Math.min(COL_W_MAX, Math.max(COL_W_MIN, width))))
    colWidths.value = next
  }

  /** 列宽合计：横向滚动的内容宽度。 */
  const totalWidth = computed(() => {
    let w = 0
    for (let c = 0; c < totalCols.value; c += 1) w += colWidth(c)
    return w
  })

  /** 恢复自动列宽（双击列分隔线时按当前页内容重算）。 */
  function autoFitColumn(col: number): number {
    // 采样已缓存的页：没缓存的列只按表头估，不会为了量宽度去翻页。
    let widest = measureText(header.value[col] ?? '') + 20
    for (const page of pages.value.values()) {
      for (const row of page) {
        const text = row[col] ?? ''
        if (text.length > 64) {
          widest = Math.max(widest, COL_W_MAX)
          continue
        }
        widest = Math.max(widest, measureText(text) + 20)
      }
    }
    const w = Math.min(COL_W_MAX, Math.max(COL_W_MIN, widest))
    const next = new Map(colWidths.value)
    next.set(col, w)
    colWidths.value = next
    return w
  }

  /** 估算一段文本的像素宽度：中日韩按 2 个半角算。 */
  function measureText(s: string): number {
    let w = 0
    for (const ch of s) w += /[\u2e80-\u9fff\uff00-\uffef]/.test(ch) ? 13 : 7
    return w
  }

  // ---- 滚动坐标 ----

  const scrollHost = ref<HTMLElement | null>(null)
  /** 滚动条空间的滚动位置（压缩后的像素）。 */
  const scrollTop = ref(0)
  /** 视口高度（像素）。 */
  const viewport = ref(0)

  /** 逻辑总高度。 */
  const logicalHeight = computed(() => displayRows.value * ROW_H)
  /** 逻辑可滚距离。 */
  const maxLogical = computed(() => Math.max(0, logicalHeight.value - viewport.value))
  /** 滚动条空间高度：压缩到上限之内。 */
  const scrollHeight = computed(() => Math.min(logicalHeight.value, MAX_SCROLL_PX))
  /** 渲染窗口：一次算完，滚动坐标与窗口位置同源。 */
  const grid = computed(() =>
    computeGridWindow(displayRows.value, viewport.value, scrollTop.value),
  )
  /** 压缩比：滚动条像素 → 逻辑像素。 */
  const scale = computed(() => grid.value.scale)
  /** 视口顶部的逻辑位置。 */
  const logicalScroll = computed(() => grid.value.logicalScroll)
  /** 视口顶部的显示行。 */
  const firstVisible = computed(() => grid.value.firstVisible)
  /** 渲染窗口的首行（带 overscan）。 */
  const windowStart = computed(() => grid.value.windowStart)
  /** 渲染窗口的末行。 */
  const windowEnd = computed(() => grid.value.windowEnd)
  /** 渲染窗口的顶边（滚动条空间坐标）。 */
  const windowTop = computed(() => grid.value.windowTop)
  /** 视口高度：量不到时用兜底值，别让窗口退化成几行。 */
  const effectiveViewport = computed(() =>
    viewport.value > 0 ? viewport.value : FALLBACK_VIEWPORT,
  )

  // ---- 显示下标 ↔ 原始行 ----

  /** 被删的行号（升序）。 */
  const deletedList = computed(() => [...deletedRows.value].sort((a, b) => a - b))
  /** 插入锚点（升序）与累计条数。 */
  const insertIndex = computed(() => {
    const keys = [...insertedRows.value.keys()].sort((a, b) => a - b)
    const cum: number[] = []
    let total = 0
    for (const k of keys) {
      cum.push(total)
      total += insertedRows.value.get(k)?.length ?? 0
    }
    return { keys, cum, total }
  })

  /** 原始行 r 之前被删的行数。 */
  function deletedBefore(r: number): number {
    const list = deletedList.value
    let lo = 0
    let hi = list.length
    while (lo < hi) {
      const mid = (lo + hi) >> 1
      if (list[mid] < r) lo = mid + 1
      else hi = mid
    }
    return lo
  }

  /** 原始行 r 之前插入的记录总数（锚点在 r 之前的算在内）。 */
  function insertedBefore(r: number): number {
    const { keys, cum } = insertIndex.value
    let lo = 0
    let hi = keys.length
    while (lo < hi) {
      const mid = (lo + hi) >> 1
      if (keys[mid] < r) lo = mid + 1
      else hi = mid
    }
    if (lo === 0) return 0
    return cum[lo - 1] + (insertedRows.value.get(keys[lo - 1])?.length ?? 0)
  }

  /** 原始行 r 在显示序列里的下标。 */
  function displayIndexOf(r: number): number {
    return r - dataStart.value - deletedBefore(r) + insertedBefore(r)
  }

  /** 显示下标 d 处的原始行号（二分）。 */
  function originalRowAt(d: number): number {
    let lo = dataStart.value
    let hi = totalRows.value - 1
    let best = dataStart.value
    while (lo <= hi) {
      const mid = (lo + hi) >> 1
      if (displayIndexOf(mid) <= d) {
        best = mid
        lo = mid + 1
      } else {
        hi = mid - 1
      }
    }
    return best
  }

  /** 渲染窗口里的每一行。 */
  const windowRows = computed(() => {
    const start = windowStart.value
    const end = windowEnd.value
    if (start >= end) return []
    const out: Array<{
      /** 显示下标。 */
      index: number
      /** 原始行号；插入行为 null。 */
      orig: number | null
      /** 行号栏显示的文本。 */
      label: string
      /** 字段值（已应用覆盖层）。 */
      cells: string[]
      /** 插入行的标记。 */
      inserted: boolean
    }> = []
    const ops = insertedRows.value
    for (let d = start; d < end; d += 1) {
      const r = originalRowAt(d)
      if (r < dataStart.value) continue
      const slot = displayIndexOf(r)
      if (slot === d) {
        if (deletedRows.value.has(r)) continue
        out.push({
          index: d,
          orig: r,
          label: String(r + 1),
          cells: cellsOf(r),
          inserted: false,
        })
        continue
      }
      // slot < d：这个位置放着锚在 r 之后的插入行。
      const list = ops.get(r)
      if (!list) continue
      const k = d - slot - 1
      const rec = list[k]
      if (!rec) continue
      out.push({
        index: d,
        orig: null,
        label: '＋',
        cells: rec,
        inserted: true,
      })
    }
    return out
  })

  /** 一行的字段值：页缓存 + 覆盖层。 */
  function cellsOf(orig: number): string[] {
    const pageIdx = Math.floor(orig / PAGE_SIZE)
    const page = pages.value.get(pageIdx)
    const base = page?.[orig - pageIdx * PAGE_SIZE]
    const edits = cellEdits.value.get(orig)
    const width = totalCols.value
    const out: string[] = new Array(width)
    for (let c = 0; c < width; c += 1) {
      if (edits?.has(c)) out[c] = edits.get(c) ?? ''
      else out[c] = base?.[c] ?? ''
    }
    return out
  }

  /** 某格的当前值（含覆盖层）：查找替换要读窗口外的格，不能只查 windowRows。 */
  function cellValue(orig: number, col: number): string {
    const edits = cellEdits.value.get(orig)
    if (edits?.has(col)) return edits.get(col) ?? ''
    const pageIdx = Math.floor(orig / PAGE_SIZE)
    const page = pages.value.get(pageIdx)
    return page?.[orig - pageIdx * PAGE_SIZE]?.[col] ?? ''
  }

  // ---- 取数 ----

  /** 取一页（已在缓存里就跳过）。 */
  async function ensurePage(pageIdx: number) {
    if (!meta.value || pending.has(pageIdx)) return
    if (pages.value.has(pageIdx)) return
    if (pageIdx < 0 || pageIdx * PAGE_SIZE >= totalRows.value) return
    pending.add(pageIdx)
    try {
      const res = await api.table.page({
        path: opts.path.value,
        start_row: pageIdx * PAGE_SIZE,
        rows: PAGE_SIZE,
      })
      const next = new Map(pages.value)
      next.set(pageIdx, res.rows)
      // 页缓存封顶：先丢最早取的（当前窗口附近的页刚被 ensureVisiblePages 续上，
      // 顺序里排在后面，不会立刻被丢）。
      pageOrder.push(pageIdx)
      while (pageOrder.length > MAX_CACHED_PAGES) {
        const old = pageOrder.shift()
        if (old == null || old === pageIdx) continue
        next.delete(old)
      }
      pages.value = next
      version.value += 1
      if (res.stale) opts.onError?.('stale')
    } catch (e) {
      opts.onError?.(normalizeError(e).message)
    } finally {
      pending.delete(pageIdx)
    }
  }

  /** 保证渲染窗口覆盖的页都在缓存里。 */
  function ensureVisiblePages() {
    if (!meta.value) return
    const first = windowStart.value
    const last = Math.max(first, windowEnd.value - 1)
    const firstPage = Math.floor(first / PAGE_SIZE)
    const lastPage = Math.floor(last / PAGE_SIZE)
    for (let p = firstPage; p <= lastPage; p += 1) void ensurePage(p)
    // 预取：前后各几页，滚过去时不用等。
    for (let p = firstPage - PREFETCH_PAGES; p < firstPage; p += 1) {
      if (p >= 0) void ensurePage(p)
    }
    for (let p = lastPage + 1; p <= lastPage + PREFETCH_PAGES; p += 1) {
      void ensurePage(p)
    }
  }

  /** 取某一行所在页（不等待完成，用于查找跳转前预热）。 */
  async function ensureRowCached(orig: number) {
    await ensurePage(Math.floor(orig / PAGE_SIZE))
  }

  // ---- 滚动 ----

  /** 滚轮：按行步进。压缩后像素步进不精确（3 格滚轮 = 几十行），所以自己算。 */
  function onWheel(e: WheelEvent) {
    if (!meta.value || displayRows.value <= 0) return
    e.preventDefault()
    const dir = e.deltaY > 0 ? 1 : e.deltaY < 0 ? -1 : 0
    if (dir === 0) return
    scrollToLogical(logicalScroll.value + dir * 3 * ROW_H)
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

  /** 滚到第几行（显示下标）。 */
  function scrollToDisplayRow(index: number) {
    const target = Math.max(0, Math.min(index, displayRows.value - 1))
    scrollToLogical(target * ROW_H)
  }

  /** 滚到某个原始行（显示下标由映射算出）。 */
  function scrollToOriginalRow(orig: number) {
    scrollToDisplayRow(displayIndexOf(orig))
  }

  /** 把某格滚进视口并尽量居中：查找跳转要的是「看到这一格」。 */
  function scrollToCell(orig: number, col: number) {
    // 纵向：目标行放在视口中间
    const d = displayIndexOf(orig)
    const center = d * ROW_H - effectiveViewport.value / 2 + ROW_H / 2
    scrollToLogical(center)
    // 横向：列不在视口内才滚
    const host = scrollHost.value
    if (!host) return
    let left = 0
    for (let c = 0; c < col; c += 1) left += colWidth(c)
    const right = left + colWidth(col)
    const viewLeft = host.scrollLeft
    const viewRight = viewLeft + host.clientWidth
    if (left < viewLeft || right > viewRight) {
      host.scrollLeft = Math.max(0, left - host.clientWidth / 3)
    }
  }

  /** 滚动条被拖动：按比例换算回逻辑位置。 */
  function onScroll() {
    const host = scrollHost.value
    if (!host || !meta.value) return
    scrollTop.value = host.scrollTop
  }

  watch([windowStart, windowEnd, version], () => {
    ensureVisiblePages()
  })

  // ---- 打开 / 关闭 ----

  /** 建索引并载入首页。 */
  async function open() {
    const path = opts.path.value
    if (!path) return
    loading.value = true
    error.value = ''
    try {
      const m = await api.table.open({
        path,
        encoding: opts.encoding?.value ?? null,
        delimiter: opts.delimiter?.value ?? null,
      })
      meta.value = m
      pages.value = new Map()
      pageOrder.length = 0
      cellEdits.value = new Map()
      deletedRows.value = new Set()
      insertedRows.value = new Map()
      colWidths.value = new Map()
      // 后端按采样给的列宽（字符宽度 → 像素），没给就留空走默认档。
      autoWidths.value = (m.col_widths ?? []).map((w) =>
        w == null ? COL_W_DEFAULT : Math.min(COL_W_MAX, Math.max(COL_W_MIN, Math.round(w * 7.4))),
      )
      version.value += 1
      scrollTop.value = 0
      scrollHost.value && (scrollHost.value.scrollTop = 0)
      opts.onMeta?.(m)
      ensureVisiblePages()
    } catch (e) {
      error.value = normalizeError(e).message
      opts.onError?.(error.value)
    } finally {
      loading.value = false
    }
  }

  /** 关标签：释放后端索引。 */
  async function close() {
    const path = opts.path.value
    if (!path) return
    try {
      await api.table.close(path)
    } catch {
      /* 释放失败不影响使用 */
    }
  }

  // ---- 编辑 ----

  /** 改一个单元格。 */
  function setCell(orig: number, col: number, value: string) {
    const next = new Map(cellEdits.value)
    let rowEdits = next.get(orig)
    if (!rowEdits) {
      rowEdits = new Map()
      next.set(orig, rowEdits)
    }
    rowEdits.set(col, value)
    cellEdits.value = next
    version.value += 1
  }

  /** 某格是否被改过（脏格高亮）。 */
  function isDirtyCell(orig: number, col: number): boolean {
    return cellEdits.value.get(orig)?.has(col) ?? false
  }

  /** 删一行。 */
  function deleteRow(orig: number) {
    const next = new Set(deletedRows.value)
    next.add(orig)
    deletedRows.value = next
    version.value += 1
  }

  /** 在某行之后插入一条记录。 */
  function insertRowAfter(orig: number, values: string[]) {
    const next = new Map(insertedRows.value)
    const list = next.get(orig)
    if (list) list.push(values)
    else next.set(orig, [values])
    insertedRows.value = next
    version.value += 1
  }

  /** 撤销全部改动。 */
  function resetEdits() {
    cellEdits.value = new Map()
    deletedRows.value = new Set()
    insertedRows.value = new Map()
    version.value += 1
  }

  /** 改动清单（写回用）。 */
  function buildOps(): TableOp[] {
    const ops: TableOp[] = []
    for (const [row, cols] of cellEdits.value) {
      for (const [col, value] of cols) {
        ops.push({ kind: 'set_cell', row, col, value })
      }
    }
    for (const row of deletedRows.value) {
      ops.push({ kind: 'delete_row', row })
    }
    for (const [after, list] of insertedRows.value) {
      for (const values of list) {
        // 后端语义：插在第 N 行之前。前端锚点是「某行之后」，所以 +1。
        ops.push({ kind: 'insert_row', row: after + 1, values })
      }
    }
    return ops
  }

  // ---- 尺寸观察 ----

  let observer: ResizeObserver | null = null
  /** 量一次视口高度。挂载时机由组件决定，scrollHost 变化时也要重量。 */
  function measure() {
    viewport.value = scrollHost.value?.clientHeight ?? 0
  }

  onMounted(() => {
    measure()
    observer = new ResizeObserver(measure)
    if (scrollHost.value) observer.observe(scrollHost.value)
  })

  // scrollHost 是模板 ref，组件挂载后才赋值：没被 onMounted 覆盖到的
  // 情形（条件渲染、v-if 切换）在这里补观察，否则 viewport 永远停在 0。
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
    loading,
    error,
    header,
    hasHeader,
    totalRows,
    totalCols,
    displayRows,
    dataStart,
    editCount,
    scrollHost,
    scrollHeight,
    windowTop,
    windowRows,
    logicalScroll,
    firstVisible,
    colWidths,
    totalWidth,
    colWidth,
    setColWidth,
    autoFitColumn,
    open,
    close,
    measure,
    onWheel,
    onScroll,
    scrollToLogical,
    scrollToDisplayRow,
    scrollToOriginalRow,
    scrollToCell,
    displayIndexOf,
    cellValue,
    setCell,
    isDirtyCell,
    deleteRow,
    insertRowAfter,
    resetEdits,
    buildOps,
    ensureVisiblePages,
    ensureRowCached,
  }
}
