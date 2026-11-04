<!-- 大文件查看器：虚拟滚动 + 按页读，内存只驻留可视区行。 -->
<template>
  <div class="nt-bigfile" @keydown="onKey">
    <div class="nt-bigfile-bar">
      <span class="nt-bigfile-badge">
        <NtIcon name="file" size="sm" />
        {{ t('bigfile.mode') }}
      </span>
      <span class="nt-readout" :title="path">{{ fileName }}</span>
      <span class="nt-readout">{{ t('bigfile.lines', { n: total.toLocaleString() }) }}</span>
      <span class="nt-readout">{{ sizeText }}</span>
      <span class="nt-readout" :title="t('bigfile.readonlyHint')">{{ encodingLabel }}</span>
      <div class="nt-bigfile-spacer" />
      <div class="nt-bigfile-find">
        <input
          ref="findInput"
          v-model="query"
          class="nt-bigfile-input"
          :placeholder="t('bigfile.findPlaceholder')"
          @keydown.enter.prevent="findNext"
          @keydown.esc="clearFind"
        />
        <label class="nt-bigfile-check">
          <input v-model="matchCase" type="checkbox" />
          <span>{{ t('find.matchCase') }}</span>
        </label>
        <label class="nt-bigfile-check">
          <input v-model="wholeWord" type="checkbox" />
          <span>{{ t('find.wholeWord') }}</span>
        </label>
        <ElButton size="small" :loading="finding" @click="findNext">
          {{ t('find.next') }}
        </ElButton>
        <span v-if="findNote" class="nt-bigfile-note">{{ findNote }}</span>
      </div>
      <ElButton size="small" @click="gotoLine">
        <NtIcon name="list" size="sm" />
        <span class="nt-btn-text">{{ t('dialog.gotoLine') }}</span>
      </ElButton>
      <ElButton size="small" @click="saveAs">
        <NtIcon name="download" size="sm" />
        <span class="nt-btn-text">{{ t('menu.saveAs') }}</span>
      </ElButton>
    </div>

    <div ref="host" class="nt-bigfile-body" @scroll.passive="onScroll">
      <div class="nt-bigfile-spacer-top" :style="{ height: `${topPad}px` }" />
      <div
        v-for="row in rows"
        :key="row.no"
        class="nt-bigfile-row"
        :class="{ 'is-hit': row.hit }"
        :style="{ height: `${ROW_H}px` }"
        @click="onRowClick(row.no)"
      >
        <span class="nt-bigfile-no">{{ row.no }}</span>
        <span class="nt-bigfile-text" v-html="row.html" />
      </div>
      <div v-if="rows.length === 0" class="nt-bigfile-loading">
        <span class="nt-spinner" />
      </div>
      <div class="nt-bigfile-spacer-bottom" :style="{ height: `${bottomPad}px` }" />
    </div>

    <div class="nt-bigfile-foot">
      <span class="nt-readout">{{ t('bigfile.readonlyHint') }}</span>
      <span class="nt-bigfile-spacer" />
      <span class="nt-readout">{{ t('bigfile.jump', { line: cursorLine }) }}</span>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton } from 'element-plus'
import NtIcon from '@/components/common/NtIcon.vue'
import { api, normalizeError } from '@/api'
import { useToastStore } from '@/stores/toast'
import { askText } from '@/components/common/askText'
import { baseName, formatSize } from '@/utils'
import type { BigFileHitVO, BigFileMetaVO } from '@/types/api'

const props = defineProps<{ path: string; encoding?: string }>()
const emit = defineEmits<{ (e: 'meta', meta: BigFileMetaVO): void }>()

const { t } = useI18n()
const toast = useToastStore()

/** 行高：与 CSS 里 `.nt-bigfile-row` 的 height 同值，虚拟滚动靠它算偏移。 */
const ROW_H = 20
/** 一页请求的行数：略大于常见一屏（视口 800px ≈ 40 行），翻页更顺。 */
const PAGE_LINES = 300
/** 视口上方预取的页数：快速滚动时不至于一片空白。 */
const PREFETCH = 2

const meta = ref<BigFileMetaVO | null>(null)
const total = computed(() => meta.value?.total_lines ?? 0)
const fileName = computed(() => baseName(props.path))
const sizeText = computed(() => (meta.value ? formatSize(meta.value.size) : ''))
const encodingLabel = computed(() => meta.value?.encoding ?? '')
const host = ref<HTMLElement | null>(null)
const findInput = ref<HTMLInputElement | null>(null)

/** 已载入的页：页号 → 行数组。只保留视口附近的页。 */
const pages = ref(new Map<number, string[]>())
const scrollTop = ref(0)
const viewHeight = ref(0)
const cursorLine = ref(1)
/** 查找命中行号集合：用于行背景高亮与「下一个」定位。 */
const hitLines = ref(new Set<number>())
const query = ref('')
const matchCase = ref(false)
const wholeWord = ref(false)
const finding = ref(false)
const findNote = ref('')
/** 当前查找游标：下一次「查找下一个」从这一行之后找。 */
let findCursor = 0
let findRound = 0

