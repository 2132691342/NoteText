// 浮窗统一容器：遮罩 + 焦点圈 + Esc 关闭 + 滚动锁。
// 所有浮窗一律包它，不自写 Teleport + backdrop——每处写一遍，
<template>
  <Teleport to="body">
    <div class="nt-modal-mask" @click.self="emit('close')">
      <div
        ref="box"
        class="nt-modal"
        :style="{ width: boxWidth }"
        role="dialog"
        aria-modal="true"
        :aria-label="title"
        tabindex="-1"
      >
        <header class="nt-modal-head">
          <span class="nt-modal-title">{{ title }}</span>
          <button class="nt-icon-btn" @click="emit('close')">
            <NtIcon name="x" />
          </button>
        </header>
        <div class="nt-modal-body">
          <slot />
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script lang="ts" setup>
import { computed, onMounted, onBeforeUnmount, ref } from 'vue'
import NtIcon from '@/components/common/NtIcon.vue'

const props = defineProps<{ title: string; width?: number | string }>()
const emit = defineEmits<{ close: [] }>()

const box = ref<HTMLElement | null>(null)

/**
 * 宽度归一成带单位的 CSS 值。
 *
 * 模板里一律写 `width="640"`（字符串），直接丢给 style 就是无单位的
 * `640`——CSS 不认，浮窗会退化成内容宽。数字与纯数字字符串都要补 px。
 */
const boxWidth = computed(() => {
  const w = props.width
  if (w == null || w === '') return undefined
  if (typeof w === 'number') return `${w}px`
  return /^\d+(\.\d+)?$/.test(w.trim()) ? `${w.trim()}px` : w
})

/** 焦点圈：Tab 在浮窗内循环，不逃到编辑器。 */
function trapFocus(e: KeyboardEvent) {
  if (e.key !== 'Tab' || !box.value) return
  const focusables = box.value.querySelectorAll<HTMLElement>(
    'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
  )
  if (focusables.length === 0) return
  const first = focusables[0]
  const last = focusables[focusables.length - 1]
  if (e.shiftKey && document.activeElement === first) {
    e.preventDefault()
    last.focus()
  } else if (!e.shiftKey && document.activeElement === last) {
    e.preventDefault()
    first.focus()
  }
}

/**
 * Esc 关闭：监听在 document 上而不是弹窗元素上。
 *
 * 挂在元素上的 @keydown.esc 只在焦点还在弹窗内时有效；焦点一旦跑进
 * 内部的 ElInput（它的 input 是独立元素）或者用户点了别处，Esc 就失灵。
 * 浮窗是全屏模态，Esc 必须永远有效。
 */
function onEsc(e: KeyboardEvent) {
  if (e.key !== 'Escape') return
  e.preventDefault()
  e.stopPropagation()
  emit('close')
}

/** 滚动锁：浮窗打开时背后不滚动，否则滚轮会穿透到编辑器。 */
let prevOverflow = ''
onMounted(() => {
  prevOverflow = document.body.style.overflow
  document.body.style.overflow = 'hidden'
  box.value?.focus()
  document.addEventListener('keydown', trapFocus, true)
  // capture 阶段：先于编辑器与全局快捷键拿到 Esc。
  document.addEventListener('keydown', onEsc, true)
})

onBeforeUnmount(() => {
  document.body.style.overflow = prevOverflow
  document.removeEventListener('keydown', trapFocus, true)
  document.removeEventListener('keydown', onEsc, true)
})
</script>

<style scoped>
.nt-modal-mask {
  position: fixed;
  inset: 0;
  z-index: var(--nt-z-modal);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--nt-sp-6);
  background: var(--el-overlay-color-lighter, rgba(15, 23, 42, 0.28));
  /* 进：遮罩淡入；出由 useQuitGuard 不负责，浮窗是整块替换，无退场动画 */
  animation: nt-mask-in var(--nt-dur) var(--nt-ease);
}

.nt-modal {
  display: flex;
  flex-direction: column;
  max-width: 100%;
  max-height: 100%;
  background: var(--nt-surface);
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius-xl);
  box-shadow: var(--nt-card-shadow);
  outline: none;
  /* 从 96% 缩到 100%：轻微的「弹出来」而不是硬切 */
  animation: nt-modal-in var(--nt-dur-slow) var(--nt-ease);
}

@keyframes nt-mask-in {
  from {
    opacity: 0;
  }
}

@keyframes nt-modal-in {
  from {
    opacity: 0;
    transform: scale(0.965) translateY(-6px);
  }
}

.nt-modal-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: var(--nt-ctl-h-lg);
  padding: 0 var(--nt-sp-2) 0 var(--nt-sp-4);
  border-bottom: 1px solid var(--nt-border);
  flex: none;
}

.nt-modal-title {
  font-size: var(--nt-fs-md);
  font-weight: 600;
  color: var(--nt-ink);
}

.nt-modal-body {
  flex: 1;
  min-height: 0;
  padding: var(--nt-sp-4);
  overflow: auto;
}
</style>
