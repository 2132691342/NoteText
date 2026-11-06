// 大文件分块视图：浏览走虚拟滚动网格（后端 big_file_lines 分页取行，全文件连续滚动），
// 编辑保留「单块装进 CodeMirror + 乐观锁写回」的局部编辑链路。
// 与 CodeEditor 分开：这里的正文不进注册表、没有全量保存链路。
<template>
  <div class="nt-chunked">
    <div class="nt-chunked-bar">
      <ElInput
        v-model="gotoInput"
        size="small"
        class="nt-chunked-goto"
        :placeholder="t('chunked.gotoPlaceholder')"
        @keydown.enter.prevent="onGoto"
      >
        <template #append>
          <ElButton size="small" :disabled="loading" @click="onGoto">
            {{ t('chunked.goto') }}
          </ElButton>
        </template>
      </ElInput>
      <span class="nt-readout">
        {{ t('chunked.position', { n: currentLineNo, total: totalLines }) }}
      </span>
      <span class="nt-chunked-spacer" />
      <!-- 编辑开关：默认只读浏览，用户明确要改这块时才解锁 -->
      <ElButton
        size="small"
        :type="editing ? 'primary' : 'default'"
        :disabled="loading || !!progress || !!error"
        :title="t('chunked.editHint')"
        @click="toggleEdit"
      >
        {{ editing ? t('chunked.editing') : t('chunked.editBlock') }}
      </ElButton>
      <ElButton
        v-if="editing"
        size="small"
        :disabled="!blockDirty || loading"
        :loading="saving"
        @click="saveBlock"
      >
        {{ t('chunked.saveBlock') }}
      </ElButton>
      <ElButton
        v-if="editing"
        size="small"
        text
        :disabled="!blockDirty || saving"
        @click="discardBlock"
      >
        {{ t('chunked.discardBlock') }}
      </ElButton>
      <ElButton size="small" :title="t('chunked.find')" @click="toggleFind">
        <NtIcon name="search" size="sm" />
      </ElButton>
      <span class="nt-readout nt-chunked-ro">
        {{ editing ? t('chunked.editingWarn') : t('chunked.readonly') }}
      </span>
    </div>

    <!-- 查找条：GB 级文件不能靠肉眼滚动，流式扫描一轮一轮往下要 -->
    <div v-if="findOpen" class="nt-chunked-find">
      <ElInput
        ref="findInput"
        v-model="findText"
        size="small"
        class="nt-chunked-find-input"
        :placeholder="t('chunked.findPlaceholder')"
        clearable
        @keydown.enter.prevent="startFind"
      />
      <ElButton size="small" :loading="finding" :disabled="!findText" @click="startFind">
        {{ t('chunked.find') }}
      </ElButton>
      <ElButton
        size="small"
        :disabled="finding || !findHits.length"
        @click="stepFind(1)"
      >
        {{ t('chunked.findNext') }}
      </ElButton>
      <ElButton
        size="small"
        :disabled="finding || !findHits.length"
        @click="stepFind(-1)"
      >
        {{ t('chunked.findPrev') }}
      </ElButton>
      <ElCheckbox v-model="findCase" size="small" :disabled="finding">
        {{ t('chunked.findCase') }}
      </ElCheckbox>
      <span class="nt-readout">
        <template v-if="finding">{{ t('chunked.finding') }}</template>
        <template v-else-if="findHits.length">
          {{ findCursor + 1 }} / {{ findHits.length }}{{ findMore ? '+' : '' }}
        </template>
        <template v-else-if="findDone">{{ t('chunked.noMatch') }}</template>
      </span>
      <span class="nt-chunked-spacer" />
      <ElButton size="small" text @click="findOpen = false">{{ t('common.close') }}</ElButton>
    </div>

    <!-- 建索引进度：GB 级文件要顺序扫一遍，几秒钟内给得出可信反馈 -->
    <div v-if="progress" class="nt-chunked-progress">
      <div class="nt-chunked-progress-bar">
        <div class="nt-chunked-progress-fill" :style="{ width: progressPct }" />
      </div>
      <div class="nt-chunked-progress-row">
        <span>{{ t('chunked.indexing') }}</span>
        <span class="nt-readout">{{ progressPct }}</span>
        <ElButton size="small" text @click="onCancel">{{ t('common.cancel') }}</ElButton>
      </div>
    </div>

    <div v-else-if="error" class="nt-mount-error">
      <span class="nt-mount-error-title">{{ t('editor.openFailedTitle') }}</span>
      <span class="nt-mount-error-detail">{{ error }}</span>
    </div>

    <!--
      浏览模式：虚拟滚动网格。ref 必须叫 scrollHost——useBigFileGrid 用它量视口。
      tabindex 让方向键 / PageUp / PageDown 走浏览器原生滚动。
    -->
    <div
      v-show="!progress && !error && !editing"
      ref="scrollHost"
      class="nt-chunked-body"
      tabindex="0"
      @wheel="onWheel"
      @scroll="onScroll"
    >
      <div class="nt-chunked-grid" :style="{ height: `${scrollHeight}px` }">
        <div class="nt-chunked-window" :style="{ transform: `translateY(${windowTop}px)` }">
          <div
            v-for="row in windowRows"
            :key="row.index"
            class="nt-chunked-row"
            :class="{ 'is-hit': row.lineNo === hitLine }"
          >
            <span class="nt-chunked-gutter">{{ row.lineNo }}</span>
            <span
              v-if="row.cached"
              class="nt-chunked-text"
              :class="{ 'is-clipped': row.clipped }"
              :title="titleOf(row.text)"
            >
              {{ row.text }}
            </span>
            <span v-else class="nt-chunked-text is-pending" />
          </div>
        </div>
        <div v-if="isEmpty" class="nt-chunked-empty">{{ t('chunked.emptyFile') }}</div>
      </div>
    </div>

    <!-- 编辑模式：当前块装进只属于这一块的 CodeMirror 实例 -->
    <div v-show="editing && !progress && !error" ref="host" class="nt-chunked-body" />
  </div>
