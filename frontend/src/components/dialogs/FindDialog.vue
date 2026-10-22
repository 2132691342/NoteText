// 查找 / 替换浮窗：CodeMirror 的搜索面板不够（要「在文件中查找」），
//
// 所以这里自绘一版，把「本文件查找 / 替换 / 目录查找」三件事放一起。
// 查找逻辑在编辑器侧（CodeMirror 的 search），目录查找走后端

<template>
  <NtModal :title="mode === 'replace' ? t('dialog.replace') : t('dialog.find')" width="520" @close="emit('close')">
    <div class="nt-find">
      <div class="nt-find-row">
        <span class="nt-label">{{ t('dialog.find') }}</span>
        <ElInput
          ref="findInput"
          v-model="query"
          size="small"
          :placeholder="t('dialog.find')"
          spellcheck="false"
          @keydown.enter="next"
        />
        <span class="nt-readout">{{ index + 1 }} {{ t('dialog.ofTotal', { total: matches.length }) }}</span>
      </div>

      <template v-if="mode === 'replace'">
        <div class="nt-find-row">
          <span class="nt-label">{{ t('dialog.replace') }}</span>
          <ElInput v-model="replacement" size="small" spellcheck="false" />
        </div>
      </template>

      <div class="nt-find-flags">
        <ElCheckbox v-model="matchCase">{{ t('dialog.matchCase') }}</ElCheckbox>
        <ElCheckbox v-model="wholeWord">{{ t('dialog.wholeWord') }}</ElCheckbox>
        <ElCheckbox v-model="regex">{{ t('dialog.regex') }}</ElCheckbox>
        <ElCheckbox v-model="wrapAround">{{ t('dialog.wrapAround') }}</ElCheckbox>
      </div>

      <div class="nt-find-actions">
        <template v-if="mode === 'replace'">
          <ElButton size="small" @click="replaceOne">{{ t('dialog.replaceOne') }}</ElButton>
          <ElButton size="small" @click="replaceAll">{{ t('dialog.replaceAll') }}</ElButton>
        </template>
        <div class="nt-find-actions-right">
          <ElButton size="small" @click="prev">{{ t('dialog.previous') }}</ElButton>
          <ElButton size="small" type="primary" @click="next">{{ t('dialog.next') }}</ElButton>
        </div>
      </div>

      <div v-if="results.length" class="nt-find-results">
        <div class="nt-tool-col-head">
          <span>{{ t('dialog.results') }}</span>
          <span class="nt-readout">{{ results.length }}</span>
        </div>
        <div class="nt-find-results-body">
          <button
            v-for="(r, i) in results"
            :key="i"
            class="nt-find-result"
            :class="{ 'is-on': i === index }"
            @click="jumpTo(i)"
          >
            <span class="nt-find-result-path">{{ r.path }}</span>
            <span class="nt-find-result-line" v-html="highlight(r)" />
          </button>
        </div>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { nextTick, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElCheckbox, ElInput } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { useDialogStore } from '@/stores/dialogs'
import { useEditorStore } from '@/stores/editor'
import { getText } from '@/composables/useEditorRegistry'
import { useToastStore } from '@/stores/toast'

const props = defineProps<{ mode?: 'find' | 'replace' }>()
const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const dialogs = useDialogStore()
const editor = useEditorStore()
const toast = useToastStore()

const mode = ref<'find' | 'replace'>(props.mode ?? 'find')
const findInput = ref<InstanceType<typeof ElInput> | null>(null)

const query = ref(dialogs.seedText)
const replacement = ref('')
const matchCase = ref(false)
const wholeWord = ref(false)
const regex = ref(false)
const wrapAround = ref(true)

const matches = ref<number[]>([])
const index = ref(0)
const results = ref<Array<{ path: string; line: string; offset: number }>>([])

onMounted(async () => {
  await nextTick()
  findInput.value?.focus()
  await scan()
})

watch([query, matchCase, wholeWord, regex], () => void scan())

