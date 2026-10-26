// 查找 / 替换浮窗：五个 Tab 覆盖主流编辑器的全部查找形态。
// 五个 Tab 的分工：

<template>
  <NtModal :title="tabTitle" width="620" @close="emit('close')">
    <div class="nt-find">
      <div class="nt-find-tabs" role="tablist">
        <button
          v-for="tb in TABS"
          :key="tb.id"
          class="nt-find-tab"
          :class="{ 'is-on': tab === tb.id }"
          role="tab"
          :aria-selected="tab === tb.id"
          @click="tab = tb.id"
        >
          {{ t(tb.labelKey) }}
        </button>
      </div>

      <!-- ============ 查找 ============ -->
      <template v-if="tab === 'find'">
        <!-- 不可搜的视图（预览 / 十六进制 / 挂载失败）要说明白：
             静默灰按钮的用户报告就是「按钮没法按」。 -->
        <div v-if="!searchable" class="nt-find-unavailable">{{ t('dialog.findUnavailable') }}</div>
        <div class="nt-field">
          <span class="nt-label">{{ t('dialog.find') }}</span>
          <ElInput
            ref="findInput"
            v-model="query"
            size="small"
            spellcheck="false"
            :disabled="!searchable"
            @keydown.enter.prevent="onEnter($event)"
            @keydown.esc.prevent="emit('close')"
          >
            <template v-if="countLabel" #suffix>
              <span class="nt-find-count">{{ countLabel }}</span>
            </template>
          </ElInput>
        </div>
        <div class="nt-find-flags">
          <ElCheckbox v-model="matchCase">{{ t('dialog.matchCase') }}</ElCheckbox>
          <ElCheckbox v-model="wholeWord">{{ t('dialog.wholeWord') }}</ElCheckbox>
          <ElCheckbox v-model="regex">{{ t('dialog.regex') }}</ElCheckbox>
          <ElCheckbox v-model="extended">{{ t('dialog.extended') }}</ElCheckbox>
          <ElCheckbox v-model="inSelection">{{ t('dialog.inSelection') }}</ElCheckbox>
          <ElCheckbox v-model="backward">{{ t('dialog.backward') }}</ElCheckbox>
          <ElCheckbox v-model="wrapAround">{{ t('dialog.wrapAround') }}</ElCheckbox>
        </div>
        <div class="nt-find-actions">
          <span v-if="regexError" class="nt-find-err">{{ regexError }}</span>
          <div class="nt-find-actions-right">
            <ElButton size="small" :disabled="!hasMatches" @click="prev">
              {{ t('dialog.previous') }}
            </ElButton>
            <ElButton size="small" type="primary" :disabled="!hasMatches" @click="next">
              {{ t('dialog.next') }}
            </ElButton>
          </div>
        </div>
      </template>

      <!-- ============ 替换 ============ -->
      <template v-else-if="tab === 'replace'">
        <div v-if="!searchable" class="nt-find-unavailable">{{ t('dialog.findUnavailable') }}</div>
        <!-- 标签已说明这是什么字段，placeholder 不再重复同一句话。
             重复的后果是同一行出现两遍「查找」，用户会以为有两个不同的东西。 -->
        <div class="nt-field">
          <span class="nt-label">{{ t('dialog.find') }}</span>
          <ElInput
            v-model="query"
            size="small"
            spellcheck="false"
            :disabled="!searchable"
            @keydown.enter.prevent="replaceOne"
            @keydown.esc.prevent="emit('close')"
          >
            <template v-if="countLabel" #suffix>
              <span class="nt-find-count">{{ countLabel }}</span>
            </template>
          </ElInput>
        </div>
        <div class="nt-field">
          <span class="nt-label">{{ t('dialog.replaceWith') }}</span>
          <ElInput
            v-model="replacement"
            size="small"
            spellcheck="false"
            :disabled="!searchable"
            @keydown.enter.prevent="replaceOne"
          />
        </div>
        <div class="nt-find-flags">
          <ElCheckbox v-model="matchCase">{{ t('dialog.matchCase') }}</ElCheckbox>
          <ElCheckbox v-model="wholeWord">{{ t('dialog.wholeWord') }}</ElCheckbox>
          <ElCheckbox v-model="regex">{{ t('dialog.regex') }}</ElCheckbox>
          <ElCheckbox v-model="extended">{{ t('dialog.extended') }}</ElCheckbox>
          <ElCheckbox v-model="inSelection">{{ t('dialog.inSelection') }}</ElCheckbox>
          <ElCheckbox v-model="wrapAround">{{ t('dialog.wrapAround') }}</ElCheckbox>
        </div>
        <div class="nt-find-actions">
          <span v-if="regexError" class="nt-find-err">{{ regexError }}</span>
          <div class="nt-find-actions-right">
            <ElButton size="small" :disabled="!hasMatches" @click="prev">
              {{ t('dialog.previous') }}
            </ElButton>
            <ElButton size="small" type="primary" :disabled="!hasMatches" @click="replaceOne">
              {{ t('dialog.replaceOne') }}
            </ElButton>
            <ElButton size="small" type="primary" :loading="replacing" :disabled="!hasMatches" @click="replaceAll">
              {{ t('dialog.replaceAll') }}
            </ElButton>
          </div>
        </div>
      </template>

      <!-- ============ 目录 ============ -->
      <template v-else-if="tab === 'files'">
        <div class="nt-field">
          <span class="nt-label">{{ t('dialog.directory') }}</span>
          <ElInput v-model="dir" size="small" readonly @click="pickDir" />
          <ElButton size="small" class="nt-field-fixed" @click="pickDir">
            {{ t('common.browse') }}
          </ElButton>
        </div>
        <div class="nt-field">
          <span class="nt-label">{{ t('dialog.find') }}</span>
          <ElInput
            v-model="query"
            size="small"
            spellcheck="false"
            @keydown.enter.prevent="runDir"
          />
        </div>
        <div class="nt-field">
          <span class="nt-label">{{ t('dialog.includePattern') }}</span>
          <ElInput v-model="include" size="small" placeholder="*.md;*.txt" spellcheck="false" />
          <span class="nt-label">{{ t('dialog.excludePattern') }}</span>
          <ElInput v-model="exclude" size="small" placeholder="node_modules" spellcheck="false" />
        </div>
        <div class="nt-find-flags">
          <ElCheckbox v-model="matchCase">{{ t('dialog.matchCase') }}</ElCheckbox>
          <ElCheckbox v-model="wholeWord">{{ t('dialog.wholeWord') }}</ElCheckbox>
          <ElCheckbox v-model="regex">{{ t('dialog.regex') }}</ElCheckbox>
          <ElCheckbox v-model="subdirs">{{ t('dialog.includeSubdirs') }}</ElCheckbox>
          <div class="nt-find-actions-right">
            <span v-if="dirRunning" class="nt-readout">
              {{ t('toast.scanning', { n: dirScanned }) }}
            </span>
            <ElButton v-if="dirRunning" size="small" @click="dirStop">{{ t('dialog.stop') }}</ElButton>
            <ElButton v-else size="small" type="primary" :disabled="!query" @click="runDir">
              {{ t('dialog.search') }}
            </ElButton>
          </div>
        </div>
      </template>

      <!-- ============ 标记 ============ -->
      <template v-else-if="tab === 'mark'">
        <div class="nt-field">
          <span class="nt-label">{{ t('dialog.find') }}</span>
          <ElInput
            v-model="query"
            size="small"
            spellcheck="false"
            @keydown.enter.prevent="markAll"
          >
            <template v-if="countLabel" #suffix>
              <span class="nt-find-count">{{ countLabel }}</span>
            </template>
          </ElInput>
        </div>
        <div class="nt-find-flags">
          <ElCheckbox v-model="matchCase">{{ t('dialog.matchCase') }}</ElCheckbox>
          <ElCheckbox v-model="wholeWord">{{ t('dialog.wholeWord') }}</ElCheckbox>
          <ElCheckbox v-model="regex">{{ t('dialog.regex') }}</ElCheckbox>
        </div>
        <div class="nt-find-actions">
          <span class="nt-find-mark-hint">{{ t('dialog.markHint') }}</span>
          <div class="nt-find-actions-right">
            <ElButton size="small" @click="clearMarks">{{ t('dialog.clearMarks') }}</ElButton>
            <ElButton size="small" type="primary" :disabled="!hasMatches" @click="markAll">
              {{ t('dialog.markAll') }}
            </ElButton>
          </div>
        </div>
        <div class="nt-find-marks">
          <span class="nt-label nt-field-fixed">{{ t('dialog.markColor') }}</span>
          <button
            v-for="(c, i) in MARK_COLORS"
            :key="c"
            class="nt-find-mark-swatch"
            :class="{ 'is-on': markColor === c }"
            :style="{ '--swatch': `var(--nt-mark-line-${i + 1})` }"
            :title="t(`dialog.markColor${i + 1}`)"
            @click="markColor = c"
          />
        </div>
      </template>

      <!-- ============ 结果 ============ -->
      <template v-else>
        <div class="nt-find-results">
          <div class="nt-tool-col-head">
            <span>{{ t('dialog.results') }}</span>
            <span class="nt-readout">{{ hits.length }}</span>
          </div>
          <div class="nt-find-results-body">
            <div v-if="!hits.length" class="nt-empty-sm">{{ t('dialog.searchHint') }}</div>
            <button
              v-for="(r, i) in hits"
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

        <!-- 目录查找的结果落在侧栏面板：浮窗关掉后还能反复点开。 -->
        <div v-if="panelGroups.length" class="nt-find-panel-link">
          <span class="nt-readout">
            {{ t('dialog.inFiles') }} · {{ panelTotal }}
          </span>
          <ElButton size="small" class="nt-field-fixed" @click="openPanel">
            {{ t('dialog.openPanel') }}
          </ElButton>
        </div>
      </template>
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
import { useFileTreeStore } from '@/stores/fileTree'
import { useSettingsStore } from '@/stores/settings'
import { useToastStore } from '@/stores/toast'
import { getText, get, docVersionOf } from '@/composables/useEditorRegistry'
import { doApplyChanges, doSelectRange, doMarkSelection, doClearMarks } from '@/composables/useEditorActions'
import { globMatch } from '@/utils'
import { api } from '@/api'
import { rememberQuery, lastQueryState } from '@/composables/useLastQuery'
import { useFindResultsStore } from '@/stores/findResults'
import type { BatchFileHits } from '@/types/api'
import type { MarkColor } from '@/composables/useEditorActions'

