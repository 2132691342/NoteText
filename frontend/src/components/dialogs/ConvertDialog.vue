// 格式转换浮窗：JSON / YAML / TOML / XML 互转。
// 为什么在前端做而不是后端：这四种格式在配置文件场景里都是小文档

<template>
  <NtModal :title="t('convert.title')" width="720" @close="dialogs.close()">
    <div class="nt-conv">
      <div class="nt-field nt-conv-bar">
        <span class="nt-label">{{ t('convert.from') }}</span>
        <ElSelect v-model="from" size="small" class="nt-conv-sel">
          <ElOption v-for="f in FORMATS" :key="f.id" :value="f.id" :label="f.label" />
        </ElSelect>
        <ElButton size="small" class="nt-conv-swap" @click="swap">{{ t('convert.swap') }}</ElButton>
        <span class="nt-label">{{ t('convert.to') }}</span>
        <ElSelect v-model="to" size="small" class="nt-conv-sel">
          <ElOption v-for="f in FORMATS" :key="f.id" :value="f.id" :label="f.label" />
        </ElSelect>
        <div class="nt-conv-spacer" />
        <ElButton size="small" :loading="loading" type="primary" @click="run">
          {{ t('convert.convert') }}
        </ElButton>
      </div>

      <div class="nt-conv-cols">
        <div class="nt-conv-col">
          <div class="nt-tool-col-head">
            <span>{{ t('convert.input') }}</span>
            <button class="nt-text-btn nt-conv-load" @click="loadFromEditor">
              {{ t('convert.loadFromEditor') }}
            </button>
          </div>
          <textarea
            v-model="input"
            class="nt-tool-io"
            spellcheck="false"
            :placeholder="t('convert.emptyInput')"
          />
        </div>
        <div class="nt-conv-col">
          <div class="nt-tool-col-head">
            <span>{{ t('convert.output') }}</span>
            <div class="nt-conv-out-actions">
              <button class="nt-text-btn" @click="copyOut">{{ t('convert.copyResult') }}</button>
              <!-- 应用：有选区替换选区，没选区替换全文。转完还要手动
                   复制粘贴回去，那是把「一步的事」拆成三步。 -->
              <button v-if="output" class="nt-text-btn" @click="applyToEditor">
                {{ t('common.apply') }}
              </button>
            </div>
          </div>
          <pre class="nt-tool-io nt-conv-out">{{ output }}</pre>
        </div>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElOption, ElSelect } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { useDialogStore } from '@/stores/dialogs'
import { useEditorStore } from '@/stores/editor'
import { useToastStore } from '@/stores/toast'
import { getText, get } from '@/composables/useEditorRegistry'
import { doReplaceRange } from '@/composables/useEditorActions'
import { convertText, type ConvertFormat } from '@/utils/formatConvert'
import { copyText } from '@/utils'

const { t } = useI18n()
const dialogs = useDialogStore()
const editor = useEditorStore()
const toast = useToastStore()

const FORMATS = computed(() => [
  { id: 'json' as ConvertFormat, label: t('convert.json') },
  { id: 'yaml' as ConvertFormat, label: t('convert.yaml') },
  { id: 'toml' as ConvertFormat, label: t('convert.toml') },
  { id: 'xml' as ConvertFormat, label: t('convert.xml') },
])

const from = ref<ConvertFormat>('json')
const to = ref<ConvertFormat>('yaml')
const input = ref(dialogs.seedText)
const output = ref('')
const loading = ref(false)

/** 应用目标：打开浮窗时的选区（右键「更多转换…」带着选中文本进来）。 */
const targetRange = ref<{ from: number; to: number } | null>(null)

onMounted(() => {
  // 右键 / 快操作带选中文本进来：直接载入并转一次，进来就是结果。
  if (input.value) {
    void run()
    return
  }
  const id = editor.activeId
  if (id == null) return
  const v = get(id)
  if (v && getText(id).length > 0) {
    const { from: f, to } = v.state.selection.main
    if (f < to) targetRange.value = { from: f, to }
    input.value = getText(id)
    void run()
  }
})

/** 从当前标签载入正文：转配置文件的场景里「正在编辑的那个」最常见。 */
function loadFromEditor() {
  const id = editor.activeId
  const text = id != null ? getText(id) : ''
  if (!text) {
    toast.warn(t('convert.emptyInput'))
    return
  }
  const v = id != null ? get(id) : undefined
  if (v) {
    const { from, to } = v.state.selection.main
    targetRange.value = from < to ? { from, to } : null
  }
  input.value = text
  void run()
}

function swap() {
  const f = from.value
  from.value = to.value
  to.value = f
  // 输出搬到输入：用户连着转两次（A→B 再 B→C）时不用手动复制。
  if (output.value) {
    input.value = output.value
    output.value = ''
    targetRange.value = null
  }
}

async function copyOut() {
  if (!output.value) return
  await copyText(output.value)
  toast.ok(t('common.copied'))
}

async function run() {
  if (!input.value.trim()) {
    toast.warn(t('convert.emptyInput'))
    return
  }
  loading.value = true
  try {
    output.value = await convertText(input.value, from.value, to.value)
  } catch (e) {
    output.value = ''
    toast.bad(t('convert.parseFailed', { reason: (e as Error).message }))
  } finally {
    loading.value = false
  }
}

/** 把结果写回编辑器：有选区替换选区，没选区替换全文。
 *  用活跃标签而不是「聚焦的」——浮窗按钮正握着焦点。 */
function applyToEditor() {
  const id = editor.activeId
  if (id == null || !output.value) return
  const v = get(id)
  const range = targetRange.value
  if (range && v) {
    const len = v.state.doc.length
    doReplaceRange(id, Math.min(range.from, len), Math.min(range.to, len), output.value)
  } else if (v) {
    doReplaceRange(id, 0, v.state.doc.length, output.value)
  }
  dialogs.close()
}
</script>

<style scoped>
.nt-conv {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
  min-height: 0;
}

.nt-conv-bar {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  flex: none;
}

.nt-conv-sel {
  width: 110px;
  flex: none;
}

.nt-conv-swap {
  flex: none;
}

.nt-conv-spacer {
  flex: 1;
}

.nt-conv-cols {
  display: flex;
  gap: var(--nt-sp-3);
  flex: 1;
  min-height: 0;
}

.nt-conv-col {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-2);
}

.nt-conv-load {
  height: auto;
  font-size: var(--nt-fs-hint);
}

.nt-conv-out-actions {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
}

.nt-conv-out-actions .nt-text-btn {
  height: auto;
  font-size: var(--nt-fs-hint);
}

.nt-conv-out {
  margin: 0;
  flex: 1;
  min-height: 0;
}
</style>
