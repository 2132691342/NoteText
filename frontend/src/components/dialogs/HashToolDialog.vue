// 哈希工具浮窗：算文本或文件的 MD5 / SHA-1 / SHA-256。
//
// 默认算当前编辑器内容；「算文件」走文件对话框。
// 结果给大写与小写两份：certutil 出大写、Get-FileHash 出大写，

<template>
  <NtModal :title="t('tool.hash')" width="560" @close="emit('close')">
    <div class="nt-hash-tool">
      <div class="nt-hash-row">
        <span class="nt-label">{{ t('tool.hashAlgo') }}</span>
        <ElRadioGroup v-model="algo" size="small" @change="run">
          <ElRadioButton value="md5">MD5</ElRadioButton>
          <ElRadioButton value="sha1">SHA-1</ElRadioButton>
          <ElRadioButton value="sha256">SHA-256</ElRadioButton>
        </ElRadioGroup>
      </div>

      <div class="nt-hash-row">
        <span class="nt-label">{{ t('tool.hashText') }}</span>
        <ElButton size="small" @click="pickFile">{{ t('tool.hashFile') }}</ElButton>
        <span v-if="filePath" class="nt-hash-file" :title="filePath">{{ filePath }}</span>
      </div>

      <div v-if="!filePath" class="nt-hash-input-wrap">
        <textarea v-model="text" class="nt-tool-io" spellcheck="false" rows="6" />
      </div>

      <div v-if="result.hex" class="nt-hash-out-wrap">
        <div class="nt-hash-out-head">
          <span class="nt-label">{{ t('common.result') }}</span>
          <span class="nt-readout">{{ result.bytes }} B</span>
        </div>
        <div class="nt-hash-out">{{ result.hex }}</div>
        <div class="nt-hash-out nt-hash-out-upper">{{ result.hex.toUpperCase() }}</div>
        <div class="nt-hash-actions">
          <ElButton size="small" @click="copy(result.hex)">{{ t('tool.copyHashLower') }}</ElButton>
          <ElButton size="small" @click="copy(result.hex.toUpperCase())">
            {{ t('tool.copyHashUpper') }}
          </ElButton>
        </div>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElRadioButton, ElRadioGroup } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { api, normalizeError } from '@/api'
import { useEditorStore } from '@/stores/editor'
import { getText } from '@/composables/useEditorRegistry'
import { useToastStore } from '@/stores/toast'
import { copyText } from '@/utils'
import type { HashVO } from '@/types/api'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const editor = useEditorStore()
const toast = useToastStore()

const algo = ref<'md5' | 'sha1' | 'sha256'>('sha256')
const text = ref('')
const filePath = ref('')
const result = ref<HashVO>({ ok: true, hex: '', algo: 'sha256', bytes: 0, error_message: null })

onMounted(() => {
  if (editor.activeId != null) text.value = getText(editor.activeId)
  void run()
})

async function run() {
  try {
    result.value = await api.hash({
      text: filePath.value ? undefined : text.value,
      path: filePath.value || undefined,
      algo: algo.value,
      upper: false,
    })
  } catch (e) {
    toast.bad(`${t('error.hashFailed', { reason: normalizeError(e).message })}`)
  }
}

async function pickFile() {
  const p = await api.openFileDialog()
  if (!p) return
  filePath.value = p
  text.value = ''
  await run()
}

async function copy(s: string) {
  if (await copyText(s)) toast.ok(t('common.copied'))
}
</script>

<style scoped>
.nt-hash-tool {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
}

.nt-hash-row {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
}

.nt-label {
  font-size: var(--nt-fs-label);
  font-weight: 600;
  color: var(--nt-ink-2);
  flex: none;
  min-width: 4em;
}

.nt-hash-file {
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-2);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.nt-hash-input-wrap {
  min-height: 0;
}

.nt-hash-out-wrap {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-2);
}

.nt-hash-out-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.nt-hash-out {
  font-family: var(--font-mono);
  font-size: var(--nt-fs-sm);
  padding: var(--nt-sp-2);
  background: var(--nt-surface-2);
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius-sm);
  word-break: break-all;
  user-select: text;
}

.nt-hash-out-upper {
  color: var(--nt-ink-2);
}

.nt-hash-actions {
  display: flex;
  gap: var(--nt-sp-2);
}
</style>
