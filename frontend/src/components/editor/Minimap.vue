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
import { docVersionOf } from '@/composables/useEditorRegistry'

const props = defineProps<{
  view: EditorView
  lineCount: number
  visible: boolean
  /** 所属标签 id：正文变化靠版本号感知，不必每帧把全文拷一次。 */
  tabId?: number
}>()

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
/** 上一次绘制时的正文档号：正文没变就不重画。 */
let drawnVersion = -1

/** 正文版本号：computed 让它成为绘制 watcher 的真依赖。 */
const version = computed(() => (props.tabId != null ? docVersionOf(props.tabId) : 0))

function draw() {
  const v = version.value
  if (v === drawnVersion) {
    syncViewport()
    return
  }
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

  // 按行迭代而不是 toString+split：几 MB 的文档每键全文拷一遍必卡输入。
  const doc = view.state.doc
  const step = ROW
  // 行数超过画布能画的下限时抽样：每 N 行画一根，形状仍然可信。
  const stride = Math.max(1, Math.ceil(doc.lines / Math.floor(h / step)))
  ctx.fillStyle = ink
  for (let i = 1; i <= doc.lines; i += stride) {
    const line = doc.line(i)
    // 超宽行（几 MB 的压缩 JSON）不能物化整行文本：截头采样，形状不变。
    const head = line.text.slice(0, 200)
    if (!head.trim()) continue
    const indent = head.length - head.trimStart().length
    const len = Math.min(head.trimEnd().length, w)
    const y = ((i - 1) / doc.lines) * h
    ctx.fillRect(indent * 0.5, y, Math.max(len * 0.5, 1), Math.max(step - 1, 1))
  }
  drawnVersion = v
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
  if (repaintTimer) window.clearTimeout(repaintTimer)
})

/**
 * 正文变化与行数变化都触发重画，但打字期间不重画：
 * 一次输入一个字符就把整篇轮廓重扫一遍，长文档上这笔钱纯属浪费。
 * 停顿 220ms 后画一次，缩略图跟得上视觉，也不抢输入的主线程。
 */
let repaintTimer = 0
function scheduleRepaint() {
  if (repaintTimer) window.clearTimeout(repaintTimer)
  repaintTimer = window.setTimeout(() => {
    repaintTimer = 0
    draw()
  }, 220)
}

watch([() => props.lineCount, version], () => scheduleRepaint())
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
