// 查找 / 替换浮窗：本文件查找 + 替换。
//
// 目录级查找在 FindInFilesDialog（结果落侧栏）；这里只管当前标签。
// 查找逻辑在前端算（后端只做目录扫描）：本文件的匹配要边打字边出结果，
// 走一趟 IPC 就慢得不能用了。
<template>
  <NtModal
    :title="isReplace ? t('dialog.replace') : t('dialog.find')"
    width="560"
    @close="emit('close')"
  >
    <div class="nt-find">
      <div class="nt-find-row">
        <span class="nt-field-label">{{ t('dialog.find') }}</span>
        <ElInput
          ref="findInput"
          v-model="query"
          size="small"
          :placeholder="t('dialog.find')"
          spellcheck="false"
          @keydown.enter.prevent="onEnter($event)"
          @keydown.esc.prevent="emit('close')"
        />
        <span class="nt-readout nt-find-count">{{ countLabel }}</span>
      </div>

      <div v-if="isReplace" class="nt-find-row">
        <span class="nt-field-label">{{ t('dialog.replace') }}</span>
        <ElInput
          v-model="replacement"
          size="small"
          spellcheck="false"
          @keydown.enter.prevent="replaceOne"
        />
      </div>

      <div class="nt-find-flags">
        <ElCheckbox v-model="matchCase">{{ t('dialog.matchCase') }}</ElCheckbox>
        <ElCheckbox v-model="wholeWord">{{ t('dialog.wholeWord') }}</ElCheckbox>
        <ElCheckbox v-model="regex">{{ t('dialog.regex') }}</ElCheckbox>
        <ElCheckbox v-model="wrapAround">{{ t('dialog.wrapAround') }}</ElCheckbox>
      </div>

      <div class="nt-find-actions">
        <template v-if="isReplace">
          <ElButton size="small" :disabled="!hasMatches" @click="replaceOne">
            {{ t('dialog.replaceOne') }}
          </ElButton>
          <ElButton size="small" :disabled="!hasMatches" @click="replaceAll">
            {{ t('dialog.replaceAll') }}
          </ElButton>
        </template>
        <div class="nt-find-actions-right">
          <ElButton size="small" :disabled="!hasMatches" @click="prev">
            {{ t('dialog.previous') }}
          </ElButton>
          <ElButton size="small" type="primary" :disabled="!hasMatches" @click="next">
            {{ t('dialog.next') }}
          </ElButton>
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
            :key="`${r.offset}-${i}`"
            class="nt-find-result"
            :class="{ 'is-on': i === index }"
            @click="jumpTo(i)"
          >
            <span class="nt-find-result-lineno">{{ r.lineNo }}</span>
            <span class="nt-find-result-line" v-html="highlight(r)" />
          </button>
        </div>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElCheckbox, ElInput } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { useDialogStore } from '@/stores/dialogs'
import { useEditorStore } from '@/stores/editor'
import { getText, get } from '@/composables/useEditorRegistry'
import { doApplyChanges, doSelectRange } from '@/composables/useEditorActions'
import { useToastStore } from '@/stores/toast'

const props = defineProps<{ mode?: 'find' | 'replace' }>()
const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()
const dialogs = useDialogStore()
const editor = useEditorStore()
const toast = useToastStore()

const isReplace = computed(() => props.mode === 'replace')
const findInput = ref<InstanceType<typeof ElInput> | null>(null)

const query = ref(dialogs.seedText)
const replacement = ref('')
const matchCase = ref(false)
const wholeWord = ref(false)
const regex = ref(false)
const wrapAround = ref(true)

/** 命中区间：offset 是字符下标，length 是匹配长度。 */
interface Hit {
  offset: number
  length: number
  lineNo: number
  line: string
}
const hits = ref<Hit[]>([])
const results = computed(() => hits.value)
const index = ref(0)
const hasMatches = computed(() => hits.value.length > 0)

const countLabel = computed(() => {
  if (!query.value) return ''
  if (!hasMatches.value) return t('dialog.noMatches')
  return `${index.value + 1} ${t('dialog.ofTotal', { total: hits.value.length })}`
})

onMounted(async () => {
  await nextTick()
  findInput.value?.focus()
  // 打开时若有选中文本，直接用它当查询词（命令层已塞进 seedText）。
  scan()
})

onUnmounted(() => {
  clearPaint()
})

watch([query, matchCase, wholeWord, regex], () => scan())
// 正文变了要重扫：用户边查边改，命中位置会失效。
watch(
  () => {
    const id = editor.activeId
    return id == null ? '' : getText(id).length
  },
  () => scan(),
)

/** 扫一遍当前标签：算出全部命中偏移。 */
function scan() {
  const tab = editor.active
  if (!tab || !query.value) {
    hits.value = []
    index.value = 0
    clearPaint()
    return
  }
  const text = getText(tab.id)
  const found: Hit[] = []
  const re = buildRegex()
  if (!re) {
    hits.value = []
    return
  }
  let m: RegExpExecArray | null
  let guard = 0
  while ((m = re.exec(text)) !== null && guard < 20000) {
    guard += 1
    const offset = m.index
    const length = m[0].length
    const lineStart = text.lastIndexOf('\n', offset - 1) + 1
    let lineEnd = text.indexOf('\n', offset)
    if (lineEnd < 0) lineEnd = text.length
    found.push({
      offset,
      length,
      lineNo: text.slice(0, offset).split('\n').length,
      line: text.slice(lineStart, Math.min(lineEnd, lineStart + 240)),
    })
    // 零宽匹配：手动推进，否则死循环。
    if (m[0] === '') re.lastIndex += 1
  }
  hits.value = found
  if (index.value >= found.length) index.value = 0
  // 第一次有结果就把视野带过去：用户按 Ctrl+F 后应该看到第一处。
  if (found.length && !paintedOnce) {
    paintedOnce = true
    jumpTo(0)
  } else {
    paintAll()
  }
}

