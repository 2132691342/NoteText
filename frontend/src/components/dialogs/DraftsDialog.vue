// 草稿管理：未命名标签在退出 / 关闭时被收成草稿，这里把它们找回来。
// 为什么需要它：用户写了一半的东西没给文件名，直接丢就是数据丢失。
<template>
  <NtModal :title="t('dialog.drafts')" width="620" @close="dialogs.close()">
    <div class="nt-drafts">
      <div v-if="loading" class="nt-empty-sm">
        <span class="nt-spinner" />
      </div>
      <div v-else-if="!list.length" class="nt-empty-sm">{{ t('dialog.draftsEmpty') }}</div>
      <div v-else class="nt-drafts-list">
        <button
          v-for="d in list"
          :key="d.id"
          class="nt-draft-row"
          @click="open(d)"
        >
          <span class="nt-draft-title">{{ d.title || t('tab.untitled') }}</span>
          <span class="nt-readout nt-draft-time">{{ formatTime(d.updated_at) }}</span>
          <span class="nt-readout nt-draft-size">{{ d.content.length }}</span>
          <button
            class="nt-icon-btn"
            :title="t('common.delete')"
            @click.stop="remove(d.id)"
          >
            <NtIcon name="trash" size="sm" />
          </button>
        </button>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import NtModal from '@/components/common/NtModal.vue'
import NtIcon from '@/components/common/NtIcon.vue'
import { useDialogStore } from '@/stores/dialogs'
import { useEditorStore } from '@/stores/editor'
import { useToastStore } from '@/stores/toast'
import { api, normalizeError } from '@/api'
import type { DraftVO } from '@/types/api'
import { formatTime } from '@/utils'

const { t } = useI18n()
const dialogs = useDialogStore()
const editor = useEditorStore()
const toast = useToastStore()

const list = ref<DraftVO[]>([])
const loading = ref(true)

async function load() {
  loading.value = true
  try {
    // 空数组契约（AGENTS.md §2.3.2）：后端返回 null 时前端按空列表读。
    list.value = (await api.listDrafts()) ?? []
  } catch (e) {
    toast.bad(normalizeError(e).message)
    list.value = []
  } finally {
    loading.value = false
  }
}

onMounted(load)

/** 打开草稿：走 newTab(draftId)，路径前缀 draft: 让编辑器按草稿接口读正文。 */
function open(d: DraftVO) {
  editor.newTab({ draftId: d.id, title: d.title })
  dialogs.close()
}

async function remove(id: string) {
  try {
    await api.removeDraft(id)
    list.value = list.value.filter((d) => d.id !== id)
    toast.ok(t('toast.draftDeleted'))
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}
</script>

<style scoped>
.nt-drafts-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 420px;
  overflow-y: auto;
}

.nt-draft-row {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
  width: 100%;
  padding: var(--nt-sp-2) var(--nt-sp-3);
  border-radius: var(--nt-radius-xs);
  text-align: left;
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-2);
  transition:
    background-color var(--nt-dur-fast) var(--nt-ease),
    color var(--nt-dur-fast) var(--nt-ease);
}

.nt-draft-row:hover {
  background: var(--nt-surface-hover);
  color: var(--nt-ink);
}

.nt-draft-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.nt-draft-time,
.nt-draft-size {
  flex: none;
  font-variant-numeric: tabular-nums;
}
</style>
