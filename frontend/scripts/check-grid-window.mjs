// 虚拟滚动窗口的验证：直接跑生产代码（gridScroll.ts），不复制一份算法。
//
// 两件事必须成立，缺一个用户就看到「表里写着已经到底、屏幕上只有十几行」：
// 1. 视口量不到时窗口不能退化成几行；
// 2. 窗口顶边必须落在滚动容器的空间内（压缩坐标，不是逻辑坐标）。
import { computeGridWindow, FALLBACK_VIEWPORT, MAX_SCROLL_PX, OVERSCAN, ROW_H } from '../src/composables/gridScroll.ts'

const cases = [
  ['百万行 · 视口 800 · 顶部', { rows: 1_000_000, viewport: 800, scrollTop: 0 }],
  ['百万行 · 视口 0（故障态）', { rows: 1_000_000, viewport: 0, scrollTop: 0 }],
  ['百万行 · 视口 800 · 一半', { rows: 1_000_000, viewport: 800, scrollTop: 10_000_000 }],
  ['百万行 · 视口 800 · 底部', { rows: 1_000_000, viewport: 800, scrollTop: 19_999_999 }],
  ['小表 100 行 · 顶部', { rows: 100, viewport: 800, scrollTop: 0 }],
  ['小表 100 行 · 底部', { rows: 100, viewport: 800, scrollTop: 1800 }],
  ['空表', { rows: 0, viewport: 800, scrollTop: 0 }],
]

let failed = 0
for (const [label, c] of cases) {
  const w = computeGridWindow(c.rows, c.viewport, c.scrollTop)
  const inRange = w.windowTop >= -1 && w.windowTop <= w.scrollHeight
  const lines = [
    `${inRange ? 'ok ' : 'BAD'} ${label.padEnd(24)}`,
    `行数=${String(w.count).padStart(4)}`,
    `窗口顶=${String(Math.round(w.windowTop)).padStart(9)}`,
    `容器高=${String(Math.round(w.scrollHeight)).padStart(8)}`,
    `首可见行=${String(w.firstVisible).padStart(7)}`,
  ].join(' ')
  console.log(lines)

  if (!inRange) {
    console.log(`    ^ 窗口被推到容器外：顶边必须落在 [0, ${w.scrollHeight}]`)
    failed += 1
  }
  // 窗口必须画满「一屏 + 上下 overscan」，除非到底后剩下的行不够那么多。
  const full = Math.ceil(FALLBACK_VIEWPORT / ROW_H) + OVERSCAN * 2 + 1
  const available = c.rows - w.windowStart
  const want = Math.min(available, full)
  if (c.rows > 0 && w.count < want) {
    console.log(`    ^ 只剩 ${w.count} 行：窗口退化（至少要 ${want} 行）`)
    failed += 1
  }
  if (w.windowEnd > c.rows) {
    console.log(`    ^ 窗口末行 ${w.windowEnd} 越过了总行数 ${c.rows}`)
    failed += 1
  }
  if (w.scrollHeight > MAX_SCROLL_PX) {
    console.log(`    ^ 容器高 ${w.scrollHeight} 超过上限 ${MAX_SCROLL_PX}`)
    failed += 1
  }
}

if (failed) {
  console.log(`\nFAIL: ${failed} 项检查未通过`)
  process.exit(1)
}
console.log('\nOK: 窗口行数正常，且始终落在滚动容器内')
