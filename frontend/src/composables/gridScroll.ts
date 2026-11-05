// 虚拟滚动的坐标系与窗口计算：大表格与大表格工作簿共用（逻辑像素 / 压缩滚动条像素 / 显示下标）。
// 空间压缩的理由：Chrome 的元素高度上限约 33,554,428px，1000 万行 × 26px = 2.6 亿 px。

/** 行高：与 CSS 的 --nt-sheet-row-h 必须同值。 */
export const ROW_H = 26
/** 滚动空间的上限：远低于 Chrome 的元素高度上限，留足余量。 */
export const MAX_SCROLL_PX = 20_000_000
/** 视口上下各多渲染几行，快速滚动不露白。 */
export const OVERSCAN = 12
/**
 * 视口量不到时的兜底高度。
 * viewport 为 0 时不能只画 OVERSCAN 那几行——那正是「100 万行的表只显示 25 行」的样子。
 */
export const FALLBACK_VIEWPORT = 600

/** 渲染窗口的一次计算结果。 */
export interface GridWindow {
  /** 窗口首行（含 overscan）。 */
  windowStart: number
  /** 窗口末行（不含）。 */
  windowEnd: number
  /** 窗口里的行数。 */
  count: number
  /** 窗口顶边（滚动条空间坐标）。 */
  windowTop: number
  /** 视口顶部的显示行。 */
  firstVisible: number
  /** 视口顶部的逻辑像素位置。 */
  logicalScroll: number
  /** 滚动条像素 → 逻辑像素的换算比。 */
  scale: number
  /** 滚动条空间高度。 */
  scrollHeight: number
}

/**
 * 算一次渲染窗口。
 * `topOffset` 是吸顶表头的高度：表头占据滚动内容顶部，数据行从它下面开始，
 * 不算上它，前几行数据会被不透明的 sticky 表头盖住（scrollTop=0 时整片不可见）。
 * `windowTop` 必须用滚动条（压缩）坐标：用逻辑坐标会把窗口推出容器外，滚到中途就什么也看不见。
 */
export function computeGridWindow(
  displayRows: number,
  viewport: number,
  scrollTop: number,
  topOffset = 0,
): GridWindow {
  // 表头也算进逻辑高度与滚动空间：否则滚到底时最后几行差一个表头的高度够不着。
  const logicalHeight = displayRows * ROW_H + topOffset
  const maxLogical = Math.max(0, logicalHeight - viewport)
  const scrollHeight = Math.min(logicalHeight, MAX_SCROLL_PX)
  // 压缩比：滚动条像素 → 逻辑像素。
  // 不能用 scrollHeight / logicalHeight：滚动条到底时对应的是 scrollHeight - viewport，那会让底部差一屏。
  const scale = maxLogical <= 0 ? 1 : (scrollHeight - viewport) / maxLogical
  // 滚动位置钳到浏览器允许的最大值：越界的入参不该把窗口推出容器外。
  const st =
    viewport > 0 ? Math.min(scrollTop, Math.max(0, scrollHeight - viewport)) : scrollTop
  const logicalScroll =
    maxLogical <= 0 ? 0 : Math.min(st / scale, maxLogical)
  // 数据行从表头下面开始：视口顶部落在表头里时第一可见行还是 0。
  const firstVisible = Math.max(0, Math.floor((logicalScroll - topOffset) / ROW_H))
  const effViewport = viewport > 0 ? viewport : FALLBACK_VIEWPORT
  const windowStart = Math.max(0, firstVisible - OVERSCAN)
  const visible = Math.ceil(effViewport / ROW_H) + OVERSCAN * 2 + 1
  const windowEnd = Math.min(displayRows, windowStart + visible)
  const windowTop = st - (firstVisible - windowStart) * ROW_H
  return {
    windowStart,
    windowEnd,
    count: windowEnd - windowStart,
    windowTop,
    firstVisible,
    logicalScroll,
    scale: scale > 0 ? scale : 1,
    scrollHeight,
  }
}
