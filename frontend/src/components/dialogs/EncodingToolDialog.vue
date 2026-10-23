// 编码转换浮窗：整体转码 + 行尾转换。
// 转码只改「怎么写」，不改编辑器里的文本：用户看到的一直是解码后的
<template>
  <NtModal :title="t('tool.encoding')" width="520" @close="emit('close')">
    <div class="nt-enc-tool">
      <div class="nt-field">
        <span class="nt-label">{{ t('tool.from') }}</span>
        <ElSelect v-model="from" size="small" filterable>
          <ElOption v-for="e in encodings" :key="e.id" :label="encodingLabel(e.id)" :value="e.id" />
        </ElSelect>
      </div>
      <div class="nt-field">
        <span class="nt-label">{{ t('tool.to') }}</span>
        <ElSelect v-model="to" size="small" filterable>
          <ElOption v-for="e in encodings" :key="e.id" :label="encodingLabel(e.id)" :value="e.id" />
        </ElSelect>
        <ElCheckbox v-model="writeBom">{{ t('tool.writeBom') }}</ElCheckbox>
      </div>

      <div class="nt-field">
        <span class="nt-label">{{ t('tool.convertLineEnding') }}</span>
        <ElRadioGroup v-model="ending" size="small">
          <ElRadioButton value="crlf">CRLF</ElRadioButton>
          <ElRadioButton value="lf">LF</ElRadioButton>
          <ElRadioButton value="cr">CR</ElRadioButton>
        </ElRadioGroup>
      </div>

      <div class="nt-enc-note">
        <p>{{ t('tool.encodingConvertHint') }}</p>
        <p v-if="uncertain" class="nt-enc-warn">
          {{ t('tool.encodingLowConfidence') }}
        </p>
      </div>

      <div class="nt-enc-actions">
        <ElButton size="small" @click="emit('close')">{{ t('common.cancel') }}</ElButton>
        <ElButton size="small" type="primary" :loading="running" @click="apply">
          {{ t('tool.convert') }}
        </ElButton>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElCheckbox, ElOption, ElRadioButton, ElRadioGroup, ElSelect } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { api, normalizeError } from '@/api'
import { getText, commitBaseline } from '@/composables/useEditorRegistry'
import { doReplaceDoc } from '@/composables/useEditorActions'
import { useEditorStore } from '@/stores/editor'
import { useToastStore } from '@/stores/toast'
import type { EncodingInfo } from '@/types/api'
import { encodingLabel } from '@/utils/encoding'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const editor = useEditorStore()
const toast = useToastStore()

const encodings = ref<EncodingInfo[]>([])
const from = ref('utf-8')
const to = ref('utf-8')
const writeBom = ref(false)
const ending = ref<'crlf' | 'lf' | 'cr'>('crlf')
const running = ref(false)
const uncertain = ref(false)

onMounted(async () => {
  encodings.value = await api.encodings()
  const tab = editor.active
  if (tab) {
    from.value = tab.encoding
    ending.value = tab.line_ending
    writeBom.value = tab.has_bom
    uncertain.value = tab.encoding_uncertain
  }
})

async function apply() {
  const tab = editor.active
  if (!tab?.path || tab.path.startsWith('draft:')) {
    toast.warn(t('toast.saveFirstForEncoding'))
    return
  }
  running.value = true
  try {
    // 行尾转换是文本层的事：先换编辑器里的文本（可撤销），再按新行尾写盘。
    if (ending.value !== tab.line_ending) {
      doReplaceDoc(tab.id, convertEnding(getText(tab.id), ending.value))
      tab.line_ending = ending.value
    }
    // 编码与 BOM 只在写盘时生效：用户看到的一直是解码后的 Unicode。
    tab.encoding = to.value
    tab.has_bom = writeBom.value
    await api.writeFile(
      tab.path,
      getText(tab.id),
      tab.encoding,
      tab.line_ending,
      tab.has_bom,
    )
    commitBaseline(tab.id)
    editor.markDirty(tab.id, false)
    toast.ok(t('toast.encodingChanged', { encoding: to.value }))
    emit('close')
  } catch (e) {
    toast.bad(`${t('error.encodeFailed', { reason: normalizeError(e).message })}`)
  } finally {
    running.value = false
  }
}

function convertEnding(text: string, target: 'crlf' | 'lf' | 'cr'): string {
  const lf = text.replace(/\r\n/g, '\n').replace(/\r/g, '\n')
  if (target === 'lf') return lf
  if (target === 'cr') return lf.replace(/\n/g, '\r')
  return lf.replace(/\n/g, '\r\n')
}
</script>

<style scoped>
.nt-enc-tool {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
}

.nt-enc-note {
  padding: var(--nt-sp-2) var(--nt-sp-3);
  background: var(--nt-surface-2);
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius-sm);
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-2);
}

.nt-enc-warn {
  margin-top: var(--nt-sp-2);
  color: var(--nt-warning);
}

.nt-enc-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--nt-sp-2);
}
</style>