</template>

<script lang="ts" setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { ElButton, ElCheckbox, ElInput } from 'element-plus'
import { EditorState, Compartment } from '@codemirror/state'
import {
  EditorView,
  drawSelection,
  dropCursor,
  highlightActiveLine,
  highlightSpecialChars,
  keymap,
  lineNumbers,
} from '@codemirror/view'
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands'
import { syntaxHighlighting, defaultHighlightStyle } from '@codemirror/language'
import { api, normalizeError } from '@/api'
import type { BigFileHitVO, BigFileMetaVO } from '@/types/api'
import type { Tab } from '@/stores/editor'
import { useEditorStore } from '@/stores/editor'
import { useToastStore } from '@/stores/toast'
import { useBigFileGrid } from '@/composables/useBigFileGrid'
import NtIcon from '@/components/common/NtIcon.vue'

const props = defineProps<{ tab: Tab }>()
const { t } = useI18n()
const editor = useEditorStore()
const toast = useToastStore()

/** 元信息缓存：同一路径重复打开不重建索引。 */
const metaCache = new Map<string, BigFileMetaVO>()

/** 进块编辑前的卡顿提示阈值：索引统计出的最长行超过它就提醒一句。 */
const WIDE_EDIT_WARN_BYTES = 256 * 1024
/** 悬浮提示里一行的字符上限：再长 DOM 属性也吃不消。 */
const TITLE_CAP = 500

const loading = ref(false)
const error = ref('')
/** 建索引进度（done/total 字节）；null = 不在扫描。 */
const progress = ref<{ done: number; total: number } | null>(null)
const gotoInput = ref('')
/** 查找命中的高亮行：浏览模式下命中行整行高亮。 */
const hitLine = ref<number | null>(null)
const progressPct = computed(() =>
  progress.value && progress.value.total > 0
    ? `${Math.min(100, Math.round((progress.value.done / progress.value.total) * 100))}%`
    : '0%',
)

// ---- 浏览网格 ----