const firstVisible = computed(() => Math.floor(scrollTop.value / ROW_H))
const pageOf = (line: number) => Math.floor((line - 1) / PAGE_LINES)

/** 渲染窗口：视口行 + 预取。 */
const windowStart = computed(() => Math.max(0, firstVisible.value - PREFETCH * 40))
const windowEnd = computed(
  () => Math.min(total.value, firstVisible.value + Math.ceil(viewHeight.value / ROW_H) + 40),
)
const topPad = computed(() => windowStart.value * ROW_H)
const bottomPad = computed(() => Math.max(0, (total.value - windowEnd.value) * ROW_H))

interface Row {
  no: number
  html: string
  hit: boolean
}

/** 当前渲染的行。正文从页缓存取；缺失的页在 ensurePages 里异步补。 */
const rows = computed<Row[]>(() => {
  const out: Row[] = []
  const cache = pages.value
  for (let line = windowStart.value + 1; line <= windowEnd.value; line += 1) {
    const page = cache.get(pageOf(line))
    if (!page) continue
    const text = page[(line - 1) % PAGE_LINES]
    if (text === undefined) continue
    out.push({ no: line, html: escapeHtml(text), hit: hitLines.value.has(line) })
  }
  return out
})

/** HTML 转义：正文直接进 v-html，不转义会把 `<` 当标签吃掉。 */
function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

/** 请求视口覆盖的页（含预取），已在缓存里的跳过。 */
const inflight = new Set<number>()
async function ensurePages() {
  if (!meta.value) return
  const from = pageOf(Math.max(1, windowStart.value))
  const to = pageOf(Math.max(1, windowEnd.value))
  for (let p = from; p <= to; p += 1) {
    if (pages.value.has(p) || inflight.has(p)) continue
    inflight.add(p)
    const startLine = p * PAGE_LINES + 1
    void api
      .readBigPage({ path: props.path, start_line: startLine, lines: PAGE_LINES })
      .then((res) => {
        if (!res.lines.length) return
        // 页可能因字节预算少于 PAGE_LINES：按实际长度记录页步长。
        const next = new Map(pages.value)
        next.set(p, res.lines)
        pages.value = next
      })
      .catch((e) => toast.bad(normalizeError(e).message))
      .finally(() => inflight.delete(p))
  }
  // 只保留视口附近的页：几十页下去内存会一直涨。
  pruneFarPages(from, to)
}

function pruneFarPages(from: number, to: number) {
  if (pages.value.size <= 12) return
  const next = new Map<number, string[]>()
  for (const [k, v] of pages.value) {
    if (k >= from - 2 && k <= to + 2) next.set(k, v)
  }
  pages.value = next
}

function onScroll() {
  scrollTop.value = host.value?.scrollTop ?? 0
  void ensurePages()
}

/** 跳到某行：把滚动条放到该行，并按页载入。 */
async function jumpTo(line: number) {
  const l = Math.min(Math.max(1, line), Math.max(1, total.value))
  cursorLine.value = l
  if (!host.value) return
  host.value.scrollTop = (l - 1) * ROW_H - viewHeight.value / 3
  scrollTop.value = host.value.scrollTop
  await ensurePages()
  void highlightLine(l)
}

/** 短暂高亮目标行：跳过去之后要能看见落在哪。 */
const flash = ref(0)
function highlightLine(line: number) {
  flash.value = line
  window.setTimeout(() => {
    if (flash.value === line) flash.value = 0
  }, 1200)
}

async function gotoLine() {
  const v = await askText({
    title: t('dialog.gotoLine'),
    label: t('dialog.lineNumber'),
    value: String(cursorLine.value),
  })
  const n = Number(v)
  if (!Number.isFinite(n) || n < 1) return
  await jumpTo(Math.floor(n))
}

/** 查找：流式扫描后端，命中行记下来，光标移到第一个。 */
async function findNext() {
  const q = query.value.trim()
  if (!q) {
    findInput.value?.focus()
    return
  }
  finding.value = true
  findNote.value = ''
  try {
    const res = await api.findInBigFile({
      path: props.path,
      query: q,
      match_case: matchCase.value,
      whole_word: wholeWord.value,
      from_line: findRound === 0 ? 1 : findCursor + 1,
      skip: 0,
      limit: 500,
    })
    if (res.hits.length === 0) {
      if (findRound > 0) {
        // 到头了：回到开头再试一轮，用户按 F3 不会想记圈数。
        findRound = 0
        findCursor = 0
        findNote.value = t('find.wrapped')
        finding.value = false
        return
      }
      findNote.value = t('find.noMatch')
      hitLines.value = new Set()
      return
    }
    findRound += 1
    findNote.value = t('find.hitCount', { n: res.hits.length })
    hitLines.value = new Set(res.hits.map((h: BigFileHitVO) => h.line))
    findCursor = res.hits[0].line
    await jumpTo(res.hits[0].line)
  } catch (e) {
    toast.bad(normalizeError(e).message)
  } finally {
    finding.value = false
  }
}

