// 大表格网格：分页取数 + 虚拟滚动（空间压缩）+ 覆盖层编辑；坐标系在 gridScroll.ts。
// 关键不变量：**滚动容器的 DOM 必须挂到 scrollHost 上**——量不到视口时窗口只剩几行。
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch, type Ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { api, normalizeError } from '@/api'
import {
  computeGridWindow,
  contentYOf,
  FALLBACK_VIEWPORT,
  headerHeightOf,
  MAX_SCROLL_PX,
  ROW_H,
} from '@/composables/gridScroll'
import type { TableMetaVO, TableOp, TableViewVO } from '@/types/api'

export { ROW_H }

/** 一页多少行：一次 IPC 取一屏的十几倍。 */
const PAGE_SIZE = 500
/** 预取页数：滚到某页之前先把它前后几页取来。 */
const PREFETCH_PAGES = 3
/** 页缓存上限：滚动很远时丢最久没用过的页（LRU），避免开着大表吃满内存。 */
const MAX_CACHED_PAGES = 80
/** 列宽：初值、下限、上限（像素）。 */
export const COL_W_DEFAULT = 160
/**
 * 列宽下限。不能设太大：低于它的短列（序号 / 开关位 / 两字颜色）
 * 全部被顶到同一个宽度，一张 13 列的表看起来像 13 个等宽格子，
 * 反而比自适应更浪费横向空间。56 是「数字列 + 表头两字」的下限。
 */
const COL_W_MIN = 56
const COL_W_MAX = 640
/**
 * 后端给的列宽是「字符宽度」（中日韩按 2 个半角算），这里换成像素。
 * 13px 字号下 ASCII 平均字宽约 7.2px、全角约 13px；后端已按 2 折算，
 * 所以统一乘 7.4 再补单元格内边距（左右各 8px），
 * 少算内边距会让每列都差一截，长文本一律被截成省略号。
 */
const COL_W_PAD = 18
function widthFromChars(chars: number): number {
  return Math.round(chars * 7.4 + COL_W_PAD)
}

export interface TableGridOptions {
  path: Ref<string>
  /** 编码：`'auto'` 或不指定时由后端探测。 */
  encoding?: Ref<string | undefined>
  /** 分隔符：不指定时由后端嗅探。 */
  delimiter?: Ref<string | undefined>
  /** 表头判定覆盖；`null` = 交给后端启发式。 */
  hasHeader?: Ref<boolean | null>
  /** 吸顶表头的行数覆盖：默认按「列字母行 + 有表头值行」算（2 或 1）。
   *  是纯函数而不是 computed：setup 阶段 watch 就会求值一次，里面不能碰调用方还没赋值的变量。 */
  headerRows?: () => number
  /** 索引建好后回调（供状态栏刷新读数）。 */
  onMeta?: (meta: TableMetaVO) => void
  /** 出错回调。 */
  onError?: (message: string) => void
}