const grid = useBigFileGrid({ path: computed(() => props.tab.path) })
const {
  meta,
  totalLines,
  isEmpty,
  scrollHost,
  scrollHeight,
  windowTop,
  windowRows,
  onWheel,
  onScroll,
  scrollToLine,
  currentLine,
  setMeta,
  resetPages,
} = grid

const currentLineNo = computed(() => (editing.value ? blockStartLine.value : currentLine()))

/** 悬浮提示截断：一行的完整内容走复制或块编辑，title 只负责「这是什么」。 */
function titleOf(text: string): string {
  return text.length > TITLE_CAP ? `${text.slice(0, TITLE_CAP)}…` : text
}

// ---- 编辑态：单块装进 CodeMirror ----

const editing = ref(false)
const saving = ref(false)
/** 当前块是否有未保存的改动。 */
const blockDirty = ref(false)
const blockIdx = ref(0)
const blockStartLine = ref(1)
/** 当前块的原文快照：脏标记与乐观锁基线都用它。 */
const blockBaseline = ref('')

const host = ref<HTMLElement | null>(null)
const editableComp = new Compartment()
let view: EditorView | null = null
let unlistenProgress: UnlistenFn | null = null

/** CodeMirror 实例懒创建：只读浏览的用户不该为编辑器实例付初始化钱。 */
function ensureView() {
  if (view || !host.value) return
  view = new EditorView({
    state: EditorState.create({
      doc: '',
      extensions: [
        history(),
        drawSelection(),
        dropCursor(),
        highlightSpecialChars(),
        highlightActiveLine(),
        lineNumbers(),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        keymap.of([
          ...defaultKeymap,
          ...historyKeymap,
          // Ctrl+S 只保存当前块：这个视图没有「整篇保存」的概念。
          { key: 'Mod-s', preventDefault: true, run: () => (void saveBlock(), true) },
        ]),
        editableComp.of(EditorState.readOnly.of(true)),
        EditorView.updateListener.of((vu) => {
          if (vu.docChanged) {
            blockDirty.value = currentText() !== blockBaseline.value
          }
          if (vu.selectionSet || vu.docChanged) syncCursorToTab()
        }),
      ],
    }),
    parent: host.value,
  })
}

/** 把块文本装进视图，光标可选落在块内某一行。 */
function setDoc(text: string, cursorLine?: number) {
  if (!view) return
  const state = view.state
  let anchor = 0
  if (cursorLine != null && cursorLine > 1) {
    const line = state.doc.line(Math.min(cursorLine, state.doc.lines))
    anchor = line.from
  }
  view.dispatch({ changes: { from: 0, to: state.doc.length, insert: text }, selection: { anchor } })
  blockBaseline.value = text
  blockDirty.value = false
}

/** 当前块正文。 */
function currentText(): string {
  return view?.state.doc.toString() ?? ''
}

/** 进入编辑：定位到当前可见行所在块并装入 CodeMirror。 */
async function toggleEdit() {
  if (editing.value) {
    if (blockDirty.value) {
      const keep = await api.showConfirm(t('chunked.discardTitle'), t('chunked.discardBody'))
      if (!keep) return
    }
    // 退回只读：把正文恢复成基线，避免「看起来改了但没存」。
    setDoc(blockBaseline.value)
    editing.value = false
    view?.dispatch({ effects: editableComp.reconfigure(EditorState.readOnly.of(true)) })
    return
  }
  if (!props.tab.path) return
  loading.value = true
  try {
    const line = currentLine()
    const idx = await api.bigFile.gotoLine(props.tab.path, line)
    const b = await api.bigFile.block(props.tab.path, idx)
    ensureView()
    blockIdx.value = b.block_index
    blockStartLine.value = b.start_line
    const local = Math.max(1, line - b.start_line + 1)
    setDoc(b.text, local)
    if (b.stale) toast.warn(t('toast.bigFileStale', { name: props.tab.title }))
    // 索引统计出的最长行超阈值：提醒卡顿风险，编辑主动权仍在用户。
    if ((meta.value?.max_line_bytes ?? 0) > WIDE_EDIT_WARN_BYTES) {
      toast.warn(t('chunked.wideEditWarn'))
    }
    editing.value = true
    view?.dispatch({ effects: editableComp.reconfigure([]) })
    await nextTick()
    view?.focus()
  } catch (e) {
    error.value = normalizeError(e).message
  } finally {
    loading.value = false
  }
}

