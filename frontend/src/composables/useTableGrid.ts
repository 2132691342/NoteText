// 大表格网格：分页取数 + 虚拟滚动（空间压缩）+ 覆盖层编辑。
//
// 为什么要有「空间压缩」：Chrome 的元素高度上限约 33,554,428px。
// 1000 万行 × 26px = 2.6 亿 px，直接映射会让滚动条彻底失效。
// 做法是：滚动条的长度按比例压缩，行本身仍按真实高度渲染；
// 滚轮按「行」步进（不受压缩影响），拖滚动条时按比例换算回逻辑位置。
//
// 坐标系（三个，别混）：
// - 逻辑像素：displayRows × ROW_H，与文件真实行数对应；
// - 滚动条像素：压缩后的空间，范围 [0, scrollHeight - viewport]；
// - 显示下标：数据行在可见序列里的序号（删除的行不占位，插入的行占位）。
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch, type Ref } from 'vue'
import { api, normalizeError } from '@/api'
import type { TableMetaVO, TableOp } from '@/types/api'

/** 行高：与 CSS 的行高保持一致。 */
export const ROW_H = 26
/** 滚动空间的上限：远低于 Chrome 的元素高度上限，留足余量。 */
const MAX_SCROLL_PX = 20_000_000
/** 一页多少行：一次 IPC 取一屏的十几倍。 */
const PAGE_SIZE = 500
/** 视口上下各多渲染几行，快速滚动不露白。 */
const OVERSCAN = 12
/** 预取页数：滚到某页之前先把它前面几页取来。 */
const PREFETCH_PAGES = 2

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

  // ---- 滚动坐标 ----

  const scrollHost = ref<HTMLElement | null>(null)
  /** 滚动条空间的滚动位置（压缩后的像素）。 */
  const scrollTop = ref(0)
  /** 视口高度。 */
  const viewport = ref(0)

  /** 逻辑总高度。 */
  const logicalHeight = computed(() => displayRows.value * ROW_H)
  /** 逻辑可滚距离。 */
  const maxLogical = computed(() => Math.max(0, logicalHeight.value - viewport.value))
  /** 滚动条空间高度：压缩到上限之内。 */
  const scrollHeight = computed(() => Math.min(logicalHeight.value, MAX_SCROLL_PX))
  /**
   * 压缩比：滚动条像素 → 逻辑像素。
   *
   * 不能用 `scrollHeight / logicalHeight`：那会让底部差一屏——
   * 滚动条到底时对应的是 `scrollHeight - viewport`，不是 `scrollHeight`。
   * 这里用 `(scrollHeight - viewport) / maxLogical`，两端都精确对齐。
   */
  const scale = computed(() => {
    if (maxLogical.value <= 0) return 1
    const denom = scrollHeight.value - viewport.value
    return denom > 0 ? denom / maxLogical.value : 1
  })

  /** 视口顶部的逻辑位置。 */
  const logicalScroll = computed(() =>
    maxLogical.value <= 0 ? 0 : Math.min(scrollTop.value / scale.value, maxLogical.value),
  )
  /** 视口顶部的显示行。 */
  const firstVisible = computed(() => Math.floor(logicalScroll.value / ROW_H))
  /** 渲染窗口的首行（带 overscan）。 */
  const windowStart = computed(() => Math.max(0, firstVisible.value - OVERSCAN))
  /** 渲染窗口的末行。 */
  const windowEnd = computed(() => {
    const visible = Math.ceil(viewport.value / ROW_H) + OVERSCAN * 2 + 1
    return Math.min(displayRows.value, windowStart.value + visible)
  })
  /**
   * 渲染窗口的顶边（相对滚动内容）。
   *
   * 窗口里的第一行是 `windowStart`，它应当出现在视口里
   * `firstVisible - windowStart` 行之后的位置——那几行是预渲染的余量，
   * 已经被滚过去了，所以窗口要往上提那么多。
   */
  const windowTop = computed(
    () => scrollTop.value - (firstVisible.value - windowStart.value) * ROW_H,
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
      /** 插入行的标记：同一锚点下的第几条。 */
      inserted: boolean
    }> = []
    const ops = insertedRows.value
    for (let d = start; d < end; d += 1) {
      const r = originalRowAt(d)
      if (r < dataStart.value) continue
      const slot = displayIndexOf(r)
      if (slot === d) {
        // 原始行本身（未被删）。
        if (deletedRows.value.has(r)) continue
        out.push({
          index: d,
          orig: r,
          label: String(r),
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

  // ---- 取数 ----

  /** 取一页（已在缓存里就跳过）。 */
  async function ensurePage(pageIdx: number) {
    if (!meta.value || pending.has(pageIdx)) return
    if (pages.value.has(pageIdx)) return
    pending.add(pageIdx)
    try {
      const res = await api.table.page({
        path: opts.path.value,
        start_row: pageIdx * PAGE_SIZE,
        rows: PAGE_SIZE,
      })
      const next = new Map(pages.value)
      next.set(pageIdx, res.rows)
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
    // 预取：往前的几页先取来，滚过去时不用等。
    for (let p = firstPage - 1; p >= firstPage - PREFETCH_PAGES; p -= 1) {
      if (p >= 0) void ensurePage(p)
    }
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
      cellEdits.value = new Map()
      deletedRows.value = new Set()
      insertedRows.value = new Map()
      version.value += 1
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
  onMounted(() => {
    viewport.value = scrollHost.value?.clientHeight ?? 0
    observer = new ResizeObserver(() => {
      viewport.value = scrollHost.value?.clientHeight ?? 0
    })
    if (scrollHost.value) observer.observe(scrollHost.value)
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
    open,
    close,
    onWheel,
    onScroll,
    scrollToLogical,
    scrollToDisplayRow,
    scrollToOriginalRow,
    displayIndexOf,
    setCell,
    isDirtyCell,
    deleteRow,
    insertRowAfter,
    resetEdits,
    buildOps,
    ensureVisiblePages,
  }
}