export function useTableGrid(opts: TableGridOptions) {
  const meta = ref<TableMetaVO | null>(null)
  const loading = ref(false)
  const error = ref('')
  /** 建索引进度（0..1）；null = 没有进行中的扫描。 */
  const progress = ref<number | null>(null)

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

  /** 表格视图（排序 / 筛选）：只改显示顺序，不动文件。 */
  const view = ref<TableViewVO | null>(null)
  /** 当前排序列与方向（`null` = 文件顺序）。 */
  const sortCol = ref<number | null>(null)
  const sortDesc = ref(false)
  /** 筛选条件：关键字 + 限定列（空 = 全列）。 */
  const filterText = ref('')
  const filterCols = shallowRef<number[]>([])
  /** 视图正在重建（排序 / 筛选要顺序扫一遍文件）。 */
  const viewBusy = ref(false)

  /**
   * 撤销栈：只存反向操作，不存快照。
   * 覆盖层本来就是增量，所以「撤销一格」= 把那一格改回旧值；
   * 存整表快照会在几十万行上直接吃光内存。
   */
  type UndoStep = { forward: () => void; backward: () => void }
  const undoStack: UndoStep[] = []
  const redoStack: UndoStep[] = []
  const canUndo = ref(false)
  const canRedo = ref(false)
  /** 栈上限：再多用户也按不回去，占内存没意义。 */
  const UNDO_LIMIT = 200

  function pushUndo(step: UndoStep) {
    undoStack.push(step)
    if (undoStack.length > UNDO_LIMIT) undoStack.shift()
    redoStack.length = 0
    canUndo.value = undoStack.length > 0
    canRedo.value = false
  }

  /** 撤销一步。成对操作：`backward` 回到旧值，栈项转进重做栈。 */
  function undo() {
    const step = undoStack.pop()
    if (!step) return
    step.backward()
    redoStack.push(step)
    canUndo.value = undoStack.length > 0
    canRedo.value = true
    version.value += 1
  }

  /** 重做一步：`forward` 再走一遍。 */
  function redo() {
    const step = redoStack.pop()
    if (!step) return
    step.forward()
    undoStack.push(step)
    canUndo.value = true
    canRedo.value = redoStack.length > 0
    version.value += 1
  }

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
  /** 总记录数（含表头行）：视图与索引的口径一致，筛选后行号栏跟着变少。 */
  const totalRows = computed(() => view.value?.total_rows ?? meta.value?.total_rows ?? 0)
  /** 数据行数（不含表头）：读数里「n / total 行」用这个，否则无表头的表会多算一行。 */
  const viewDataRows = computed(() => {
    if (view.value) return view.value.data_rows
    // 不能用 dataStart：它在本行之后才声明，读到的是 TDZ 里的临时值。
    const headerRows = meta.value?.has_header ? 1 : 0
    return Math.max(0, (meta.value?.total_rows ?? 0) - headerRows)
  })
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

  /** 设列宽（拖拽结束时调，顺便落盘）。 */
  function setColWidth(col: number, width: number) {
    const next = new Map(colWidths.value)
    next.set(col, Math.round(Math.min(COL_W_MAX, Math.max(COL_W_MIN, width))))
    colWidths.value = next
    saveColWidths()
  }

  // ---- 列宽与冻结的记忆 ----
  //
  // 存在 localStorage 而不是数据库：它是「这个文件的查看偏好」，换机器不该跟过去，
  // 也不进导出备份。key 用路径的后缀（同名文件在同目录下不会撞，因为带目录名）。

  const WIDTHS_KEY = 'nt.table.widths'
  const FREEZE_KEY = 'nt.table.freeze'

  /** 冻结首列：横向滚动时第一列常驻，宽表里不至于「滚过去就不知道这行是谁」。 */
  const frozenFirst = ref(false)

  /** 路径 → 序列化后的列宽，避免每个文件一个 key 把 localStorage 撑爆。 */
  function pathKey(): string {
    const p = opts.path.value
    if (!p) return ''
    // 用文件名 + 目录尾段：够区分，又不会把整条长路径当 key。
    const parts = p.replace(/\\/g, '/').split('/')
    return parts.slice(-2).join('/')
  }

  function loadPrefs() {
    if (!pathKey()) return
    try {
      const allW = JSON.parse(localStorage.getItem(WIDTHS_KEY) ?? '{}') as Record<string, number[]>
      const w = allW[pathKey()]
      if (Array.isArray(w)) {
        const m = new Map<number, number>()
        w.forEach((px, i) => {
          if (typeof px === 'number' && px > 0) m.set(i, px)
        })
        colWidths.value = m
      }
      const allF = JSON.parse(localStorage.getItem(FREEZE_KEY) ?? '{}') as Record<string, boolean>
      frozenFirst.value = allF[pathKey()] === true
    } catch {
      /* 存的东西坏了就当没存过，不该拦住打开文件 */
    }
  }

  function saveColWidths() {
    const key = pathKey()
    if (!key) return
    try {
      const allW = JSON.parse(localStorage.getItem(WIDTHS_KEY) ?? '{}') as Record<string, number[]>
      const width: number[] = []
      for (const [c, px] of colWidths.value) width[c] = px
      allW[key] = width
      localStorage.setItem(WIDTHS_KEY, JSON.stringify(allW))
    } catch {
      /* 超配额就放弃记忆，不影响当前会话 */
    }
  }

  function toggleFreeze() {
    frozenFirst.value = !frozenFirst.value
    const key = pathKey()
    if (!key) return
    try {
      const allF = JSON.parse(localStorage.getItem(FREEZE_KEY) ?? '{}') as Record<string, boolean>
      allF[key] = frozenFirst.value
      localStorage.setItem(FREEZE_KEY, JSON.stringify(allF))
    } catch {
      /* 同上 */
    }
  }

  /** 冻结首列的左偏移（像素）：粘在行号栏右侧。 */
  function frozenLeft(gutter: number): number {
    return gutter
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

  /** 逻辑总高度：含吸顶表头的占位，滚到底时最后一行才够得着。 */
  const logicalHeight = computed(() => displayRows.value * ROW_H + headerHeightOf(topRows()))
  /** 滚动条空间高度：压缩到上限之内。 */
  const scrollHeight = computed(() => Math.min(logicalHeight.value, MAX_SCROLL_PX))
  /**
   * 渲染窗口：一次算完，滚动坐标与窗口位置同源。
   * **只依赖四个标量**——不能在这里再读 `logicalScroll` / `maxLogical`：
   * 那两个是 grid 派生出来的，反向依赖会形成循环，每帧多算两轮。
   */
  const grid = computed(() =>
    computeGridWindow(displayRows.value, viewport.value, scrollTop.value, topRows()),
  )
  /** 内容坐标系的视口顶部（滚动条位置换算回来）。 */
  const logicalScroll = computed(() => grid.value.logicalScroll)
  /** 内容坐标系的滚动范围：`scrollToContent` 的钳制上界。 */
  const maxLogical = computed(() => Math.max(0, logicalHeight.value - viewport.value))
  /** 吸顶表头行数：覆盖优先，否则按有没有表头值行算。
   *  读的是本 composable 的 hasHeader，不是调用方的变量——
   *  setup 阶段 watch 会立即求值一次，去读调用方还没赋值的变量就是 TDZ 崩。 */
  function topRows(): number {
    return Math.max(0, opts.headerRows?.() ?? (hasHeader.value ? 2 : 1))
  }
  /** 吸顶表头占的高度（像素）。 */
  const headerOffset = computed(() => headerHeightOf(topRows()))
  /** 压缩比：滚动条像素 → 内容像素。 */
  const scale = computed(() => grid.value.scale)
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

  /**
   * 显示下标 d 处的原始行号。
   *
   * 没有删行 / 插行时显示下标与原始行号只差一个表头偏移——直接加，
   * 不做二分。二分是 O(log n)，而渲染窗口每帧对每行都要问一次，
   * 几十万行的表上「每帧几十次二分」正是拖动滚动条跟不上手的原因。
   */
  function originalRowAt(d: number): number {
    if (deletedRows.value.size === 0 && insertedRows.value.size === 0) {
      return d + dataStart.value
    }
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

  /**
   * 一行的字段值：页缓存 + 覆盖层。
   *
   * 没有改动、也没有缺列时**直接返回缓存里的那个数组**，不逐格复制：
   * 渲染窗口每帧要几十行 × 十几列，逐格新建数组在滚动时就是主要开销。
   * 缓存数组是只读的（取页时已补齐列），下游只读不写。
   */
  function cellsOf(orig: number): string[] {
    const pageIdx = Math.floor(orig / PAGE_SIZE)
    const page = pages.value.get(pageIdx)
    const base = page?.[orig - pageIdx * PAGE_SIZE]
    const edits = cellEdits.value.get(orig)
    if (!edits && base && base.length === totalCols.value) return base
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
      // 页缓存封顶：丢最久没用过的（LRU）。来回拖动时刚看过的页不会被误丢。
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

  // rAF 合帧：滚轮与滚动条事件一帧之内只应用最新状态。
  // 不合并的话触控板每秒几十次事件就是几十次全窗口 DOM patch，渲染跟不上拖动速度。
  let scrollRaf = 0
  let wheelRaf = 0
  let wheelAcc = 0

  function scheduleScroll(apply: () => void) {
    if (scrollRaf) return
    scrollRaf = requestAnimationFrame(() => {
      scrollRaf = 0
      apply()
    })
  }

  /** 滚轮：按行步进。压缩后像素步进不精确（3 格滚轮 = 几十行），所以自己算。 */
  function onWheel(e: WheelEvent) {
    if (!meta.value || displayRows.value <= 0) return
    e.preventDefault()
    const dir = e.deltaY > 0 ? 1 : e.deltaY < 0 ? -1 : 0
    if (dir === 0) return
    // 一帧内的多次滚轮先累积，帧边界一次应用：快速滚动不失真也不掉帧。
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

  /** 设到某个内容位置（含表头占位），并同步滚动条。 */
  function scrollToContent(target: number) {
    const clamped = Math.max(0, Math.min(target, maxLogical.value))
    const px = clamped * scale.value
    scrollTop.value = px
    const host = scrollHost.value
    if (host && Math.abs(host.scrollTop - px) > 0.5) {
      host.scrollTop = px
    }
  }

  /** `scrollToContent` 的别名：滚轮按内容像素累加，单位与它一致。 */
  const scrollToLogical = scrollToContent

  /** 滚到第几行（显示下标）：行顶贴着可视区顶部，不藏在吸顶表头下面。 */
  function scrollToDisplayRow(index: number) {
    const target = Math.max(0, Math.min(index, displayRows.value - 1))
    scrollToContent(contentYOf(target, topRows()))
  }

  /** 滚到某个原始行（显示下标由映射算出）。 */
  function scrollToOriginalRow(orig: number) {
    scrollToDisplayRow(displayIndexOf(orig))
  }

  /** 把某格滚进视口并尽量居中：查找跳转要的是「看到这一格」。 */
  function scrollToCell(orig: number, col: number) {
    // 纵向：目标行放在视口中间（行中心对准视口中心）。
    const d = displayIndexOf(orig)
    const center = contentYOf(d, topRows()) + ROW_H / 2 - effectiveViewport.value / 2
    scrollToContent(center)
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

  /** 滚动条被拖动：按比例换算回逻辑位置（rAF 合帧）。 */
  function onScroll() {
    scheduleScroll(() => {
      const host = scrollHost.value
      if (!host || !meta.value) return
      scrollTop.value = host.scrollTop
    })
  }

  onBeforeUnmount(() => {
    if (scrollRaf) cancelAnimationFrame(scrollRaf)
    if (wheelRaf) cancelAnimationFrame(wheelRaf)
  })

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
    progress.value = 0
    // 建索引要顺序扫一遍文件：GB 级表格几十秒都有可能，
    // 没有进度就是「打开就卡死」。事件按路径过滤，多个标签互不干扰。
    let off: UnlistenFn | null = null
    try {
      off = await listen<{ path: string; done: number; total: number }>(
        'table:progress',
        (ev) => {
          if (ev.payload.path !== path || ev.payload.total <= 0) return
          progress.value = Math.min(1, ev.payload.done / ev.payload.total)
        },
      )
    } catch {
      /* 浏览器预览里没有 Tauri 事件，进度条不出现即可 */
    }
    try {
      // 编码：`'auto'` 或未指定时交后端探测——前端预设 'utf-8' 会让探测永远不生效，
      // GB18030 的 CSV 会整表解成替换符。
      const enc = opts.encoding?.value
      const m = await api.table.open({
        path,
        encoding: enc && enc !== 'auto' ? enc : null,
        delimiter: opts.delimiter?.value ?? null,
        has_header: opts.hasHeader?.value ?? null,
      })
      meta.value = m
      pages.value = new Map()
      pageOrder.length = 0
      // 重建索引等于换了行基准：视图与撤销栈一起作废。
      view.value = null
      sortCol.value = null
      sortDesc.value = false
      filterText.value = ''
      filterCols.value = []
      cellEdits.value = new Map()
      deletedRows.value = new Set()
      insertedRows.value = new Map()
      colWidths.value = new Map()
      loadPrefs()
      // 后端按采样给的列宽（字符宽度 → 像素），没给就留空走默认档。
      autoWidths.value = (m.col_widths ?? []).map((w) =>
        w == null ? COL_W_DEFAULT : Math.min(COL_W_MAX, Math.max(COL_W_MIN, widthFromChars(w))),
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
      progress.value = null
      off?.()
    }
  }

  // ---- 视图：排序 / 筛选 ----

  /**
   * 重建视图。排序与筛选都走这一条：后端顺序扫一遍文件收顺序表，
   * 之后分页按字节区间取，所以「排序后翻到第 50 万行」与未排序时一样快。
   * 不传参数就用当前状态（取消排序时把列也清掉）。
   */
  async function applyView(
    next?: { sortCol?: number | null; sortDesc?: boolean; filterText?: string; filterCols?: number[] },
  ) {
    const path = opts.path.value
    if (!path) return
    if (next) {
      if (next.sortCol !== undefined) sortCol.value = next.sortCol
      if (next.sortDesc !== undefined) sortDesc.value = next.sortDesc
      if (next.filterText !== undefined) filterText.value = next.filterText
      if (next.filterCols !== undefined) filterCols.value = next.filterCols
    }
    const filtering = filterText.value.trim().length > 0
    const sorting = sortCol.value != null
    if (!filtering && !sorting) {
      await clearView()
      return
    }
    viewBusy.value = true
    error.value = ''
    try {
      const v = await api.table.view({
        path,
        sort_col: sortCol.value,
        sort_desc: sortDesc.value,
        filter_query: filtering ? filterText.value : null,
        filter_cols: filtering ? filterCols.value : [],
        match_case: false,
      })
      view.value = v
      // 视图换了行序：页缓存与覆盖层全部作废（行号已经不对应同一行内容）。
      pages.value = new Map()
      pageOrder.length = 0
      resetEdits()
      version.value += 1
      scrollToContent(0)
      ensureVisiblePages()
    } catch (e) {
      error.value = normalizeError(e).message
      opts.onError?.(error.value)
    } finally {
      viewBusy.value = false
    }
  }

  /** 取消排序 / 筛选，回到文件顺序。 */
  async function clearView() {
    if (!view.value && sortCol.value == null && !filterText.value.trim()) return
    const path = opts.path.value
    sortCol.value = null
    sortDesc.value = false
    filterText.value = ''
    filterCols.value = []
    view.value = null
    try {
      if (path) await api.table.clearView(path)
    } catch {
      /* 释放失败不影响使用：后端下次打开会重建 */
    }
    pages.value = new Map()
    pageOrder.length = 0
    version.value += 1
    scrollToContent(0)
    ensureVisiblePages()
  }

  /** 点列头：升序 → 降序 → 取消，三态循环。 */
  async function toggleSort(col: number) {
    if (sortCol.value !== col) {
      await applyView({ sortCol: col, sortDesc: false })
    } else if (!sortDesc.value) {
      await applyView({ sortCol: col, sortDesc: true })
    } else {
      await applyView({ sortCol: null, sortDesc: false })
    }
  }

  /** 按关键字筛选（全列）。 */
  async function setFilter(text: string) {
    await applyView({ filterText: text })
  }

  /** 该列的排序标记：`'asc'` / `'desc'` / `null`。 */
  function sortMark(col: number): 'asc' | 'desc' | null {
    if (sortCol.value !== col) return null
    return sortDesc.value ? 'desc' : 'asc'
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

  /** 改一个单元格（记一步撤销）。 */
  function setCell(orig: number, col: number, value: string) {
    const before = cellValue(orig, col)
    if (before === value) return
    applyCell(orig, col, value)
    pushUndo({
      forward: () => applyCell(orig, col, value),
      backward: () => applyCell(orig, col, before),
    })
  }

  /** 直接写覆盖层，不进撤销栈（撤销 / 重做自己走这里）。
   *  值回到基线（页缓存里那格的原值）就删掉覆盖项：否则撤销后读数还挂着「1 处未保存」。 */
  function applyCell(orig: number, col: number, value: string) {
    const base = baselineCell(orig, col)
    const next = new Map(cellEdits.value)
    const rowEdits = next.get(orig) ? new Map(next.get(orig)!) : new Map<number, string>()
    if (value === base) rowEdits.delete(col)
    else rowEdits.set(col, value)
    if (rowEdits.size === 0) next.delete(orig)
    else next.set(orig, rowEdits)
    cellEdits.value = next
    version.value += 1
  }

  /** 一格的基线值：页缓存里的原值（不含覆盖层）。 */
  function baselineCell(orig: number, col: number): string {
    const pageIdx = Math.floor(orig / PAGE_SIZE)
    const page = pages.value.get(pageIdx)
    return page?.[orig - pageIdx * PAGE_SIZE]?.[col] ?? ''
  }

  /** 某格是否被改过（脏格高亮）。 */
  function isDirtyCell(orig: number, col: number): boolean {
    return cellEdits.value.get(orig)?.has(col) ?? false
  }

  /** 删一行（记一步撤销）。 */
  function deleteRow(orig: number) {
    if (deletedRows.value.has(orig)) return
    applyDelete(orig, true)
    pushUndo({
      forward: () => applyDelete(orig, true),
      backward: () => applyDelete(orig, false),
    })
  }

  /** 设置删除标记，不进撤销栈。 */
  function applyDelete(orig: number, deleted: boolean) {
    const next = new Set(deletedRows.value)
    if (deleted) next.add(orig)
    else next.delete(orig)
    deletedRows.value = next
    version.value += 1
  }

  /** 在某行之后插入一条记录（记一步撤销）。 */
  function insertRowAfter(orig: number, values: string[]) {
    const at = applyInsert(orig, values)
    pushUndo({
      forward: () => void applyInsert(orig, values),
      backward: () => applyUninsert(orig, at),
    })
  }

  /** 插入并返回它在列表里的位置，撤销时按位置摘掉（同锚点可能插了多条）。 */
  function applyInsert(orig: number, values: string[]): number {
    const next = new Map(insertedRows.value)
    const list = next.get(orig) ? [...next.get(orig)!] : []
    list.push(values)
    next.set(orig, list)
    insertedRows.value = next
    version.value += 1
    return list.length - 1
  }

  function applyUninsert(orig: number, at: number) {
    const next = new Map(insertedRows.value)
    const list = next.get(orig)
    if (!list) return
    const copy = [...list]
    copy.splice(at, 1)
    if (copy.length === 0) next.delete(orig)
    else next.set(orig, copy)
    insertedRows.value = next
    version.value += 1
  }

  /** 撤销全部改动（同时清空撤销栈：这是「从头再来」，不该还能撤销回去）。 */
  function resetEdits() {
    cellEdits.value = new Map()
    deletedRows.value = new Set()
    insertedRows.value = new Map()
    undoStack.length = 0
    redoStack.length = 0
    canUndo.value = false
    canRedo.value = false
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
    progress,
    header,
    hasHeader,
    totalRows,
    viewDataRows,
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
    scrollToContent,
    headerOffset,
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
    // 视图与撤销
    view,
    viewBusy,
    sortCol,
    sortDesc,
    filterText,
    filterCols,
    applyView,
    clearView,
    toggleSort,
    setFilter,
    sortMark,
    frozenFirst,
    toggleFreeze,
    frozenLeft,
    undo,
    redo,
    canUndo,
    canRedo,
  }
}