const props = defineProps<{ mode?: 'find' | 'replace' }>()
const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()
const dialogs = useDialogStore()
const editor = useEditorStore()
const tree = useFileTreeStore()
const toast = useToastStore()

/** 五个 Tab：查找 / 替换 / 目录 / 标记 / 结果。 */
const TABS = [
  { id: 'find', labelKey: 'dialog.find' },
  { id: 'replace', labelKey: 'dialog.replace' },
  { id: 'files', labelKey: 'dialog.inFiles' },
  { id: 'mark', labelKey: 'dialog.mark' },
  { id: 'results', labelKey: 'dialog.results' },
] as const

type TabId = (typeof TABS)[number]['id']

const MARK_COLORS: MarkColor[] = [1, 2, 3, 4, 5]

const tab = ref<TabId>(props.mode === 'replace' ? 'replace' : 'find')
const tabTitle = computed(() => {
  if (tab.value === 'results') return t('dialog.results')
  return t(TABS.find((x) => x.id === tab.value)?.labelKey ?? 'dialog.find')
})

const settings = useSettingsStore()

const findResults = useFindResultsStore()

/** 侧栏面板里已累积的目录查找结果：结果 Tab 给一个入口过去。 */
const panelGroups = computed(() => findResults.groups)
const panelTotal = computed(() => findResults.total)