let paintedOnce = false

function buildRegex(): RegExp | null {
  const q = query.value
  if (!q) return null
  try {
    let src = regex.value ? q : q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
    if (wholeWord.value) src = `\\b(?:${src})\\b`
    return new RegExp(src, matchCase.value ? 'g' : 'gi')
  } catch {
    return null
  }
}

function jumpTo(i: number) {
  const tab = editor.active
  if (!tab || !hits.value[i]) return
  index.value = i
  const h = hits.value[i]
  doSelectRange(tab.id, h.offset, h.offset + h.length)
  paintAll()
}

function next() {
  if (!hasMatches.value) return
  const n = hits.value.length
  const nextIdx = wrapAround.value
    ? (index.value + 1) % n
    : Math.min(index.value + 1, n - 1)
  jumpTo(nextIdx)
}

function prev() {
  if (!hasMatches.value) return
  const n = hits.value.length
  const prevIdx = wrapAround.value
    ? (index.value - 1 + n) % n
    : Math.max(index.value - 1, 0)
  jumpTo(prevIdx)
}

/** 回车：替换模式走「替换当前」，查找模式走「下一个」。 */
function onEnter(e: Event | KeyboardEvent) {
  const ke = e as KeyboardEvent
  if (isReplace.value && (ke.ctrlKey || ke.metaKey)) {
    replaceAll()
    return
  }
  next()
}

function replaceOne() {
  const tab = editor.active
  const h = hits.value[index.value]
  if (!tab || !h) return
  const v = get(tab.id)
  if (!v) return
  // 用一次事务替换：撤销是「一步回到替换前」，而不是逐字回退。
  const from = h.offset
  const to = h.offset + h.length
  const literal = replacement.value
  doApplyChanges(tab.id, [{ from, to, insert: literal }])
  const delta = literal.length - h.length
  // 命中表不必重扫全文：后面的偏移量整体平移即可（比重新扫快得多）。
  hits.value = hits.value
    .filter((x) => x.offset !== from)
    .map((x) => (x.offset > from ? { ...x, offset: x.offset + delta } : x))
  if (index.value >= hits.value.length) index.value = 0
  paintAll()
}

function replaceAll() {
  const tab = editor.active
  if (!tab || !hits.value.length) return
  const count = hits.value.length
  // 一次提交全部变更：撤销一次就回到替换前（逐个 dispatch 要按 N 次）。
  const changes = hits.value.map((h) => ({
    from: h.offset,
    to: h.offset + h.length,
    insert: replacement.value,
  }))
  doApplyChanges(tab.id, changes)
  toast.ok(t('toast.replaced', { count }))
  scan()
}

/** 当前命中靠编辑器选区表示；其余命中在列表里可见，编辑区不铺满高亮。 */
function paintAll() {
  /* no-op：选中即高亮（highlightSelectionMatches 也会标出同词命中） */
}

function clearPaint() {
  /* no-op：选区由下一次点击或输入自然覆盖 */
}

/** 结果行里给匹配上高亮。 */
function highlight(r: Hit): string {
  const esc = (s: string) =>
    s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  if (!query.value) return esc(r.line)
  // 在行内定位本次命中的位置：同一行可能有多处，取最近的一处。
  const needle = query.value
  const lower = r.line.toLowerCase()
  const idx = matchCase.value ? r.line.indexOf(needle) : lower.indexOf(needle.toLowerCase())
  if (idx < 0) return esc(r.line)
  return (
    esc(r.line.slice(0, idx)) +
    `<mark>${esc(r.line.slice(idx, idx + needle.length))}</mark>` +
    esc(r.line.slice(idx + needle.length))
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

.nt-field-label {
  font-size: var(--nt-fs-sm);
  font-weight: 600;
  color: var(--nt-ink-2);
  flex: none;
  min-width: 3.5em;
}

.nt-find-count {
  flex: none;
  min-width: 6em;
  text-align: right;
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
  max-height: 260px;
  overflow-y: auto;
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius-sm);
  background: var(--nt-surface-2);
}

.nt-find-result {
  display: flex;
  gap: var(--nt-sp-2);
  width: 100%;
  padding: var(--nt-sp-1) var(--nt-sp-2);
  text-align: left;
  font-family: var(--nt-font-mono);
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-2);
  border-radius: var(--nt-radius-xs);
  transition:
    background-color var(--nt-dur-fast) var(--nt-ease),
    color var(--nt-dur-fast) var(--nt-ease);
}

.nt-find-result:hover {
  background: var(--nt-surface-hover);
}

.nt-find-result.is-on {
  background: var(--nt-primary-soft);
  color: var(--nt-ink);
}

.nt-find-result-lineno {
  flex: none;
  min-width: 3em;
  text-align: right;
  color: var(--nt-ink-muted);
  font-variant-numeric: tabular-nums;
}

.nt-find-result-line {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: pre;
}

.nt-find-result-line :deep(mark) {
  background: var(--nt-primary);
  color: var(--nt-primary-contrast);
  border-radius: 2px;
}
</style>
