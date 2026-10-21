// 轻提示宿主：右下角堆叠，可选中复制，悬停不计时。
//
// 与 ElMessage 的分工见 stores/toast.ts：这里只负责渲染与交互。
<template>
  <Teleport to="body">
    <div class="nt-toast-host">
      <TransitionGroup name="nt-toast">
        <div
          v-for="item in toast.items"
          :key="item.id"
          class="nt-toast"
          :class="`is-${item.kind}`"
          @mouseenter="toast.hovered = item.id"
          @mouseleave="toast.hovered = null"
        >
          <NtIcon class="nt-toast-icon" :name="iconOf(item.kind)" />
          <span class="nt-toast-text">{{ item.text }}</span>
          <button class="nt-toast-close" @click="toast.dismiss(item.id)">
            <NtIcon name="x" />
          </button>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<script lang="ts" setup>
import { useToastStore, type ToastKind } from '@/stores/toast'
import NtIcon from '@/components/common/NtIcon.vue'

const toast = useToastStore()

function iconOf(kind: ToastKind): string {
  switch (kind) {
    case 'ok':
      return 'check'
    case 'warn':
      return 'alert'
    case 'bad':
      return 'alert'
    default:
      return 'info'
  }
}
</script>

<style scoped>
.nt-toast-host {
  position: fixed;
  right: var(--nt-sp-4);
  bottom: var(--nt-sp-4);
  z-index: var(--nt-z-toast);
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-2);
  pointer-events: none;
}

.nt-toast {
  display: flex;
  align-items: flex-start;
  gap: var(--nt-sp-2);
  max-width: 420px;
  padding: var(--nt-sp-2) var(--nt-sp-3);
  background: var(--nt-surface);
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius);
  box-shadow: var(--nt-card-shadow);
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink);
  pointer-events: auto;
  /* 错误信息常带错误码，要能整段选中复制给开发者 */
  user-select: text;
}

.nt-toast-icon {
  margin-top: 2px;
  flex: none;
}

.nt-toast.is-ok .nt-toast-icon {
  color: var(--nt-success);
}

.nt-toast.is-warn .nt-toast-icon {
  color: var(--nt-warning);
}

.nt-toast.is-bad .nt-toast-icon {
  color: var(--nt-danger);
}

.nt-toast-text {
  flex: 1;
  min-width: 0;
  word-break: break-word;
}

.nt-toast-close {
  display: inline-flex;
  padding: 2px;
  border-radius: var(--nt-radius-xs);
  color: var(--nt-ink-muted);
  flex: none;
}

.nt-toast-close:hover {
  background: var(--nt-surface-hover);
  color: var(--nt-ink);
}

/* 进出场：淡出 + 缩放 0.96，不做瞬时消失 */
.nt-toast-enter-active,
.nt-toast-leave-active {
  transition:
    opacity var(--nt-dur) var(--nt-ease),
    transform var(--nt-dur) var(--nt-ease);
}

.nt-toast-enter-from,
.nt-toast-leave-to {
  opacity: 0;
  transform: scale(0.96);
}

.nt-toast-move {
  transition: transform var(--nt-dur) var(--nt-ease);
}
</style>
