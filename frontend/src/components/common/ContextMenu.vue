// 通用右键菜单：把一个条目列表渲染成浮层。
//
// 为什么做成通用组件：标签栏、文件树、编辑器各自写过一套，三处的
// 间距 / 键盘可达性 / 点外部关闭行为都不一样。收敛到一处后
// 「右键菜单怎么关」只有一个答案。
<template>
  <Teleport to="body">
    <div ref="root" class="nt-menu" :style="style" @contextmenu.prevent>
      <template v-for="(item, i) in items" :key="i">
        <div v-if="item.sep" class="nt-menu-sep" />
        <button
          v-else
          class="nt-menu-entry"
          :class="{ 'is-danger': item.danger }"
          :disabled="item.disabled"
          @click="pick(item)"
        >
          <span class="nt-menu-entry-label">{{ item.label }}</span>
          <span v-if="item.hint" class="nt-menu-entry-key">{{ item.hint }}</span>
        </button>
      </template>
    </div>
  </Teleport>
</template>

<script lang="ts" setup>
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import type { MenuItem } from './contextMenu'

const props = withDefaults(
  defineProps<{
    x: number
    y: number
    items: MenuItem[]
    /** 最小宽度，px。 */
    minWidth?: number
  }>(),
  { minWidth: 180 },
)
const emit = defineEmits<{ close: [] }>()

const root = ref<HTMLElement | null>(null)
const pos = ref({ left: props.x, top: props.y })

/**
 * 贴边修正：菜单不能被窗口右/下边缘裁掉。
 *
 * 先按点击位置落一次，再量真实尺寸纠正——先估一个尺寸再定位，
 * 在「字体缩放到 140%」时估值会差很多，菜单会压到边缘外。
 */
watch(
  () => [props.x, props.y, props.items],
  async () => {
    pos.value = { left: props.x, top: props.y }
    await nextTick()
    const el = root.value
    if (!el) return
    const r = el.getBoundingClientRect()
    const gap = 6
    let left = props.x
    let top = props.y
    if (left + r.width > window.innerWidth - gap) {
      left = Math.max(gap, window.innerWidth - r.width - gap)
    }
    if (top + r.height > window.innerHeight - gap) {
      top = Math.max(gap, window.innerHeight - r.height - gap)
    }
    pos.value = { left, top }
  },
  { immediate: true, deep: true },
)

const style = computed(() => ({
  left: `${pos.value.left}px`,
  top: `${pos.value.top}px`,
  minWidth: `${props.minWidth}px`,
}))

/** 点外部 / Esc / 滚轮关闭：右键菜单的三种常规退场。 */
function onDocDown(e: MouseEvent) {
  if (!root.value?.contains(e.target as Node)) emit('close')
}
function onEsc(e: KeyboardEvent) {
  if (e.key !== 'Escape') return
  e.preventDefault()
  e.stopPropagation()
  emit('close')
}
function onWheel() {
  emit('close')
}

onMounted(() => {
  // mousedown 而不是 click：用户在别处按下鼠标时菜单就该消失，
  // 而不是等他在别处松开——那中间菜单还盖着内容。
  document.addEventListener('mousedown', onDocDown, true)
  document.addEventListener('keydown', onEsc, true)
  document.addEventListener('wheel', onWheel, { passive: true })
})
onUnmounted(() => {
  document.removeEventListener('mousedown', onDocDown, true)
  document.removeEventListener('keydown', onEsc, true)
  document.removeEventListener('wheel', onWheel)
})

function pick(item: MenuItem) {
  if (item.sep || item.disabled) return
  emit('close')
  item.run?.()
}
</script>
