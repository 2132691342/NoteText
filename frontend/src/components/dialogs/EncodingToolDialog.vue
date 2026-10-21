// 编码转换浮窗：整体转码 + 行尾转换。
//
// 转码只改「怎么写」，不改编辑器里的文本：用户看到的一直是解码后的
// Unicode，落盘时才按目标编码编码（service::file::write 的逻辑）。
<template>
  <NtModal :title="t('tool.encoding')" width="520" @close="emit('close')">
    <div class="nt-enc-tool">
      <div class="nt-enc-row">
        <span class="nt-label">{{ t('tool.from') }}</span>
        <ElSelect v-model="from" size="small" filterable>
          <ElOption v-for="e in encodings" :key="e.id" :label="e.label" :value="e.id" />
        </ElSelect>
      </div>
      <div class="nt-enc-row">
        <span class="nt-label">{{ t('tool.to') }}</span>
        <ElSelect v-model="to" size="small" filterable>
          <ElOption v-for="e in encodings" :key="e.id" :label="e.label" :value="e.id" />
        </ElSelect>
        <ElCheckbox v-model="writeBom">{{ t('tool.writeBom') }}</ElCheckbox>
      </div>

      <div class="nt-enc-row">
        <span class="nt-label">{{ t('tool.convertLineEnding') }}</span>
        <ElRadioGroup v-model="ending" size="small">
          <ElRadioButton value="crlf">CRLF</ElRadioButton>
          <ElRadioButton value="lf">LF</ElRadioButton>
          <ElRadioButton value="cr">CR</ElRadioButton>
        </ElRadioGroup>
      </div>

      <div class="nt-enc-note">
        <p>转换会立即写回当前文件。行尾与编码按上面的选择落盘，编辑器里的文本不变。</p>
        <p v-if="uncertain" class="nt-enc-warn">
          当前编码是探测出来的、置信度不高——先确认上面「源编码」选对了再转。
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
import { getText } from '@/composables/useEditorRegistry'
import { useEditorStore } from '@/stores/editor'
import { useToastStore } from '@/stores/toast'
import type { EncodingInfo } from '@/types/api'

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
  if (!tab?.path) {
    toast.warn('新建标签还没有路径，请先另存为')
    return
  }
  running.value = true
  try {
    // 行尾转换在编辑器侧做（纯文本操作），编码在写盘时做。
    if (ending.value !== tab.line_ending) {
      document.dispatchEvent(
        new CustomEvent('editor-replace-doc', {
          detail: { tabId: tab.id, text: convertEnding(getText(tab.id), ending.value) },
        }),
      )
    }
    // 写盘时带上目标编码：后端 service::file::write 按它编码。
    document.dispatchEvent(
      new CustomEvent('editor-save-as', {
        detail: { tabId: tab.id, encoding: to.value, bom: writeBom.value, line_ending: ending.value },
      }),
    )
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

.nt-enc-row {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
}

.nt-label {
  font-size: var(--nt-fs-label);
  font-weight: 600;
  color: var(--nt-ink-2);
  flex: none;
  min-width: 5em;
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
