// 查找结果面板：目录级查找的命中列表，点击跳到文件对应行。
// 从「浮窗里显示结果」搬到侧栏：目录级查找往往命中几十个文件，
<template>
  <div class="nt-panel">
    <div class="nt-panel-head">
      <span>{{ t('sidebar.findResults') }}</span>
      <button class="nt-icon-btn" :title="t('common.clear')" @click="clear">
        <NtIcon name="trash" size="sm" />
      </button>
    </div>

    <div v-if="!groups.length" class="nt-empty">
      <p>{{ t('sidebar.noFindResults') }}</p>
      <p class="nt-empty-hint">{{ t('sidebar.findResultsHint') }}</p>
    </div>

    <div v-else class="nt-panel-list">
      <div class="nt-fr-summary">{{ t('tool.batchHitSummary', { files: groups.length, count: total }) }}</div>
      <div v-for="g in groups" :key="g.path" class="nt-fr-group">
        <button class="nt-fr-file" @click="open(g.path)">
          <NtIcon name="file" />
          <span class="nt-fr-name">{{ fileName(g.path) }}</span>
          <span class="nt-fr-count">{{ g.lines.length }}</span>
        </button>
        <button
          v-for="ln in g.lines"
          :key="`${g.path}-${ln}`"
          class="nt-fr-line"
          @click="open(g.path, ln)"
        >
          <span class="nt-fr-ln">{{ ln }}</span>
        </button>
      </div>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import NtIcon from '@/components/common/NtIcon.vue'
import { useFindResultsStore } from '@/stores/findResults'
import { useEditorStore } from '@/stores/editor'
import { useToastStore } from '@/stores/toast'
import { baseName } from '@/utils'

const { t } = useI18n()
const store = useFindResultsStore()
const editor = useEditorStore()
const toast = useToastStore()

const groups = computed(() => store.groups)
const total = computed(() => store.total)

function fileName(p: string) {
  return baseName(p)
}

async function open(path: string, line?: number) {
  const tab = await editor.openPath(path)
  if (!tab) {
    toast.bad(t('toast.openFailed'))
    return
  }
  if (line) {
    // 跳行要等编辑器挂载完成：这里 nextTick 之后实例才注册。
    requestAnimationFrame(() => {
      document.dispatchEvent(
        new CustomEvent('editor-goto-line', { detail: { tabId: tab.id, line } }),
      )
    })
  }
}

function clear() {
  store.clear()
}
</script>

<style scoped>
.nt-fr-summary {
  padding: var(--nt-sp-2) var(--nt-sp-3);
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-2);
  flex: none;
}

.nt-fr-group {
  display: flex;
  flex-direction: column;
}

.nt-fr-file {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  padding: var(--nt-sp-1) var(--nt-sp-3);
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink);
  text-align: left;
}

.nt-fr-file:hover {
  background: var(--nt-surface-hover);
}

.nt-fr-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.nt-fr-count {
  flex: none;
  font-size: var(--nt-fs-hint);
  color: var(--nt-primary);
  font-variant-numeric: tabular-nums;
}

.nt-fr-line {
  display: flex;
  padding: 1px var(--nt-sp-3) 1px calc(var(--nt-sp-3) + var(--nt-sp-5));
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-2);
  text-align: left;
  font-family: var(--nt-font-editor);
}

.nt-fr-line:hover {
  background: var(--nt-surface-hover);
  color: var(--nt-ink);
}

.nt-fr-ln {
  font-variant-numeric: tabular-nums;
}
</style>