/** 打开侧栏的查找结果面板。 */
function openPanel() {
  settings.updateUI({ show_sidebar: true, sidebar_tab: 'findResults' })
  emit('close')
}

const findInput = ref<InstanceType<typeof ElInput> | null>(null)
// seedText 是命令层塞进来的选中文本；没有选中时接着上次的查询词——
// 「改完再按 Ctrl+F」要的是同一个词，不是空框。
const query = ref(dialogs.seedText || lastQueryState().query)
const replacement = ref('')
// 无选中时连选项一起恢复：上次勾了「全字匹配」，这次不该静默丢掉。
const prevQuery = lastQueryState()
const seeded = !!dialogs.seedText
const matchCase = ref(seeded ? false : prevQuery.matchCase)
const wholeWord = ref(seeded ? false : prevQuery.wholeWord)
const regex = ref(seeded ? false : prevQuery.regex)
// 浮窗已经开着时再次 Ctrl+F（比如刚选中新词）：命令层会更新 seedText，查询词要跟上。
watch(
  () => dialogs.seedText,
  (s) => {
    if (s) query.value = s
  },
)
const extended = ref(false)
const inSelection = ref(false)
const backward = ref(false)
const wrapAround = ref(true)
const subdirs = ref(true)
const markColor = ref<MarkColor>(1)

