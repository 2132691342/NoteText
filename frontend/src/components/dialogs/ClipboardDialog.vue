// 剪贴板历史浮窗：列出本次会话复制 / 剪切过的内容，点击回插。
// 只攒不监听系统剪贴板：Webview 里读系统剪贴板要权限，
<template>
  <NtModal :title="t('tool.clipboard')" width="620" @close="emit('close')">
    <div class="nt-clip">
      <div class="nt-clip-head">
        <span class="nt-readout">{{ entries.length }} {{ t('tool.clipCount') }}</span>
        <ElButton size="small" :disabled="!entries.length" @click="onClear">
          {{ t('common.clear') }}
        </ElButton>
      </div>

      <div v-if="entries.length === 0" class="nt-empty">
        <p>{{ t('tool.clipEmpty') }}</p>
        <p class="nt-empty-hint">{{ t('tool.clipHint') }}</p>
      </div>

      <div v-else class="nt-clip-list">
        <button
          v-for="(e, i) in entries"
          :key="e.at"
          class="nt-clip-row"
          @click="onInsert(e.text)"
        >
          <span class="nt-clip-idx">{{ i + 1 }}</span>
          <span class="nt-clip-text">{{ preview(e.text) }}</span>
        </button>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { useEditorStore } from '@/stores/editor'
import { useToastStore } from '@/stores/toast'
import { useClipboardHistory, clearClip } from '@/composables/useClipboardHistory'
import { doInsert } from '@/composables/useEditorActions'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const editor = useEditorStore()
const toast = useToastStore()

const { entries } = useClipboardHistory()

/** 列表里只显示首行：多行片段展开会把列表撑爆。 */
function preview(text: string): string {
  const first = text.split('\n')[0]
  return text.split('\n').length > 1 ? `${first} …` : first
}

const hasTab = computed(() => editor.activeId != null)

function onInsert(text: string) {
  if (!hasTab.value) {
    toast.bad(t('tool.clipNoEditor'))
    return
  }
  doInsert(editor.activeId, text)
  emit('close')
}

function onClear() {
  clearClip()
}
</script>

<style scoped>
.nt-clip {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
  min-height: 240px;
}

.nt-clip-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex: none;
}

.nt-clip-list {
  display: flex;
  flex-direction: column;
  gap: 1px;
  overflow: auto;
  min-height: 0;
}

.nt-clip-row {
  display: flex;
  align-items: flex-start;
  gap: var(--nt-sp-3);
  padding: var(--nt-sp-2) var(--nt-sp-3);
  border-radius: var(--nt-radius-sm);
  text-align: left;
  color: var(--nt-ink);
}

.nt-clip-row:hover {
  background: var(--nt-surface-hover);
}

.nt-clip-row:active {
  transform: scale(0.995);
}

.nt-clip-idx {
  flex: none;
  min-width: 2em;
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-3);
  font-variant-numeric: tabular-nums;
}

.nt-clip-text {
  font-family: var(--nt-font-editor);
  font-size: var(--nt-fs-sm);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
