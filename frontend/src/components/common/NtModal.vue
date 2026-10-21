// 浮窗统一容器：遮罩 + 焦点圈 + Esc 关闭 + 滚动锁。
//
// 所有浮窗一律包它，不自写 Teleport + backdrop——每处写一遍，
// 就会出现「有的浮窗 Esc 能关、有的不能」这种不一致。
<template>
  <Teleport to="body">
    <div class="nt-modal-mask" @click.self="emit('close')">
      <div
        ref="box"
        class="nt-modal"
        :style="{ width: typeof width === 'number' ? `${width}px` : width }"
        role="dialog"
        aria-modal="true"
        :aria-label="title"
        tabindex="-1"
        @keydown.esc.prevent="emit('close')"
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
import { onMounted, onBeforeUnmount, ref } from 'vue'
import NtIcon from '@/components/common/NtIcon.vue'

defineProps<{ title: string; width?: number | string }>()
const emit = defineEmits<{ close: [] }>()

const box = ref<HTMLElement | null>(null)

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

/** 滚动锁：浮窗打开时背后不滚动，否则滚轮会穿透到编辑器。 */
let prevOverflow = ''
onMounted(() => {
  prevOverflow = document.body.style.overflow
  document.body.style.overflow = 'hidden'
  box.value?.focus()
  document.addEventListener('keydown', trapFocus, true)
})

onBeforeUnmount(() => {
  document.body.style.overflow = prevOverflow
  document.removeEventListener('keydown', trapFocus, true)
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