/** 目录 Tab 的状态。 */
const dir = ref(tree.root || '')
const include = ref('*')
const exclude = ref('')
const dirRunning = ref(false)
const dirScanned = ref(0)
let dirCancelled = false

/** 命中区间：offset 是字符下标，length 是匹配长度。 */
interface Hit {
  offset: number
  length: number
  lineNo: number
  line: string
}
const hits = ref<Hit[]>([])
const index = ref(0)
const hasMatches = computed(() => hits.value.length > 0)
const regexError = ref('')

/** 当前标签有没有可搜的编辑器实例（预览 / Hex / 挂载失败都没有）。 */
const searchable = computed(() => {
  const id = editor.activeId
  return id != null && !!get(id)
})

const countLabel = computed(() => {
  if (!query.value) return ''
  if (!hasMatches.value) return t('dialog.noMatches')
  return `${index.value + 1} ${t('dialog.ofTotal', { total: hits.value.length })}`
})

onMounted(async () => {
  await nextTick()
  findInput.value?.focus()
  // 打开时若有选中文本，命令层已把它塞进 seedText 当查询词。
  scan()
})

onUnmounted(() => {
  dirCancelled = true
})

watch([query, matchCase, wholeWord, regex, extended, inSelection], () => scan())
// 正文变了要重扫：命中偏移在改字之后全部失效，不重扫就会替换到错误位置。
// 依赖必须是版本号——getText 读的是普通 Map，读它建立不起响应依赖。
watch(
  () => {
    const id = editor.activeId
    return id == null ? 0 : docVersionOf(id)
  },
  () => scan(),
)

/**
 * 把「扩展模式」的转义序列还原成真实字符。
 *
 * 用户写 `\n` `\t` 是想搜真的换行与制表符，但普通模式下它们是两字符。
 * 正则模式下不做这层转换：那里 `\n` 已经是合法的正则转义。
 */
function expandEscapes(s: string): string {
  return s
    .replace(/\\r/g, '\r')
    .replace(/\\n/g, '\n')
    .replace(/\\t/g, '\t')
    .replace(/\\0/g, '\0')
    .replace(/\\x([0-9a-fA-F]{2})/g, (_, h: string) => String.fromCharCode(parseInt(h, 16)))
}

/** 建查找用的正则。三种模式的差别只在「查询词本身怎么解释」。 */
function buildRegex(): RegExp | null {
  regexError.value = ''
  const raw = query.value
  if (!raw) return null
  const q = extended.value && !regex.value ? expandEscapes(raw) : raw
  try {
    let src = regex.value ? q : q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
    if (wholeWord.value) src = `\\b(?:${src})\\b`
    return new RegExp(src, matchCase.value ? 'g' : 'gi')
  } catch (e) {
    // 正则写错要给出行内提示：弹个 toast 会打断打字，而用户此刻正在改表达式。
    regexError.value = (e as Error).message
    return null
  }
}

