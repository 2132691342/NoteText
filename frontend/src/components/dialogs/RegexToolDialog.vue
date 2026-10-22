// 正则测试浮窗：实时匹配 + 捕获组表格。
//
// 输入变化即重算（防抖 150ms）：测试器要「边打边看」，
// 点一次按钮才更新就没法用了。
<template>
  <NtModal :title="t('tool.regex')" width="760" @close="emit('close')">
    <div class="nt-regex-tool">
      <div class="nt-regex-pattern">
        <span class="nt-label">{{ t('tool.regexPattern') }}</span>
        <ElInput v-model="pattern" size="small" placeholder="\d+" spellcheck="false" />
      </div>
      <div class="nt-regex-flags">
        <ElCheckbox v-model="ignoreCase">i</ElCheckbox>
        <ElCheckbox v-model="multiline">m</ElCheckbox>
        <ElCheckbox v-model="dotAll">s</ElCheckbox>
        <ElCheckbox v-model="extended">x</ElCheckbox>
        <span class="nt-regex-count">{{ matches.length }} {{ t('tool.matches') }}</span>
      </div>

      <div class="nt-tool-grid">
        <div class="nt-tool-col">
          <div class="nt-tool-col-head">{{ t('tool.regexText') }}</div>
          <textarea v-model="text" class="nt-tool-io" spellcheck="false" />
        </div>
        <div class="nt-tool-col">
          <div class="nt-tool-col-head">
            <span>{{ t('tool.matches') }}</span>
            <span v-if="truncated" class="nt-readout">{{ t('dialog.onlyFirst', { n: 1000 }) }}</span>
          </div>
          <div class="nt-regex-results">
            <div v-if="!matches.length && !error" class="nt-empty-sm">
              {{ t('dialog.noMatches') }}
            </div>
            <div v-for="(m, i) in matches" :key="i" class="nt-regex-match">
              <span class="nt-regex-match-no">{{ i + 1 }}</span>
              <span class="nt-regex-match-text">{{ m.whole }}</span>
              <span class="nt-readout">{{ m.start }}–{{ m.end }}</span>
            </div>
          </div>
        </div>
      </div>

      <div v-if="matches.length" class="nt-regex-groups">
        <div class="nt-tool-col-head">{{ t('tool.groups') }}</div>
        <ElTable :data="groupRows" size="small" border max-height="180">
          <ElTableColumn type="index" label="#" width="48" />
          <ElTableColumn prop="group" :label="t('tool.groups')" width="80" />
          <ElTableColumn prop="value" show-overflow-tooltip />
        </ElTable>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElCheckbox, ElInput, ElTable, ElTableColumn } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { api, normalizeError } from '@/api'
import { useDialogStore } from '@/stores/dialogs'
import { useEditorStore } from '@/stores/editor'
import { getText, getSelection } from '@/composables/useEditorRegistry'
import { debounce } from '@/utils'
import type { RegexTestVO } from '@/types/api'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const dialogs = useDialogStore()
const editor = useEditorStore()

const pattern = ref('')
const text = ref(dialogs.seedText)
const ignoreCase = ref(false)
const multiline = ref(false)
const dotAll = ref(false)
const extended = ref(false)
const matches = ref<RegexTestVO['matches']>([])
const truncated = ref(false)
const error = ref('')

onMounted(() => {
  // 默认文本：编辑器里选中的内容；没选中就用全文。
  if (!text.value && editor.activeId != null) {
    text.value = getSelection(editor.activeId) || getText(editor.activeId)
  }
})

const groupRows = computed(() => {
  const rows: Array<{ group: string; value: string }> = []
  matches.value.forEach((m, i) => {
    m.groups.forEach((g, gi) => {
      rows.push({ group: `#${i + 1}.${gi}`, value: g })
    })
  })
  return rows
})

/** 防抖重算：打字过程中每 150ms 一次，避免每次按键都过 IPC。 */
const recompute = debounce(async () => {
  if (!pattern.value) {
    matches.value = []
    error.value = ''
    return
  }
  try {
    const r = await api.regexTest({
      pattern: pattern.value,
      text: text.value,
      ignore_case: ignoreCase.value,
      multiline: multiline.value,
      dot_all: dotAll.value,
      extended: extended.value,
    })
    if (r.ok) {
      matches.value = r.matches
      truncated.value = r.truncated
      error.value = ''
    } else {
      matches.value = []
      error.value = r.error_message ?? ''
    }
  } catch (e) {
    error.value = normalizeError(e).message
  }
}, 150)

watch([pattern, text, ignoreCase, multiline, dotAll, extended], () => void recompute())
</script>

<style scoped>
.nt-regex-tool {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
  height: 60vh;
  min-height: 0;
}

.nt-regex-pattern {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
  flex: none;
}

.nt-label {
  font-size: var(--nt-fs-label);
  font-weight: 600;
  color: var(--nt-ink-2);
  flex: none;
  min-width: 4em;
}

.nt-regex-flags {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-4);
  flex: none;
}

.nt-regex-count {
  margin-left: auto;
  font-variant-numeric: tabular-nums;
  color: var(--nt-ink-2);
  font-size: var(--nt-fs-sm);
}

.nt-regex-results {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  background: var(--nt-surface-2);
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius-sm);
}

.nt-regex-match {
  display: flex;
  align-items: baseline;
  gap: var(--nt-sp-2);
  padding: var(--nt-sp-1) var(--nt-sp-2);
  font-size: var(--nt-fs-sm);
  border-bottom: 1px solid var(--nt-border);
}

.nt-regex-match:last-child {
  border-bottom: none;
}

.nt-regex-match-no {
  color: var(--nt-ink-muted);
  font-variant-numeric: tabular-nums;
  flex: none;
  width: 2em;
  text-align: right;
}

.nt-regex-match-text {
  font-family: var(--font-mono);
  color: var(--nt-ink);
  flex: 1;
  min-width: 0;
  word-break: break-all;
}

.nt-regex-groups {
  flex: none;
}

.nt-empty-sm {
  padding: var(--nt-sp-3);
  text-align: center;
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-muted);
}
</style>
