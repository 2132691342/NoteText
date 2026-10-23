// Minimap：整篇文档的缩略轮廓，点击跳转、滚动跟随。
// 只画每行的缩进与长度，不渲染文字：几万行的日志用 canvas 画字必掉帧。
<template>
  <div class="nt-minimap" @click="onJump" @mousedown.prevent>
    <canvas ref="cv" class="nt-minimap-canvas" />
    <div class="nt-minimap-view" :style="viewStyle" />
  </div>
</template>

<script lang="ts" setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import type { EditorView } from '@codemirror/view'

const props = defineProps<{ view: EditorView; lineCount: number; visible: boolean }>()

const cv = ref<HTMLCanvasElement | null>(null)
/** 视口高亮：top/height 用比例，避免与像素换算耦合。 */
const viewTop = ref(0)
const viewHeight = ref(0.2)

/** 每行像素高度：比编辑器的行高小一个量级，缩略才放得下。 */
const ROW = 2

const viewStyle = computed(() => ({
  top: `${viewTop.value * 100}%`,
  height: `${Math.max(viewHeight.value, 0.02) * 100}%`,
}))

let raf = 0

function draw() {
  if (!props.visible) return
  const canvas = cv.value
  const view = props.view
  if (!canvas || !view) return
  const dpr = window.devicePixelRatio || 1
  const w = canvas.clientWidth
  const h = canvas.clientHeight
  if (w === 0 || h === 0) return
  canvas.width = Math.floor(w * dpr)
  canvas.height = Math.floor(h * dpr)
  const ctx = canvas.getContext('2d')
  if (!ctx) return
  ctx.scale(dpr, dpr)
  ctx.clearRect(0, 0, w, h)

  // 色值走令牌：CSS 变量里取，不在 canvas 里写死 hex。
  const css = getComputedStyle(document.documentElement)
  const ink = css.getPropertyValue('--nt-ink-3').trim()

  const text = view.state.doc.toString()
  const lines = text.split('\n')
  const step = ROW
  // 行数超过画布能画的下限时抽样：每 N 行画一根，形状仍然可信。
  const stride = Math.max(1, Math.ceil(lines.length / Math.floor(h / step)))
  ctx.fillStyle = ink
  for (let i = 0; i < lines.length; i += stride) {
    const line = lines[i]
    if (!line.trim()) continue
    const indent = line.length - line.trimStart().length
    const len = Math.min(line.trimEnd().length, w)
    const y = (i / lines.length) * h
    ctx.fillRect(indent * 0.5, y, Math.max(len * 0.5, 1), Math.max(step - 1, 1))
  }
  syncViewport()
}

/** 视口矩形：跟编辑器的滚动位置走。 */
function syncViewport() {
  const view = props.view
  if (!view) return
  const el = view.scrollDOM
  const max = el.scrollHeight - el.clientHeight
  viewTop.value = max > 0 ? el.scrollTop / el.scrollHeight : 0
  viewHeight.value = el.clientHeight / el.scrollHeight
}

/** 滚动时重画：rAF 节流，一帧最多一次。 */
function schedule() {
  if (raf) return
  raf = requestAnimationFrame(() => {
    raf = 0
    syncViewport()
  })
}

let scrollTarget: HTMLElement | null = null

onMounted(() => {
  draw()
  scrollTarget = props.view?.scrollDOM ?? null
  scrollTarget?.addEventListener('scroll', schedule, { passive: true })
})

onBeforeUnmount(() => {
  scrollTarget?.removeEventListener('scroll', schedule)
  if (raf) cancelAnimationFrame(raf)
})

watch(() => props.lineCount, () => draw())
watch(() => props.visible, (v) => v && draw())

/** 点击跳转：按点击位置的比例换算成目标滚动位置。 */
function onJump(e: MouseEvent) {
  const view = props.view
  if (!view) return
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
  const ratio = (e.clientY - rect.top) / rect.height
  const el = view.scrollDOM
  el.scrollTop = ratio * (el.scrollHeight - el.clientHeight)
}
</script>

<style scoped>
.nt-minimap {
  position: relative;
  width: 72px;
  flex: none;
  background: var(--nt-editor-gutter-bg);
  border-left: 1px solid var(--nt-border);
  cursor: pointer;
  overflow: hidden;
}

.nt-minimap-canvas {
  display: block;
  width: 100%;
  height: 100%;
}

/* 视口指示：一层极轻的覆盖，不抢正文注意力 */
.nt-minimap-view {
  position: absolute;
  left: 0;
  right: 0;
  background: var(--nt-primary);
  opacity: 0.12;
  pointer-events: none;
}
</style>
