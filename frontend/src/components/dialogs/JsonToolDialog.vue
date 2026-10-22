// JSON 工具浮窗：格式化 / 压缩 / 校验 / JSONPath / 转结构体。
//
// 输入默认带编辑器里选中的文本（dialog store 的 seedText），
// 输出可一键写回编辑器——「看到结果还要手动复制」是最烦的一步。
<template>
  <NtModal :title="t('tool.json')" width="760" @close="emit('close')">
    <div class="nt-tool">
      <div class="nt-tool-actions">
        <ElRadioGroup v-model="mode" size="small">
          <ElRadioButton value="format">{{ t('tool.jsonFormat') }}</ElRadioButton>
          <ElRadioButton value="query">{{ t('tool.jsonQuery') }}</ElRadioButton>
          <ElRadioButton value="struct">{{ t('tool.jsonToStruct') }}</ElRadioButton>
        </ElRadioGroup>
        <div class="nt-tool-actions-right">
          <ElCheckbox v-if="mode === 'format'" v-model="sortKeys">
            {{ t('tool.jsonSortKeys') }}
          </ElCheckbox>
          <ElInputNumber
            v-if="mode === 'format' && fmt === 'pretty'"
            v-model="indent"
            size="small"
            :min="0"
            :max="8"
            :title="t('tool.indent')"
            class="nt-indent"
          />
          <ElButton size="small" @click="run">{{ t('tool.run') }}</ElButton>
        </div>
      </div>

      <div class="nt-tool-grid">
        <div class="nt-tool-col">
          <div class="nt-tool-col-head">{{ t('tool.regexText') }}</div>
          <textarea v-model="input" class="nt-tool-io" spellcheck="false" />
        </div>
        <div class="nt-tool-col">
          <div class="nt-tool-col-head">
            <span>{{ t('common.result') }}</span>
            <ElButton v-if="result.ok && result.output" size="small" text @click="writeBack">
              {{ t('common.apply') }}
            </ElButton>
          </div>
          <pre class="nt-tool-io" :class="{ 'is-error': !result.ok }">{{ result.output }}</pre>
        </div>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElCheckbox, ElInputNumber, ElRadioButton, ElRadioGroup } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { api } from '@/api'
import { useDialogStore } from '@/stores/dialogs'
import { useEditorStore } from '@/stores/editor'
import { getText } from '@/composables/useEditorRegistry'
import { doReplaceDoc } from '@/composables/useEditorActions'
import { useToastStore } from '@/stores/toast'
import type { JsonFormatVO } from '@/types/api'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const dialogs = useDialogStore()
const editor = useEditorStore()
const toast = useToastStore()

const mode = ref<'format' | 'query' | 'struct'>('format')
const fmt = ref<'pretty' | 'minify'>('pretty')
const sortKeys = ref(false)
const indent = ref(2)
const input = ref(dialogs.seedText)
const result = ref<JsonFormatVO>({ ok: true, output: '', error_line: null, error_col: null, error_message: null })

// 编辑器当前内容作为默认输入：用户没选中的时候也能直接用。
if (!input.value && editor.activeId != null) {
  input.value = getText(editor.activeId)
}

async function run() {
  if (mode.value === 'format') {
    const r = await api.jsonFormat({
      text: input.value,
      format: fmt.value,
      indent: indent.value,
      sort_keys: sortKeys.value,
    })
    result.value = r
    if (!r.ok && r.error_line) {
      // 定位到出错行列：把「哪一行」标出来，用户不用自己数。
      toast.warn(`${t('error.jsonInvalid', { reason: r.error_message ?? '' })}（第 ${r.error_line} 行）`)
    }
    return
  }
  if (mode.value === 'query') {
    const r = await api.jsonQuery({ text: input.value, path: prompt('JSONPath', '$') ?? '$' })
    result.value = {
      ok: r.ok,
      output: r.ok ? JSON.stringify(r.matches, null, 2) : (r.error_message ?? ''),
      error_line: null,
      error_col: null,
      error_message: r.error_message,
    }
    return
  }
  const lang = (prompt('目标语言（go/ts/java/python/rust）', 'rust') ?? 'rust') as never
  const r = await api.jsonToStruct({ text: input.value, lang, root_name: 'Root' })
  result.value = {
    ok: r.ok,
    output: r.code,
    error_line: null,
    error_col: null,
    error_message: r.error_message,
  }
}

/** 写回编辑器：替换当前标签全文（或选区）。 */
function writeBack() {
  const id = editor.activeId
  if (id == null) return
  doReplaceDoc(id, result.value.output)
  emit('close')
}
</script>

<style scoped>
.nt-tool {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
  height: 100%;
  min-height: 0;
}

.nt-tool-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--nt-sp-3);
  flex: none;
}

.nt-tool-actions-right {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
}

.nt-indent {
  width: 88px;
}
</style>
