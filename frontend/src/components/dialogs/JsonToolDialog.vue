// JSON 工具浮窗：格式化 / 压缩 / 校验 / JSONPath / 转结构体。
// 打开即自动执行：九成场景是「看一眼排好版的 JSON」，不该再多点一次执行。
<template>
  <NtModal :title="t('tool.json')" width="760" @close="emit('close')">
    <div class="nt-tool">
      <div class="nt-tool-actions">
        <ElRadioGroup v-model="mode" size="small" @change="run">
          <ElRadioButton value="format">{{ t('tool.jsonFormat') }}</ElRadioButton>
          <ElRadioButton value="query">{{ t('tool.jsonQuery') }}</ElRadioButton>
          <ElRadioButton value="struct">{{ t('tool.jsonToStruct') }}</ElRadioButton>
        </ElRadioGroup>
        <div class="nt-tool-actions-right">
          <ElRadioGroup v-if="mode === 'format'" v-model="fmt" size="small" @change="run">
            <ElRadioButton value="pretty">{{ t('tool.pretty') }}</ElRadioButton>
            <ElRadioButton value="minify">{{ t('tool.minify') }}</ElRadioButton>
          </ElRadioGroup>
          <ElCheckbox v-if="mode === 'format' && fmt === 'pretty'" v-model="sortKeys">
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

      <!-- JSONPath：内联输入。不用原生 prompt——WebView2 里它被禁用，点了没反应。 -->
      <div v-if="mode === 'query'" class="nt-field">
        <span class="nt-label">JSONPath</span>
        <ElInput
          v-model="jsonPath"
          size="small"
          spellcheck="false"
          placeholder="$.store.book[0].title"
          @keydown.enter.prevent="run"
        />
      </div>
      <div v-if="mode === 'struct'" class="nt-field">
        <span class="nt-label">{{ t('tool.structLang') }}</span>
        <ElSelect v-model="structLang" size="small" class="nt-struct-lang" @change="run">
          <ElOption value="rust" label="Rust" />
          <ElOption value="go" label="Go" />
          <ElOption value="ts" label="TypeScript" />
          <ElOption value="java" label="Java" />
          <ElOption value="python" label="Python" />
        </ElSelect>
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
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { tr } from '@/i18n'
import {
  ElButton,
  ElCheckbox,
  ElInput,
  ElInputNumber,
  ElOption,
  ElRadioButton,
  ElRadioGroup,
  ElSelect,
} from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { api } from '@/api'
import { useDialogStore } from '@/stores/dialogs'
import { useEditorStore } from '@/stores/editor'
import { getText, get } from '@/composables/useEditorRegistry'
import { doReplaceRange } from '@/composables/useEditorActions'
import { useToastStore } from '@/stores/toast'
import type { JsonFormatVO, StructLang } from '@/types/api'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const dialogs = useDialogStore()
const editor = useEditorStore()
const toast = useToastStore()

const mode = ref<'format' | 'query' | 'struct'>('format')
const fmt = ref<'pretty' | 'minify'>('pretty')
const sortKeys = ref(false)
const indent = ref(2)
const jsonPath = ref('$')
const structLang = ref<StructLang>('rust')
const input = ref(dialogs.seedText)
const result = ref<JsonFormatVO>({ ok: true, output: '', error_line: null, error_col: null, error_message: null })

// 编辑器当前内容作为默认输入：用户没选中的时候也能直接用。
if (!input.value && editor.activeId != null) {
  input.value = getText(editor.activeId)
}

/** 应用目标：打开浮窗时的选区。选中文本进来的，应用要替换那段选区，
 *  而不是整篇——整篇替换会把选区外的内容一并冲掉。 */
const targetRange = ref<{ from: number; to: number } | null>(null)
{
  const id = editor.activeId
  const v = id != null ? get(id) : undefined
  if (v && dialogs.seedText) {
    const { from, to } = v.state.selection.main
    if (from < to) targetRange.value = { from, to }
  }
}

onMounted(() => {
  // 打开即执行：格式化是这里最高频的事，少点一次「执行」。
  if (input.value.trim()) void run()
})

async function run() {
  if (!input.value.trim()) return
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
      toast.warn(tr('error.jsonErrorAtLine', { reason: r.error_message ?? '', line: r.error_line }))
    }
    return
  }
  if (mode.value === 'query') {
    const r = await api.jsonQuery({ text: input.value, path: jsonPath.value || '$' })
    result.value = {
      ok: r.ok,
      output: r.ok ? JSON.stringify(r.matches, null, 2) : (r.error_message ?? ''),
      error_line: null,
      error_col: null,
      error_message: r.error_message,
    }
    return
  }
  const r = await api.jsonToStruct({ text: input.value, lang: structLang.value, root_name: 'Root' })
  result.value = {
    ok: r.ok,
    output: r.code,
    error_line: null,
    error_col: null,
    error_message: r.error_message,
  }
}

/** 写回编辑器：替换打开时的选区，没有选区则替换全文。 */
function writeBack() {
  const id = editor.activeId
  if (id == null) return
  const v = get(id)
  const range = targetRange.value
  if (range && v) {
    // 浮窗开着时正文可能已变：区间越界要夹回来。
    const len = v.state.doc.length
    doReplaceRange(id, Math.min(range.from, len), Math.min(range.to, len), result.value.output)
  } else {
    doReplaceRange(id, 0, v ? v.state.doc.length : 0, result.value.output)
  }
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

.nt-struct-lang {
  width: 140px;
}
</style>
