// 转到行浮窗：输入行号跳过去。
// 行号上限取当前文档行数：输入超范围时按钮置灰，而不是跳过去
<template>
  <NtModal :title="t('dialog.gotoLine')" width="360" @close="emit('close')">
    <div class="nt-goto">
      <div class="nt-field">
        <span class="nt-label">{{ t('dialog.line') }}</span>
        <ElInput
          ref="input"
          v-model="line"
          size="small"
          type="number"
          :min="1"
          :max="maxLine"
          @keydown.enter="go"
        />
        <span class="nt-readout">/ {{ maxLine }}</span>
      </div>
      <div class="nt-goto-actions">
        <ElButton size="small" @click="emit('close')">{{ t('common.cancel') }}</ElButton>
        <ElButton size="small" type="primary" :disabled="!valid" @click="go">
          {{ t('dialog.goTo') }}
        </ElButton>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed, nextTick, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElInput } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { useEditorStore } from '@/stores/editor'
import { getText } from '@/composables/useEditorRegistry'
import { doGotoLine } from '@/composables/useEditorActions'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const editor = useEditorStore()

const input = ref<InstanceType<typeof ElInput> | null>(null)
const line = ref(1)

const maxLine = computed(() => {
  const tab = editor.active
  if (!tab) return 1
  const text = getText(tab.id)
  return text === '' ? 1 : text.split('\n').length
})

const valid = computed(() => line.value >= 1 && line.value <= maxLine.value)

onMounted(async () => {
  await nextTick()
  input.value?.focus()
  // 默认填当前光标行：多数情况是想从当前位置附近跳。
  const tab = editor.active
  if (tab) line.value = tab.position.line
})

function go() {
  if (!valid.value) return
  doGotoLine(editor.activeId, line.value)
  emit('close')
}
</script>

<style scoped>
.nt-goto {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-4);
}

.nt-goto-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--nt-sp-2);
}
</style>