/** 扫一遍当前标签：算出全部命中偏移。 */
function scan() {
  const tab0 = editor.active
  if (!tab0 || !query.value || !searchable.value) {
    hits.value = []
    index.value = 0
    return
  }
  // 记下条件：F3 / Shift+F3 复用它，关掉浮窗后照样能跳。
  rememberQuery({
    query: query.value,
    matchCase: matchCase.value,
    wholeWord: wholeWord.value,
    regex: regex.value,
  })
  const re = buildRegex()
  if (!re) {
    hits.value = []
    return
  }
  const full = getText(tab0.id)
  // 「在选区内」只搜选区：范围外用 slice 截出来，偏移整体平移回原文坐标。
  let text = full
  let base = 0
  if (inSelection.value) {
    const v = get(tab0.id)
    if (!v) return
    const { from, to } = v.state.selection.main
    if (from === to) {
      hits.value = []
      return
    }
    text = full.slice(from, to)
    base = from
  }
  const found: Hit[] = []
  let m: RegExpExecArray | null
  let guard = 0
  while ((m = re.exec(text)) !== null && guard < 20000) {
    guard += 1
    const offset = base + m.index
    const length = m[0].length
    const lineStart = text.lastIndexOf('\n', m.index - 1) + 1
    let lineEnd = text.indexOf('\n', m.index)
    if (lineEnd < 0) lineEnd = text.length
    found.push({
      offset,
      length,
      lineNo: text.slice(0, m.index).split('\n').length,
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
  }
}

let paintedOnce = false

function jumpTo(i: number) {
  const tab0 = editor.active
  if (!tab0 || !hits.value[i]) return
  index.value = i
  const h = hits.value[i]
  doSelectRange(tab0.id, h.offset, h.offset + h.length)
}

function next() {
  if (!hasMatches.value) return
  const n = hits.value.length
  const nextIdx = wrapAround.value ? (index.value + 1) % n : Math.min(index.value + 1, n - 1)
  jumpTo(nextIdx)
}

function prev() {
  if (!hasMatches.value) return
  const n = hits.value.length
  // 「向上」不等于把列表倒过来走：命中的物理顺序不变，只是从当前位置往回找。
  const prevIdx = wrapAround.value
    ? (index.value - 1 + n) % n
    : Math.max(index.value - 1, 0)
  jumpTo(prevIdx)
}

/** 回车：替换模式走「替换当前」，查找模式走「下一个」。 */
function onEnter(e: Event | KeyboardEvent) {
  const ke = e as KeyboardEvent
  if (ke.ctrlKey || ke.metaKey) {
    replaceAll()
    return
  }
  if (backward.value) prev()
  else next()
}

function replaceOne() {
  const tab0 = editor.active
  const h = hits.value[index.value]
  if (!tab0 || !h) return
  // 用一次事务替换：撤销是「一步回到替换前」，而不是逐字回退。
  doApplyChanges(tab0.id, [{ from: h.offset, to: h.offset + h.length, insert: replacement.value }])
  const delta = replacement.value.length - h.length
  // 命中表不必重扫全文：后面的偏移量整体平移即可（比重新扫快得多）。
  hits.value = hits.value
    .filter((x) => x.offset !== h.offset)
    .map((x) => (x.offset > h.offset ? { ...x, offset: x.offset + delta } : x))
  if (index.value >= hits.value.length) index.value = 0
}

const replacing = ref(false)

function replaceAll() {
  const tab0 = editor.active
  if (!tab0 || !hits.value.length) return
  const count = hits.value.length
  replacing.value = true
  try {
    // 一次提交全部变更：撤销一次就回到替换前（逐个 dispatch 要按 N 次）。
    doApplyChanges(
      tab0.id,
      hits.value.map((h) => ({
        from: h.offset,
        to: h.offset + h.length,
        insert: replacement.value,
      })),
    )
    toast.ok(t('toast.replaced', { count }))
    scan()
  } finally {
    replacing.value = false
  }
}

/** 标记全部命中词：长文件里一眼定位同类内容。 */
function markAll() {
  if (!hasMatches.value) return
  for (const h of hits.value) {
    doMarkSelection(editor.activeId, markColor.value, h.offset, h.offset + h.length)
  }
  toast.ok(t('toast.marked', { count: hits.value.length }))
}

function clearMarks() {
  doClearMarks()
  toast.ok(t('toast.marksCleared'))
}

// ---- 目录查找 ----

async function pickDir() {
  const p = await api.openFolderDialog(dir.value || undefined)
  if (p) dir.value = p
}

/** 停止：协作式取消，正在读的那个文件读完就退。 */
function dirStop() {
  dirCancelled = true
  dirRunning.value = false
}

/**
 * 遍历 + 搜索。
 *
 * 并发上限 4：全并发会把 IO 打满，而编辑器还要响应用户操作。
 * 每 200 个文件让出一帧，进度数字才更新得了。
 */
async function runDir() {
  if (!dir.value || !query.value) {
    toast.warn(t('toast.needDirAndKeyword'))
    return
  }
  const re = buildRegex()
  if (!re) return
  dirRunning.value = true
  dirCancelled = false
  dirScanned.value = 0

  const out: Array<{ path: string; rel: string; line: string; lineNo: number }> = []
  const stack = [dir.value]
  const MAX_RESULTS = 5000

  while (stack.length && !dirCancelled && out.length < MAX_RESULTS) {
    const cur = stack.pop()!
    let entries
    try {
      entries = await api.readDir(cur)
    } catch {
      continue
    }
    for (const e of entries) {
      if (dirCancelled) break
      if (e.is_dir) {
        if (subdirs.value && !shouldSkipDir(e.name)) stack.push(e.path)
        continue
      }
      if (!shouldInclude(e.name)) continue
      if (e.size > 8 * 1024 * 1024) continue // 超大文件跳过：读它会把界面卡死
      try {
        const c = await api.readFile(e.path)
        const lines = c.text.split('\n')
        for (let i = 0; i < lines.length; i += 1) {
          re.lastIndex = 0
          if (re.test(lines[i])) {
            out.push({
              path: e.path,
              rel: e.path.slice(dir.value.length + 1),
              line: lines[i].slice(0, 200),
              lineNo: i + 1,
            })
            if (out.length >= MAX_RESULTS) break
          }
        }
      } catch {
        /* 读不了的文件跳过（权限 / 二进制） */
      }
      dirScanned.value += 1
      if (dirScanned.value % 200 === 0) {
        await new Promise((r) => setTimeout(r, 0))
      }
    }
  }

  dirRunning.value = false
  if (out.length >= MAX_RESULTS) {
    toast.warn(t('toast.tooManyResults', { n: MAX_RESULTS }))
  }
  if (!out.length) {
    toast.warn(t('dialog.noMatches'))
    return
  }
  // 落到侧栏面板：浮窗关掉后结果还在，用户能反复点开。
  findResults.set(groupByFile(out), out.length)
  tab.value = 'results'
}

/** 命中按文件归组：侧栏面板按这个结构渲染。 */
function groupByFile(list: Array<{ path: string; rel: string; line: string; lineNo: number }>): BatchFileHits[] {
  const byPath = new Map<string, { path: string; count: number; lines: number[]; sample: string }>()
  for (const h of list) {
    const cur = byPath.get(h.path)
    if (cur) {
      cur.count += 1
      if (cur.lines.length < 50) cur.lines.push(h.lineNo)
    } else {
      byPath.set(h.path, {
        path: h.path,
        count: 1,
        lines: [h.lineNo],
        sample: h.line.trim().slice(0, 120),
      })
    }
  }
  return [...byPath.values()].map((g) => ({ path: g.path, count: g.count, lines: g.lines, sample: g.sample }))
}

/** 常见不该进去翻的目录：翻它们只会让搜索慢十倍还多出一堆噪音命中。 */
function shouldSkipDir(name: string): boolean {
  const n = name.toLowerCase()
  return (
    n === '.git' ||
    n === 'node_modules' ||
    n === 'target' ||
    n === 'dist' ||
    n === 'build' ||
    n === '.svn' ||
    n === '.hg' ||
    n.startsWith('.')
  )
}

/** 文件名过滤：include / exclude 都支持 `*.md;*.txt` 这种分号写法。 */
function shouldInclude(name: string): boolean {
  const inc = splitPatterns(include.value)
  if (inc.length && !inc.some((p) => globMatch(p, name))) return false
  const exc = splitPatterns(exclude.value)
  if (exc.length && exc.some((p) => globMatch(p, name))) return false
  return true
}

function splitPatterns(s: string): string[] {
  return s
    .split(/[;,]/)
    .map((x) => x.trim())
    .filter(Boolean)
}

/** 结果行里给匹配上高亮。 */
function highlight(r: Hit): string {
  const esc = (s: string) =>
    s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  if (!query.value) return esc(r.line)
  // 在行内定位本次命中的位置：同一行可能有多处，取最近的一处。
  const needle = extended.value && !regex.value ? expandEscapes(query.value) : query.value
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

/* Tab 条：分段控件形态，当前项用主色软底 + 主色字。 */
.nt-find-tabs {
  display: flex;
  gap: 2px;
  flex: none;
  padding: 2px;
  background: var(--nt-surface-2);
  border-radius: var(--nt-radius-sm);
}

.nt-find-tab {
  flex: 1;
  height: var(--nt-ctl-h-sm);
  border-radius: var(--nt-radius-xs);
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-2);
  transition:
    background-color var(--nt-dur-fast) var(--nt-ease),
    color var(--nt-dur-fast) var(--nt-ease),
    transform var(--nt-dur-fast) var(--nt-ease);
}

.nt-find-tab:hover {
  color: var(--nt-ink);
  background: var(--nt-surface-hover);
}

.nt-find-tab.is-on {
  background: var(--nt-primary-soft);
  color: var(--nt-primary);
  font-weight: 600;
}

/* 命中计数：挂在输入框的 suffix 里，不占独立列。
   独立列会让同一组「查找 / 替换为」两行输入框宽度不等——
   一行有计数、一行没有，右端就参差。 */
.nt-find-count {
  flex: none;
  margin-left: var(--nt-sp-1);
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-muted);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  pointer-events: none;
}

.nt-find-err {
  font-size: var(--nt-fs-hint);
  color: var(--nt-danger);
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 不可搜视图的说明条：比灰按钮多一句「为什么」 */
.nt-find-unavailable {
  padding: var(--nt-sp-2) var(--nt-sp-3);
  border-radius: var(--nt-radius-sm);
  background: var(--nt-surface-2);
  font-size: var(--nt-fs-label);
  color: var(--nt-ink-muted);
}

.nt-find-mark-hint {
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-muted);
  min-width: 0;
}

/* 选项区：四列等宽栅格。
   flex-wrap 会让每一项按自身文字宽度排，两行的列头对不上；
   栅格把「区分大小写 / 全字匹配 / 正则表达式」固定在同一列上。 */
.nt-find-flags {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  align-items: center;
  gap: var(--nt-sp-2) var(--nt-sp-3);
}

.nt-find-flags :deep(.el-checkbox) {
  margin-right: 0;
  min-width: 0;
  height: auto;
}

.nt-find-flags :deep(.el-checkbox__label) {
  font-size: var(--nt-fs-label);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 目录查找那一行的按钮组要独占一行、靠右 */
.nt-find-flags > .nt-find-actions-right {
  grid-column: 1 / -1;
  justify-self: end;
}

.nt-find-actions {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  min-width: 0;
}

.nt-find-actions-right {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  flex: none;
}

/* 标记色板：五个色点 + 选中环，与编辑器里 5 色标记同一组令牌。 */
.nt-find-marks {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
}

.nt-find-mark-swatch {
  width: var(--nt-ctl-h-sm);
  height: var(--nt-ctl-h-sm);
  border-radius: var(--nt-radius-xs);
  background: var(--swatch);
  border: 2px solid transparent;
  transition:
    transform var(--nt-dur-fast) var(--nt-ease),
    border-color var(--nt-dur-fast) var(--nt-ease);
}

.nt-find-mark-swatch:hover {
  transform: scale(1.06);
}

.nt-find-mark-swatch.is-on {
  border-color: var(--nt-ink);
}

/* 目录查找的结果落在侧栏面板：浮窗关掉后还能反复点开。 */
.nt-find-panel-link {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
  padding-top: var(--nt-sp-2);
  border-top: 1px solid var(--nt-border);
}

.nt-find-results {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-2);
  min-height: 0;
}

.nt-find-results-body {
  max-height: 320px;
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
  font-family: var(--font-mono);
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
