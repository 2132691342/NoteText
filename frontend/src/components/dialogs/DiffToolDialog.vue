// 文本 Diff 浮窗：左右输入 → 行级对齐 + 行内高亮。
//
// 后端给的是 DiffLineVO 列表（含行内 span），这里只负责渲染。
// 「忽略空白 / 大小写」在后端算：两边判据必须一致，否则同一条差异
// 在两个参考实现里看起来不一样。
<template>
  <NtModal :title="t('tool.diffText')" width="900" @close="emit('close')">
    <div class="nt-tool">
      <div class="nt-tool-actions">
        <ElCheckbox v-model="ignoreWs">{{ t('tool.ignoreWs') }}</ElCheckbox>
        <ElCheckbox v-model="ignoreCase">{{ t('tool.ignoreCase') }}</ElCheckbox>
        <div class="nt-tool-actions-right">
          <ElButton size="small" @click="swap">{{ t('tool.right') }} ↔ {{ t('tool.left') }}</ElButton>
          <ElButton size="small" type="primary" :loading="running" @click="run">
            {{ t('tool.run') }}
          </ElButton>
        </div>
      </div>

      <div class="nt-tool-grid">
        <div class="nt-tool-col">
          <div class="nt-tool-col-head">{{ t('tool.left') }}</div>
          <textarea v-model="left" class="nt-tool-io" spellcheck="false" />
        </div>
        <div class="nt-tool-col">
          <div class="nt-tool-col-head">{{ t('tool.right') }}</div>
          <textarea v-model="right" class="nt-tool-io" spellcheck="false" />
        </div>
      </div>

      <div class="nt-diff-wrap">
        <div class="nt-diff">
          <div class="nt-diff-side">
            <div class="nt-diff-head">
              <span>{{ leftTitle }}</span>
              <span class="nt-readout">-{{ result.removed }}</span>
            </div>
            <div v-for="(l, i) in leftLines" :key="`l${i}`" class="nt-diff-line" :class="`is-${l.op}`">
              <span class="nt-diff-no">{{ l.no || '' }}</span>
              <span class="nt-diff-text" v-html="l.html" />
            </div>
          </div>
          <div class="nt-diff-side">
            <div class="nt-diff-head">
              <span>{{ rightTitle }}</span>
              <span class="nt-readout">+{{ result.added }}</span>
            </div>
            <div v-for="(l, i) in rightLines" :key="`r${i}`" class="nt-diff-line" :class="`is-${l.op}`">
              <span class="nt-diff-no">{{ l.no || '' }}</span>
              <span class="nt-diff-text" v-html="l.html" />
            </div>
          </div>
        </div>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElCheckbox } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { api, normalizeError } from '@/api'
import { useDialogStore } from '@/stores/dialogs'
import { useEditorStore } from '@/stores/editor'
import { getText } from '@/composables/useEditorRegistry'
import { useToastStore } from '@/stores/toast'
import type { TextDiffVO } from '@/types/api'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const dialogs = useDialogStore()
const editor = useEditorStore()
const toast = useToastStore()

const left = ref(dialogs.seedExtra.left as string ?? '')
const right = ref(dialogs.seedExtra.right as string ?? '')
const leftTitle = ref(dialogs.seedExtra.left_title as string ?? t('tool.left'))
const rightTitle = ref(dialogs.seedExtra.right_title as string ?? t('tool.right'))
const ignoreWs = ref(false)
const ignoreCase = ref(false)
const running = ref(false)
const result = ref<TextDiffVO>({ ok: true, lines: [], added: 0, removed: 0, changed: 0, error_message: null })

// 默认左 = 当前编辑器全文，右 = 剪贴板（用户最常见的对比场景）。
if (!left.value && editor.activeId != null) left.value = getText(editor.activeId)

const leftLines = computed(() =>
  result.value.lines
    .filter((l) => l.op !== 'added')
    .map((l) => ({ op: l.op, no: l.left_no, html: spansHtml(l.left_text, l.left_spans) })),
)
const rightLines = computed(() =>
  result.value.lines
    .filter((l) => l.op !== 'removed')
    .map((l) => ({ op: l.op, no: l.right_no, html: spansHtml(l.right_text, l.right_spans) })),
)

/** 把行内 span 包成 <span class="nt-span">。文本先转义，避免注入。 */
function spansHtml(text: string, spans: Array<[number, number]>): string {
  const esc = (s: string) =>
    s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  if (!spans.length) return esc(text)
  let out = ''
  let pos = 0
  for (const [a, b] of spans) {
    out += esc(text.slice(pos, a))
    out += `<span class="nt-span">${esc(text.slice(a, b))}</span>`
    pos = b
  }
  out += esc(text.slice(pos))
  return out
}

async function run() {
  running.value = true
  try {
    result.value = await api.textDiff({
      left: left.value,
      right: right.value,
      left_title: leftTitle.value,
      right_title: rightTitle.value,
      ignore_ws: ignoreWs.value,
      ignore_case: ignoreCase.value,
      ignore_blank: false,
    })
  } catch (e) {
    toast.bad(normalizeError(e).message)
  } finally {
    running.value = false
  }
}

function swap() {
  ;[left.value, right.value] = [right.value, left.value]
  ;[leftTitle.value, rightTitle.value] = [rightTitle.value, leftTitle.value]
  void run()
}

// 打开即跑一次：用户粘完文本就想看结果，不愿再点按钮。
void run()
</script>

<style scoped>
.nt-tool {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
  height: 60vh;
  min-height: 0;
}

.nt-tool-actions {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-4);
  flex: none;
}

.nt-tool-actions-right {
  margin-left: auto;
  display: flex;
  gap: var(--nt-sp-2);
}

.nt-diff-wrap {
  flex: 1;
  min-height: 0;
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius);
  overflow: hidden;
}
</style>
