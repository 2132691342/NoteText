// 视觉实测：量出图标真实尺寸、字号分布、各层高度。
//
// 为什么要有这个：CSS 里的尺寸规则与实际渲染结果可以差很远——
// 写错一条选择器时，样式表看起来完全正常，渲染出来的图标却是撑满的。
// 肉眼判断不可靠，量出来才可信。
//
// 用法：
//   1) 浏览器带 --remote-debugging-port=9225 启动并打开应用页面
//   2) node scripts/inspect-visual.mjs [端口] [URL 片段]
import { connect } from './cdp.mjs'

const port = Number(process.argv[2] || 9225)
const urlMatch = process.argv[3] || '5173'

const { evaluate, close } = await connect({ port, urlMatch })

const report = await evaluate(`(() => {
  const out = {}
  const root = getComputedStyle(document.documentElement)
  out.theme = document.documentElement.getAttribute('data-theme')
  out.viewport = { w: window.innerWidth, h: window.innerHeight }
  out.scale = {
    fs: root.getPropertyValue('--nt-fs-scale').trim(),
    vw: root.getPropertyValue('--nt-vw-scale').trim(),
  }

  // 图标：按实际渲染宽度分档。图标尺寸写错时这里会立刻暴露。
  const icons = [...document.querySelectorAll('.nt-icon')]
  const sizes = {}
  for (const ic of icons) {
    const w = +ic.getBoundingClientRect().width.toFixed(2)
    sizes[w] = (sizes[w] || 0) + 1
  }
  out.icons = { total: icons.length, byWidth: sizes }
  // 预期：sm 14 / md 16 / lg 18，再乘 vw-scale。列出异常项。
  out.iconOffSpec = icons
    .map((ic) => {
      const r = ic.getBoundingClientRect()
      return { w: +r.width.toFixed(2), cls: ic.getAttribute('class') }
    })
    .filter((x) => ![13.66, 15.61, 17.58].includes(x.w))

  // 字号：叶子节点的文本元素。
  const fs = {}
  for (const el of document.querySelectorAll('*')) {
    if (!el.textContent || el.children.length > 0) continue
    const v = getComputedStyle(el).fontSize
    fs[v] = (fs[v] || 0) + 1
  }
  out.fontSizes = Object.entries(fs)
    .sort((a, b) => parseFloat(b[0]) - parseFloat(a[0]))
    .map(([size, n]) => ({ size, count: n }))

  // chrome 各层高度：与设计文档声明的刻度对齐。
  const layers = [
    ['.nt-titlebar', '标题栏'],
    ['.nt-menubar', '菜单栏'],
    ['.nt-toolbar', '工具栏'],
    ['.nt-tabbar', '标签栏'],
    ['.nt-statusbar', '状态栏'],
    ['.nt-sidebar-header', '侧栏头'],
  ]
  out.layers = layers.map(([sel, name]) => {
    const el = document.querySelector(sel)
    return el
      ? { name, h: +el.getBoundingClientRect().height.toFixed(1) }
      : { name, h: null }
  })

  // 浮窗：宽度 prop 曾因单位缺失而整体失效，单独量。
  const modal = document.querySelector('.nt-modal')
  out.modal = modal
    ? {
        w: +modal.getBoundingClientRect().width.toFixed(1),
        inline: modal.style.width,
      }
    : null

  return out
})()`)

if (report?.error) {
  console.error('页面求值失败：', report.error)
  close()
  process.exit(1)
}

console.log(JSON.stringify(report, null, 2))
close()