/** 丢弃当前块的改动。 */
function discardBlock() {
  setDoc(blockBaseline.value)
}

/** 写回当前块。成功后索引被重建，网格页缓存全部作废。 */
async function saveBlock() {
  if (!props.tab.path || !blockDirty.value) return
  saving.value = true
  try {
    const res = await api.bigFile.writeBlock({
      path: props.tab.path,
      block_index: blockIdx.value,
      old_text: blockBaseline.value,
      new_text: currentText(),
    })
    blockBaseline.value = currentText()
    blockDirty.value = false
    toast.ok(t('chunked.savedBlock', { name: props.tab.title }))
    // 索引重建后块数与总行数都可能变；网格留在保存前用户看到的行。
    const at = blockStartLine.value
    editing.value = false
    resetPages(
      meta.value
        ? {
            ...meta.value,
            size: res.size,
            block_count: res.block_count,
            total_lines: res.total_lines,
          }
        : null,
      at,
    )
  } catch (e) {
    const err = normalizeError(e)
    // 乐观锁失败（1008）：磁盘上这块已经变了，提示重新加载这一块。
    if (err.code === '1008') {
      toast.warn(t('chunked.staleBlock'))
      await reloadEditingBlock()
    } else {
      toast.bad(t('error.writeFailed', { reason: err.message }))
    }
  } finally {
    saving.value = false
  }
}

