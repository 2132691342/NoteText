// 虚拟滚动的坐标系与窗口计算：大表格与工作簿网格共用。
// 空间压缩的理由：Chrome 的元素高度上限约 33,554,428px，1000 万行 × 26px = 2.6 亿 px，
// 直接拿内容高度当容器高度会让滚动条彻底失效。

/** 行高：与 CSS 的 --nt-sheet-row-h 必须同值。 */
export const ROW_H = 26
/** 滚动空间的上限：远低于 Chrome 的元素高度上限，留足余量。 */
export const MAX_SCROLL_PX = 20_000_000
/** 视口上下各多渲染几行，快速滚动不露白。 */
export const OVERSCAN = 12
/** 吸顶表头单行高度：与 CSS 的 .nt-sheet-cols 同值。 */
export const HEADER_ROW_H = 24
/** 吸顶表头底部分隔线。 */
export const HEADER_BORDER = 1
/**
 * 视口量不到时的兜底高度。
 * viewport 为 0 时不能只画 OVERSCAN 那几行——那正是「100 万行的表只显示 25 行」的样子。
 */
export const FALLBACK_VIEWPORT = 600

/** 吸顶表头占的高度（像素）：`rows` 层 × 行高 + 1px 分隔线；0 层给 0。 */
export function headerHeightOf(rows: number): number {
  return rows <= 0 ? 0 : rows * HEADER_ROW_H + HEADER_BORDER
}

/** 某一行（显示下标）在内容坐标系里的位置：表头在最前，数据行从它下面开始。 */
export function contentYOf(displayRow: number, headerRows = 0): number {
  return headerHeightOf(headerRows) + displayRow * ROW_H
}

/**
 * 一行行高在容器里的实际占用。
 * 行元素按自然高度渲染，只有绝对位移被压缩，所以这个值恒为 ROW_H。
 */
export const ROW_PX = ROW_H

/** 渲染窗口的一次计算结果。 */
export interface GridWindow {
  /** 窗口首行（含 overscan）。 */
  windowStart: number
  /** 窗口末行（不含）。 */
  windowEnd: number
  /** 窗口里的行数。 */
  count: number
  /** 窗口顶边的容器像素（直接给 translateY）。 */
  windowTop: number
  /** 视口顶部的显示行。 */
  firstVisible: number
  /** 视口顶部的内容位置（内容像素，含表头占位）。 */
  logicalScroll: number
  /** 内容像素 → 滚动条像素的压缩比（≤ 1；内容装得下时恰好为 1）。 */
  scale: number
  /** 滚动条空间高度（容器高度）。 */
  scrollHeight: number
  /** 吸顶表头高度（内容像素）。 */
  topOffset: number
}

/**
 * 算一次渲染窗口。三段坐标必须分清：
 *
 * | 坐标 | 含义 |
 * |---|---|
 * | 内容像素 | 表头 + 行 × ROW_H，与真实行号一一对应，可能上亿 |
 * | 滚动条像素 | 压到 MAX_SCROLL_PX 之内、浏览器真正滚动的那一层 |
 * | 容器像素 | 窗口层 translateY 的取值：内容位置减去「已滚过的内容」 |
 *
 * 三条不变量（写错任一条就是「只剩几行 / 前几行看不见 / 到底差一屏」）：
 * 1. 表头算进内容高度：滚到底时最后一行才不会被表头高度挤出视口；
 * 2. `scale` 用「可滚距离之比」而不是总高度之比，两端才同时对齐
 *    （滚动条到底对应的是 `scrollHeight - viewport`，不是 `scrollHeight`）；
 * 3. `windowTop` 把表头高度算进去：数据窗口是绝对定位的，从 y=0 起画会被
 *    不透明的 sticky 表头永久盖住。视口顶部落在表头里时首可见行仍是 0。
 */
export function computeGridWindow(
  displayRows: number,
  viewport: number,
  scrollTop: number,
  headerRows = 0,
): GridWindow {
  const topOffset = headerHeightOf(headerRows)
  const contentHeight = topOffset + displayRows * ROW_H
  const scrollHeight = Math.min(contentHeight, MAX_SCROLL_PX)
  // 两端的可滚距离之比：内容底对齐视口底，滚动条底也对应视口底。
  const logicalRange = Math.max(0, contentHeight - viewport)
  const scrollRange = Math.max(0, scrollHeight - viewport)
  const scale = logicalRange > 0 && scrollRange > 0 ? scrollRange / logicalRange : 0
  // 滚动位置钳到浏览器允许的最大值：越界的入参不该把窗口推到容器外。
  const st = viewport > 0 ? Math.min(Math.max(0, scrollTop), scrollRange) : Math.max(0, scrollTop)
  // 滚动条像素 → 内容像素。scale 为 0 表示内容一屏装得下，不滚动。
  const logicalScroll = scale > 0 ? Math.min(logicalRange, st / scale) : 0
  // 视口顶部落在表头里时（刚打开 / 滚回顶部），第一个数据行仍是 0。
  const firstRaw = Math.floor((logicalScroll - topOffset) / ROW_H)
  const lastRow = Math.max(0, displayRows - 1)
  const firstVisible = displayRows <= 0 ? 0 : Math.max(0, Math.min(lastRow, firstRaw))
  const effViewport = viewport > 0 ? viewport : FALLBACK_VIEWPORT
  const windowStart = Math.max(0, firstVisible - OVERSCAN)
  const visible = Math.ceil(effViewport / ROW_H) + OVERSCAN * 2 + 1
  const windowEnd = Math.min(displayRows, windowStart + visible)
  // 容器像素 = 滚动条位置 + 窗口首行相对视口顶的偏移（两者同为容器坐标）。
  const windowTop = st + topOffset + windowStart * ROW_H - logicalScroll
  return {
    windowStart,
    windowEnd,
    count: windowEnd - windowStart,
    windowTop,
    firstVisible,
    logicalScroll,
    scale,
    scrollHeight,
    topOffset,
  }
}
