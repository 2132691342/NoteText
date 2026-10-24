// 压缩任务面板：队列快照 + 暂停 / 继续 / 取消 / 清除。
// 数据全来自 archive store（事件驱动），这里只渲染，不自己拉状态。
<template>
  <NtModal :title="t('archive.tasksTitle')" width="640" @close="emit('close')">
    <div class="nt-atask">
      <div class="nt-atask-bar">
        <span class="nt-atask-count">
          {{ t('status.archive', { n: store.activeCount }) }}
        </span>
        <div class="nt-atask-spacer" />
        <ElButton size="small" :disabled="finishedCount === 0" @click="store.clearFinished()">
          {{ t('archive.clearDone') }}
        </ElButton>
      </div>

      <div v-if="store.tasks.length === 0" class="nt-atask-empty">
        {{ t('archive.taskEmpty') }}
      </div>

      <div v-else class="nt-atask-list">
        <div v-for="task in store.tasks" :key="task.id" class="nt-atask-item">
          <div class="nt-atask-line">
            <span class="nt-atask-kind">{{ t(`archive.taskKind.${task.kind}`) }}</span>
            <span class="nt-atask-title" :title="task.title">{{ task.title }}</span>
            <span class="nt-atask-status" :class="`is-${task.status}`">
              {{ t(`archive.status${task.status[0].toUpperCase()}${task.status.slice(1)}`) }}
            </span>
            <span class="nt-atask-spacer" />
            <template v-if="task.status === 'running' || task.status === 'paused'">
              <button v-if="task.status === 'running'" class="nt-icon-btn"
                :title="t('archive.pause')" @click="store.pause(task.id)">
                <NtIcon name="pause" size="sm" />
              </button>
              <button v-else class="nt-icon-btn" :title="t('archive.resume')" @click="store.resume(task.id)">
                <NtIcon name="play" size="sm" />
              </button>
            </template>
            <button v-if="isActive(task)" class="nt-icon-btn" :title="t('archive.cancelTask')"
              @click="store.cancel(task.id)">
              <NtIcon name="x" size="sm" />
            </button>
          </div>

          <ElProgress v-if="task.status === 'running'"
            :percentage="task.progress >= 0 ? Math.round(task.progress * 100) : 50"
            :indeterminate="task.progress < 0" :duration="2" :show-text="task.progress >= 0"
            :stroke-width="4" class="nt-atask-bar-el" />
          <ElProgress v-else-if="task.status === 'done'" :percentage="100" status="success"
            :stroke-width="4" :show-text="false" class="nt-atask-bar-el" />
          <ElProgress v-else-if="task.status === 'failed'" :percentage="100" status="exception"
            :stroke-width="4" :show-text="false" class="nt-atask-bar-el" />

          <div v-if="task.status === 'running' || task.status === 'paused'" class="nt-atask-meta">
            <span v-if="task.current" class="nt-atask-current">{{ task.current }}</span>
            <span v-if="task.speed">{{ formatSize(task.speed) }}/s</span>
            <span v-if="task.total > 0">
              {{ formatSize(task.processed) }} / {{ formatSize(task.total) }}
            </span>
          </div>
          <div v-if="task.error" class="nt-atask-error">{{ task.error }}</div>
        </div>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElProgress } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import NtIcon from '@/components/common/NtIcon.vue'
import { useArchiveStore } from '@/stores/archive'
import { formatSize } from '@/utils'
import type { ArchiveTaskVO } from '@/types/api'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const store = useArchiveStore()

const finishedCount = computed(
  () =>
    store.tasks.filter((t2) => t2.status === 'done' || t2.status === 'failed' || t2.status === 'cancelled')
      .length,
)

function isActive(task: ArchiveTaskVO): boolean {
  return task.status === 'queued' || task.status === 'running' || task.status === 'paused'
}
</script>

<style scoped>
.nt-atask {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
}

.nt-atask-bar {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
}

.nt-atask-count {
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-3);
  font-variant-numeric: tabular-nums;
}

.nt-atask-spacer {
  flex: 1;
}

.nt-atask-empty {
  padding: var(--nt-sp-10) 0;
  text-align: center;
  color: var(--nt-ink-muted);
  font-size: var(--nt-fs-sm);
}

.nt-atask-list {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-2);
  max-height: 50vh;
  overflow: auto;
}

.nt-atask-item {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-1);
  padding: var(--nt-sp-2) var(--nt-sp-3);
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius-sm);
}

.nt-atask-line {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
}

.nt-atask-kind {
  flex: none;
  padding: 0 var(--nt-sp-2);
  border-radius: var(--nt-radius-full);
  background: var(--nt-primary-soft);
  color: var(--nt-primary);
  font-size: var(--nt-fs-2xs);
  line-height: 18px;
}

.nt-atask-title {
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.nt-atask-status {
  flex: none;
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-3);
}

.nt-atask-status.is-done {
  color: var(--nt-success);
}

.nt-atask-status.is-failed {
  color: var(--nt-danger);
}

.nt-atask-status.is-cancelled {
  color: var(--nt-ink-muted);
}

.nt-atask-status.is-running {
  color: var(--nt-primary);
}

.nt-atask-bar-el {
  margin-top: 2px;
}

.nt-atask-meta {
  display: flex;
  gap: var(--nt-sp-3);
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-3);
  font-variant-numeric: tabular-nums;
}

.nt-atask-current {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.nt-atask-error {
  font-size: var(--nt-fs-hint);
  color: var(--nt-danger);
}
</style>
