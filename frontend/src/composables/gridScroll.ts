// 虚拟滚动的坐标系与窗口计算：大表格与大表格工作簿共用。
// 四个坐标系：逻辑像素（displayRows × ROW_H）、滚动条像素（压缩后的空间）、
// 视口逻辑位置（scrollTop / scale）、显示下标（删除的行不占位，插入的行占位）。
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
 * `windowTop` 必须用滚动条（压缩）坐标：用逻辑坐标会把窗口推出容器外，滚到中途就什么也看不见。
 */
export function computeGridWindow(
  displayRows: number,
  viewport: number,
  scrollTop: number,
): GridWindow {
  const logicalHeight = displayRows * ROW_H
  const maxLogical = Math.max(0, logicalHeight - viewport)
  const scrollHeight = Math.min(logicalHeight, MAX_SCROLL_PX)
  // 压缩比：滚动条像素 → 逻辑像素。
  // 不能用 scrollHeight / logicalHeight：滚动条到底时对应的是 scrollHeight - viewport，那会让底部差一屏。
  const scale = maxLogical <= 0 ? 1 : (scrollHeight - viewport) / maxLogical
  const logicalScroll =
    maxLogical <= 0 ? 0 : Math.min(scrollTop / scale, maxLogical)
  const firstVisible = Math.floor(logicalScroll / ROW_H)
  const effViewport = viewport > 0 ? viewport : FALLBACK_VIEWPORT
  const windowStart = Math.max(0, firstVisible - OVERSCAN)
  const visible = Math.ceil(effViewport / ROW_H) + OVERSCAN * 2 + 1
  const windowEnd = Math.min(displayRows, windowStart + visible)
  const windowTop = scrollTop - (firstVisible - windowStart) * ROW_H
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
