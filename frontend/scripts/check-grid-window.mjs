// 虚拟滚动窗口的验证：直接跑生产代码（gridScroll.ts），不复制一份算法。
//
// 用户看得见的坏法只有三种，每一种都是「不报错，只是少画」：
// 1. 视口量不到时窗口退化成几行（100 万行只显示 25 行）；
// 2. 窗口顶边被推出滚动容器（滚到中途一片空白）；
// 3. 吸顶表头把第一行盖住（打开就看不见首行），或滚到底时最后一行差一个表头够不着。
import {
  computeGridWindow,
  contentYOf,
  FALLBACK_VIEWPORT,
  MAX_SCROLL_PX,
  OVERSCAN,
  ROW_H,
} from '../src/composables/gridScroll.ts'

const cases = [
  ['百万行 · 视口 800 · 顶部', { rows: 1_000_000, viewport: 800, scrollTop: 0, header: 0 }],
  ['百万行 · 视口 0（故障态）', { rows: 1_000_000, viewport: 0, scrollTop: 0, header: 0 }],
  ['百万行 · 视口 800 · 一半', { rows: 1_000_000, viewport: 800, scrollTop: 10_000_000, header: 0 }],
  ['百万行 · 视口 800 · 底部', { rows: 1_000_000, viewport: 800, scrollTop: 19_999_999, header: 0 }],
  ['小表 100 行 · 顶部', { rows: 100, viewport: 800, scrollTop: 0, header: 0 }],
  ['小表 100 行 · 底部', { rows: 100, viewport: 800, scrollTop: 1800, header: 0 }],
  ['空表', { rows: 0, viewport: 800, scrollTop: 0, header: 0 }],
  // 两层表头（列字母 + 表头值）：49px。
  ['表头 2 行 · 顶部', { rows: 1_000_000, viewport: 800, scrollTop: 0, header: 2 }],
  ['表头 2 行 · 底部', { rows: 1_000_000, viewport: 800, scrollTop: 19_999_999, header: 2 }],
  ['表头 2 行 · 小表到底', { rows: 30, viewport: 800, scrollTop: 99999, header: 2 }],
  ['表头 1 行 · 顶部', { rows: 82_696, viewport: 623, scrollTop: 0, header: 1 }],
  ['表头 1 行 · 底部', { rows: 82_696, viewport: 623, scrollTop: 2_149_497, header: 1 }],
]

let failed = 0
/** 断言失败立刻记一笔，并打印能直接定位的现场。 */
function bad(msg) {
  console.log(`    ^ ${msg}`)
  failed += 1
}

for (const [label, c] of cases) {
  const w = computeGridWindow(c.rows, c.viewport, c.scrollTop, c.header)
  const inRange = w.windowTop >= -1 && w.windowTop <= w.scrollHeight
  console.log(
    [
      `${inRange ? 'ok ' : 'BAD'} ${label.padEnd(24)}`,
      `行数=${String(w.count).padStart(4)}`,
      `窗口顶=${String(Math.round(w.windowTop)).padStart(9)}`,
      `容器高=${String(Math.round(w.scrollHeight)).padStart(8)}`,
      `首可见行=${String(w.firstVisible).padStart(7)}`,
    ].join(' '),
  )

  if (!inRange) bad(`窗口被推到容器外：顶边必须落在 [0, ${w.scrollHeight}]`)

  // 窗口必须画满「一屏 + 上下 overscan」，除非到底后剩下的行不够那么多。
  const full = Math.ceil(FALLBACK_VIEWPORT / ROW_H) + OVERSCAN * 2 + 1
  const available = c.rows - w.windowStart
  const want = Math.min(available, full)
  if (c.rows > 0 && w.count < want) {
    bad(`只剩 ${w.count} 行：窗口退化（至少要 ${want} 行）`)
  }
  if (w.windowEnd > c.rows) bad(`窗口末行 ${w.windowEnd} 越过了总行数 ${c.rows}`)
  if (w.scrollHeight > MAX_SCROLL_PX) bad(`容器高 ${w.scrollHeight} 超过上限 ${MAX_SCROLL_PX}`)

  if (c.rows > 0 && c.viewport > 0) {
    // 打开时（scrollTop=0）第一行数据必须落在吸顶表头下方，且整行在视口内。
    const top = computeGridWindow(c.rows, c.viewport, 0, c.header)
    const firstY = top.windowTop + (0 - top.windowStart) * ROW_H
    if (top.windowStart > 0) bad('打开时窗口起点不是第 0 行')
    if (firstY < contentYOf(0, c.header) - 1) {
      bad(`首行被表头盖住：内容位置 ${Math.round(firstY)} < 表头底 ${contentYOf(0, c.header)}`)
    }
    if (firstY + ROW_H > c.viewport) bad(`首行超出视口：底边 ${Math.round(firstY + ROW_H)}`)

    // 滚动条推到底（scrollTop = 容器高 - 视口）：最后一行必须整行露在视口内。
    // windowTop 是容器坐标，视口底在容器里是 st + viewport。
    const st = Math.max(0, w.scrollHeight - c.viewport)
    const w2 = computeGridWindow(c.rows, c.viewport, st, c.header)
    const lastY = w2.windowTop + (c.rows - 1 - w2.windowStart) * ROW_H
    if (lastY + ROW_H > st + c.viewport + 1) {
      bad(`滚到底末行仍不可见：底边 ${Math.round(lastY + ROW_H)} > 视口底 ${Math.round(st + c.viewport)}`)
    }
    if (lastY < st + contentYOf(0, c.header)) {
      bad(`滚到底末行被表头盖住：容器位置 ${Math.round(lastY)}`)
    }
  }
}

// 比例映射的自洽性：同一内容位置在「滚动位置」与「窗口顶边」两处算出的容器位置必须相等。
for (const rows of [100, 10_000, 1_000_000]) {
  for (const header of [0, 1, 2]) {
    const viewport = 700
    const w = computeGridWindow(rows, viewport, 0, header)
    const st = Math.max(0, w.scrollHeight - viewport) / 2
    const mid = computeGridWindow(rows, viewport, st, header)
    // 首可见行的容器位置必须就是滚动位置（差不超过一行，取整造成）。
    const visibleTop = mid.windowTop + (mid.firstVisible - mid.windowStart) * ROW_H
    if (Math.abs(visibleTop - st) > ROW_H) {
      bad(`比例映射不自洽：rows=${rows} header=${header} 首可见行容器位置 ${Math.round(visibleTop)} ≠ 滚动位置 ${Math.round(st)}`)
    }
  }
}

if (failed) {
  console.log(`\nFAIL: ${failed} 项检查未通过`)
  process.exit(1)
}
console.log('\nOK: 窗口行数正常、始终落在滚动容器内、首行与末行都不被表头遮挡')