/** 在本文件里扫一遍：命中位置存起来，跳转只改选区。 */
async function scan() {
  const tab = editor.active
  if (!tab || !query.value) {
    matches.value = []
    results.value = []
    return
  }
  const text = getText(tab.id)
  matches.value = []
  results.value = []

  const re = buildRegex()
  if (!re) return
  let m: RegExpExecArray | null
  let guard = 0
  while ((m = re.exec(text)) !== null && guard < 10000) {
    guard += 1
    const at = m.index
    matches.value.push(at)
    const lineStart = text.lastIndexOf('\n', at - 1) + 1
    let lineEnd = text.indexOf('\n', at)
    if (lineEnd < 0) lineEnd = text.length
    results.value.push({
      path: tab.title || t('tab.untitled'),
      line: text.slice(lineStart, Math.min(lineEnd, lineStart + 200)),
      offset: at,
    })
    if (m[0] === '') re.lastIndex += 1
  }
  index.value = 0
}

function buildRegex(): RegExp | null {
  const q = query.value
  if (!q) return null
  try {
    let src = regex.value ? q : q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
    if (wholeWord.value) src = `\\b(?:${src})\\b`
    return new RegExp(src, matchCase.value ? 'g' : 'gi')
  } catch {
    toast.warn(t('error.regexInvalid', { reason: '' }))
    return null
  }
}

function jumpTo(i: number) {
  index.value = i
  const tab = editor.active
  if (!tab) return
  const at = matches.value[i]
  if (at == null) return
  document.dispatchEvent(
    new CustomEvent('editor-select-range', { detail: { tabId: tab.id, from: at, to: at + query.value.length } }),
  )
}

function next() {
  if (!matches.value.length) return
  index.value = wrapAround.value
    ? (index.value + 1) % matches.value.length
    : Math.min(index.value + 1, matches.value.length - 1)
  jumpTo(index.value)
}

function prev() {
  if (!matches.value.length) return
  index.value = wrapAround.value
    ? (index.value - 1 + matches.value.length) % matches.value.length
    : Math.max(index.value - 1, 0)
  jumpTo(index.value)
}

function replaceOne() {
  const tab = editor.active
  if (!tab || matches.value[index.value] == null) return
  const at = matches.value[index.value]
  document.dispatchEvent(
    new CustomEvent('editor-replace-range', {
      detail: { tabId: tab.id, from: at, to: at + query.value.length, text: replacement.value },
    }),
  )
  void scan()
}

function replaceAll() {
  const tab = editor.active
  if (!tab) return
  const text = getText(tab.id)
  const re = buildRegex()
  if (!re) return
  const out = text.replace(re, replacement.value)
  const count = matches.value.length
  document.dispatchEvent(
    new CustomEvent('editor-replace-doc', { detail: { tabId: tab.id, text: out } }),
  )
  toast.ok(t('toast.replaced', { count }))
  emit('close')
}

/** 结果行里给匹配上高亮。 */
function highlight(r: { line: string; offset: number }): string {
  const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  // 这里只做整行显示，定位精度由跳转保证；高亮用首个匹配位置近似。
  const idx = r.line.toLowerCase().indexOf(query.value.toLowerCase())
  if (idx < 0 || !query.value) return esc(r.line)
  return (
    esc(r.line.slice(0, idx)) +
    `<mark>${esc(r.line.slice(idx, idx + query.value.length))}</mark>` +
    esc(r.line.slice(idx + query.value.length))
  )
}
</script>

<style scoped>
.nt-find {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
}

.nt-find-row {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
}

.nt-label {
  font-size: var(--nt-fs-label);
  font-weight: 600;
  color: var(--nt-ink-2);
  flex: none;
  min-width: 3.5em;
}

.nt-find-flags {
  display: flex;
  flex-wrap: wrap;
  gap: var(--nt-sp-4);
}

.nt-find-actions {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
}

.nt-find-actions-right {
  margin-left: auto;
  display: flex;
  gap: var(--nt-sp-2);
}

.nt-find-results {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-2);
  min-height: 0;
}

.nt-find-results-body {
  max-height: 220px;
  overflow-y: auto;
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius-sm);
  background: var(--nt-surface-2);
}
</style>