function clearFind() {
  query.value = ''
  hitLines.value = new Set()
  findNote.value = ''
  findRound = 0
  findCursor = 0
}

function onRowClick(line: number) {
  cursorLine.value = line
}

function onKey(e: KeyboardEvent) {
  if (e.ctrlKey && e.key.toLowerCase() === 'f') {
    e.preventDefault()
    findInput.value?.select()
    findInput.value?.focus()
  }
}

/** 另存：把当前行区间（默认全文件）写到新文件。 */
async function saveAs() {
  const name = fileName.value
  try {
    const dest = await api.saveFileDialog(name)
    if (!dest) return
    await api.saveBigRange({
      path: props.path,
      dest,
      start_line: 1,
      end_line: 0,
    })
    toast.ok(t('toast.saved', { name: baseName(dest) }))
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}

let ro: ResizeObserver | null = null

/** 外部变更：丢掉页缓存并重建索引（文件可能整体变了）。 */
async function reload() {
  pages.value = new Map()
  hitLines.value = new Set()
  try {
    const m = await api.openBigFile(props.path, props.encoding)
    meta.value = m
    emit('meta', m)
    if (host.value) host.value.scrollTop = 0
    scrollTop.value = 0
    await ensurePages()
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}

onMounted(async () => {
  try {
    const m = await api.openBigFile(props.path, props.encoding)
    meta.value = m
    emit('meta', m)
    if (host.value) viewHeight.value = host.value.clientHeight
    await ensurePages()
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
  ro = new ResizeObserver(() => {
    viewHeight.value = host.value?.clientHeight ?? 0
    void ensurePages()
  })
  if (host.value) ro.observe(host.value)
})

onBeforeUnmount(() => {
  ro?.disconnect()
  pages.value = new Map()
})

watch(
  () => props.path,
  () => {
    pages.value = new Map()
    hitLines.value = new Set()
    findRound = 0
    findCursor = 0
  },
)

defineExpose({ jumpTo, findNext, highlightLine, flash, reload })
</script>

<style scoped>
.nt-bigfile {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  min-width: 0;
  background: var(--nt-editor-bg);
}

.nt-bigfile-bar {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
  padding: var(--nt-sp-1) var(--nt-sp-3);
  border-bottom: 1px solid var(--nt-border);
  min-height: var(--nt-h-toolbar);
  flex-wrap: wrap;
}

.nt-bigfile-badge {
  display: inline-flex;
  align-items: center;
  gap: var(--nt-sp-1);
  padding: 0 var(--nt-sp-2);
  height: var(--nt-ctl-h-sm);
  border-radius: var(--nt-radius-xs);
  background: var(--nt-elev-raised);
  border: 1px solid var(--nt-border);
  color: var(--nt-primary);
  font-size: var(--nt-fs-hint);
  font-weight: 600;
}

.nt-bigfile-spacer {
  flex: 1;
}

.nt-bigfile-find {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
}

.nt-bigfile-input {
  width: 180px;
  height: var(--nt-ctl-h-sm);
  padding: 0 var(--nt-sp-2);
  border: 1px solid var(--nt-control-border);
  border-radius: var(--nt-radius-xs);
  background: var(--nt-control-bg);
  color: var(--nt-ink-1);
  font-size: var(--nt-fs-sm);
  transition: border-color var(--nt-dur-fast) var(--nt-ease);
}

.nt-bigfile-input:focus {
  outline: none;
  border-color: var(--nt-primary);
}

.nt-bigfile-check {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-3);
  cursor: pointer;
}

.nt-bigfile-note {
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-muted);
}

.nt-bigfile-body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  font-family: var(--nt-font-editor);
  font-size: var(--nt-fs-sm);
  line-height: 20px;
}

.nt-bigfile-row {
  display: flex;
  align-items: center;
  height: 20px;
  white-space: pre;
}

.nt-bigfile-row:hover {
  background: var(--nt-elev-hover);
}

.nt-bigfile-row.is-hit {
  background: var(--nt-find-hit-bg);
}

.nt-bigfile-no {
  flex: none;
  width: 72px;
  padding-right: var(--nt-sp-2);
  text-align: right;
  color: var(--nt-ink-muted);
  font-variant-numeric: tabular-nums;
  user-select: none;
}

.nt-bigfile-text {
  flex: 1;
  min-width: 0;
  color: var(--nt-ink-1);
}

.nt-bigfile-loading {
  display: flex;
  justify-content: center;
  padding: var(--nt-sp-6);
}

.nt-bigfile-foot {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
  padding: 0 var(--nt-sp-3);
  height: var(--nt-h-status);
  border-top: 1px solid var(--nt-border);
  background: var(--nt-elev-surface);
}
</style>