/** 乐观锁失败后重读当前块，恢复基线。 */
async function reloadEditingBlock() {
  if (!props.tab.path || !view) return
  try {
    const b = await api.bigFile.block(props.tab.path, blockIdx.value)
    setDoc(b.text)
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}

/** 跳转到全局行号：网格滚过去，命中行短暂高亮。 */
async function onGoto() {
  if (!meta.value || !props.tab.path) return
  const n = Number.parseInt(gotoInput.value, 10)
  if (!Number.isFinite(n) || n < 1) return
  const line = Math.min(n, totalLines.value)
  hitLine.value = null
  scrollToLine(line)
  syncGridCursor()
}

async function onCancel() {
  if (!props.tab.path) return
  try {
    await api.bigFile.cancel(props.tab.path)
  } catch {
    /* 取消失败让扫描自然结束 */
  }
}

// ---- 查找：逐块流式扫描，一轮一页 ----

const findOpen = ref(false)
const findText = ref('')
const findCase = ref(false)
const finding = ref(false)
/** 已拿到的命中（可能只是前几页）。 */
const findHits = ref<BigFileHitVO[]>([])
const findCursor = ref(0)
/** 后端还有没扫完的部分：状态栏显示 `n+`。 */
const findMore = ref(false)
/** 至少跑过一轮且没有命中：用于区分「还没查」与「查过没找到」。 */
const findDone = ref(false)
/** 下一轮的扫描起点（字节偏移）。 */
const nextOffset = ref(0)
const findInput = ref<{ focus: () => void } | null>(null)

/** 每轮扫描的字节预算：4MB 在机械盘上也是毫秒级，用户点一次就有反馈。 */
const FIND_BUDGET = 4 * 1024 * 1024
/** 一次最多取多少条命中：再多列表也没人翻。 */
const FIND_LIMIT = 500

function toggleFind() {
  findOpen.value = !findOpen.value
  if (findOpen.value) void nextTick(() => findInput.value?.focus())
}

/** 从头开始查：重置游标与命中表。 */
async function startFind() {
  if (!findText.value || !props.tab.path) return
  findHits.value = []
  findCursor.value = 0
  nextOffset.value = 0
  findMore.value = false
  findDone.value = false
  await scanRound()
}

/** 继续扫描一轮（后端按 budget 分批，一轮不满意可以再点「下一个」）。 */
async function scanRound() {
  if (!props.tab.path || !findText.value) return
  finding.value = true
  try {
    const res = await api.bigFile.find({
      path: props.tab.path,
      query: findText.value,
      match_case: findCase.value,
      whole_word: false,
      from_offset: nextOffset.value,
      limit: FIND_LIMIT,
      budget: FIND_BUDGET,
    })
    const hits = res.hits ?? []
    if (hits.length > 0) findHits.value = [...findHits.value, ...hits]
    nextOffset.value = res.next_offset
    findMore.value = res.has_more
    findDone.value = true
  } catch (e) {
    toast.bad(normalizeError(e).message)
    findMore.value = false
  } finally {
    finding.value = false
  }
}

/**
   * 上一个 / 下一个命中。
   * 游标走到已取结果的尽头而文件还没扫完时，先补一轮扫描再前进。
   */
async function stepFind(delta: number) {
  if (!findHits.value.length) {
    await startFind()
    if (findHits.value.length) await jumpToHit(0)
    return
  }
  const next = findCursor.value + delta
  if (next < 0) {
    findCursor.value = findHits.value.length - 1
  } else if (next >= findHits.value.length) {
    if (findMore.value) {
      await scanRound()
      findCursor.value = Math.min(next, findHits.value.length - 1)
    } else {
      findCursor.value = 0
    }
  } else {
    findCursor.value = next
  }
  await jumpToHit(findCursor.value)
}

/** 跳到第 n 条命中所在行：网格滚动过去并整行高亮。 */
async function jumpToHit(n: number) {
  const hit = findHits.value[n]
  if (!hit) return
  findCursor.value = n
  hitLine.value = hit.line
  scrollToLine(hit.line)
  syncGridCursor()
}

// ---- 光标与状态栏联动 ----

/** 浏览模式的光标 = 视口顶部行。滚动 / 跳转 / 预取到达都可能改变它。 */
function syncGridCursor() {
  if (editing.value) return
  const line = currentLine()
  editor.setPosition(props.tab.id, { line, col: 1, selection_len: 0 })
  document.dispatchEvent(
    new CustomEvent('nt-cursor', {
      detail: { tabId: props.tab.id, line, col: 1, selection_len: 0 },
    }),
  )
}

watch(
  () => grid.firstVisible.value,
  () => syncGridCursor(),
)

onMounted(async () => {
  if (!props.tab.path) return
  try {
    const cached = metaCache.get(props.tab.path)
    meta.value = cached ?? (await openWithProgress(props.tab.path))
    if (meta.value) {
      metaCache.set(props.tab.path, meta.value)
      // 恢复会话时回到上次的位置。
      setMeta(meta.value, props.tab.position.line > 1 ? props.tab.position.line : 1)
      syncGridCursor()
    }
  } catch (e) {
    error.value = normalizeError(e).message
  }
})

/** 打开（建索引）并接住进度事件。 */
async function openWithProgress(path: string): Promise<BigFileMetaVO> {
  progress.value = { done: 0, total: 0 }
  unlistenProgress = await listen<{ path: string; done: number; total: number }>(
    'bigfile:progress',
    (ev) => {
      if (ev.payload.path !== path) return
      progress.value = { done: ev.payload.done, total: ev.payload.total }
    },
  )
  try {
    return await api.bigFile.open(path)
  } finally {
    unlistenProgress?.()
    unlistenProgress = null
    progress.value = null
  }
}

/** 光标换算成全局行号写回标签：块内行 + 块起始行 - 1。 */
function syncCursorToTab() {
  if (!view) return
  const pos = view.state.selection.main
  const line = view.state.doc.lineAt(pos.head)
  editor.setPosition(props.tab.id, {
    line: blockStartLine.value + line.number - 1,
    col: pos.head - line.from + 1,
    selection_len: pos.to - pos.from,
  })
  document.dispatchEvent(
    new CustomEvent('nt-cursor', {
      detail: {
        tabId: props.tab.id,
        line: blockStartLine.value + line.number - 1,
        col: pos.head - line.from + 1,
        selection_len: pos.to - pos.from,
      },
    }),
  )
}

onBeforeUnmount(() => {
  unlistenProgress?.()
  view?.destroy()
  view = null
  // 关标签即释放后端索引：内存表不该跟着打开过的文件无限增长。
  if (props.tab.path && editor.findByPath(props.tab.path)?.id === props.tab.id) {
    void api.bigFile.close(props.tab.path).catch(() => {})
  }
})
</script>

<style scoped>
.nt-chunked {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  min-width: 0;
}

.nt-chunked-bar {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  flex: none;
  height: var(--nt-h-toolbar);
  padding: 0 var(--nt-sp-2);
  border-bottom: 1px solid var(--nt-border);
  background: var(--nt-elev-surface);
}

.nt-chunked-spacer {
  flex: 1;
}

.nt-chunked-goto {
  width: 200px;
}

.nt-chunked-ro {
  color: var(--nt-ink-muted);
}

.nt-chunked-find {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  flex: none;
  height: var(--nt-h-toolbar);
  padding: 0 var(--nt-sp-2);
  border-bottom: 1px solid var(--nt-border);
  background: var(--nt-elev-raised);
}

.nt-chunked-find-input {
  width: 220px;
}

.nt-chunked-progress {
  display: flex;
  flex: 1;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--nt-sp-3);
}

