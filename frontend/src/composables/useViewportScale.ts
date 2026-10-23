// 视口比例缩放：按「窗口宽 ÷ 1440」写 --nt-vw-scale，上下限 0.8~1.35。
// 于是 1280 笔记本与 3840 4K 屏上整套刻度等比跟随。缺省 1 保证 JS 没起来时也有可用尺寸。
import { onMounted, onUnmounted } from 'vue'

const BASE_WIDTH = 1440
const MIN = 0.8
const MAX = 1.35

export function useViewportScale() {
  let raf = 0

  function apply() {
    const w = window.innerWidth
    const scale = Math.min(MAX, Math.max(MIN, w / BASE_WIDTH))
    document.documentElement.style.setProperty('--nt-vw-scale', scale.toFixed(3))
  }

  function onResize() {
    if (raf) cancelAnimationFrame(raf)
    raf = requestAnimationFrame(apply)
  }

  onMounted(() => {
    apply()
    window.addEventListener('resize', onResize)
  })

  onUnmounted(() => {
    window.removeEventListener('resize', onResize)
    if (raf) cancelAnimationFrame(raf)
  })

  return { apply }
}
