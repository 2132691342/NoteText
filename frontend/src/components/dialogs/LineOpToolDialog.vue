// 行编辑浮窗：11 种行级变换，可预览后应用。
//
// 变换在后端算（service::editor::apply_line_op）：同样的逻辑
// 也服务「批量处理文件」，两边必须一致。
<template>
  <NtModal :title="t('tool.lineOp')" width="640" @close="emit('close')">
    <div class="nt-lineop">
      <div class="nt-lineop-list">
        <button
          v-for="item in LINE_OPS"
          :key="item.op"
          class="nt-row-item nt-lineop-item"
          :class="{ 'is-on': op === item.op }"
          @click="select(item.op)"
        >
          {{ t(item.labelKey) }}
        </button>
      </div>
      <div class="nt-lineop-preview">
        <div class="nt-tool-col-head">
          <span>{{ t('common.result') }}</span>
          <span class="nt-readout">
            {{ before }} → {{ after }} 行
          </span>
        </div>
        <pre class="nt-tool-io">{{ output }}</pre>
      </div>
      <div class="nt-lineop-actions">
        <ElCheckbox v-model="selectionOnly">{{ t('dialog.inSelection') }}</ElCheckbox>
        <div class="nt-lineop-actions-right">
          <ElButton size="small" @click="emit('close')">{{ t('common.cancel') }}</ElButton>
          <ElButton size="small" type="primary" @click="apply">{{ t('common.apply') }}</ElButton>
        </div>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElCheckbox } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { api, normalizeError } from '@/api'
import { LINE_OPS } from '@/stores/commands'
import { useEditorStore } from '@/stores/editor'
import { getText, getSelection, get } from '@/composables/useEditorRegistry'
import { doReplaceDoc, doReplaceRange } from '@/composables/useEditorActions'
import { useToastStore } from '@/stores/toast'
import type { LineOp } from '@/types/api'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const editor = useEditorStore()
const toast = useToastStore()

const op = ref<LineOp>('remove_blank')
const selectionOnly = ref(false)
const output = ref('')
const before = ref(0)
const after = ref(0)

onMounted(async () => {
  const tab = editor.active
  if (!tab) return
  const text = selectionOnly.value ? getSelection(tab.id) : getText(tab.id)
  before.value = text === '' ? 0 : text.split('\n').length
  await preview()
})

async function preview() {
  const tab = editor.active
  if (!tab) return
  const text = selectionOnly.value ? getSelection(tab.id) : getText(tab.id)
  try {
    output.value = await api.applyLineOp(op.value, text)
    after.value = output.value === '' ? 0 : output.value.split('\n').length
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}

function select(next: LineOp) {
  op.value = next
  void preview()
}

function apply() {
  const tab = editor.active
  if (!tab) return
  const v = get(tab.id)
  if (!v) return
  if (selectionOnly.value) {
    // 只换选区：用户勾了「仅选区」就该只动这一块。
    const { from, to } = v.state.selection.main
    if (from === to) {
      toast.warn(t('toast.columnNoSelection'))
      return
    }
    doReplaceRange(tab.id, from, to, output.value)
  } else {
    doReplaceDoc(tab.id, output.value)
  }
  emit('close')
}
</script>

<style scoped>
.nt-lineop {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
  height: 60vh;
  min-height: 0;
}

.nt-lineop-list {
  display: flex;
  flex-wrap: wrap;
  gap: var(--nt-sp-1);
  flex: none;
}

.nt-lineop-item {
  height: var(--nt-ctl-h-sm);
  padding: 0 var(--nt-sp-3);
  font-size: var(--nt-fs-sm);
  border: 1px solid var(--nt-border);
  color: var(--nt-ink-2);
}

.nt-lineop-preview {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

.nt-lineop-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex: none;
}

.nt-lineop-actions-right {
  display: flex;
  gap: var(--nt-sp-2);
}
</style>