.nt-chunked-progress-bar {
  width: min(420px, 70%);
  height: 6px;
  border-radius: var(--nt-radius-xs);
  background: var(--nt-elev-hover);
  overflow: hidden;
}

.nt-chunked-progress-fill {
  height: 100%;
  background: var(--nt-primary);
  transition: width var(--nt-dur-fast) var(--nt-ease);
}

.nt-chunked-progress-row {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-2);
}

/* 浏览网格：滚动容器 + 压缩高度的画布 + 绝对定位窗口（与大表格同构） */
.nt-chunked-body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  position: relative;
  outline: none;
  font-size: var(--nt-fs-sm);
}

.nt-chunked-grid {
  position: relative;
  min-width: 100%;
  font-family: var(--nt-font-mono, monospace);
}

.nt-chunked-window {
  position: absolute;
  left: 0;
  right: 0;
  top: 0;
}

.nt-chunked-row {
  display: flex;
  height: 26px;
}

.nt-chunked-row.is-hit {
  background: var(--nt-warning-bg);
}

.nt-chunked-gutter {
  flex: none;
  width: 72px;
  padding: 0 var(--nt-sp-2);
  display: flex;
  align-items: center;
  justify-content: flex-end;
  color: var(--nt-ink-muted);
  background: var(--nt-editor-gutter-bg);
  border-bottom: 1px solid var(--nt-border);
  border-right: 1px solid var(--nt-border);
  font-size: var(--nt-fs-2xs);
  user-select: none;
}

.nt-chunked-text {
  flex: 1;
  min-width: 0;
  padding: 0 var(--nt-sp-2);
  display: flex;
  align-items: center;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  color: var(--nt-ink-2);
  border-bottom: 1px solid var(--nt-border);
}

/* 超宽行被截断：尾部补一个省略标记，告诉用户这不是完整内容 */
.nt-chunked-text.is-clipped::after {
  content: ' …';
  color: var(--nt-ink-muted);
  flex: none;
}

/* 页还没到：低频呼吸占位，几何不变（行高固定，不跳） */
.nt-chunked-text.is-pending {
  background: var(--nt-elev-hover);
  animation: nt-chunked-pending 1.2s ease-in-out infinite;
}

@keyframes nt-chunked-pending {
  50% {
    opacity: 0.5;
  }
}

.nt-chunked-empty {
  padding: var(--nt-sp-4);
  color: var(--nt-ink-muted);
  font-size: var(--nt-fs-sm);
}

/* 编辑态的 CodeMirror 容器 */
.nt-chunked-body :deep(.cm-editor) {
  height: 100%;
}
</style>
