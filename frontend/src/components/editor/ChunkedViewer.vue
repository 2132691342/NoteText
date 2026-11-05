// 大文件分块视图：一次只把一块（~1MB）装进编辑器，可按块编辑并写回。
// 与 CodeEditor 分开：这里的正文不进注册表、没有全量保存链路。编辑只作用于当前块，写回时后端按字节区间替换。
<template>
  <div class="nt-chunked">
    <div class="nt-chunked-bar">
      <ElButtonGroup>
        <ElButton size="small" :disabled="loading || blockIdx <= 0" @click="go(-1)">
          <NtIcon name="chevron-up" size="sm" />
        </ElButton>
        <ElButton size="small" :disabled="loading || blockIdx >= blockCount - 1" @click="go(1)">
          <NtIcon name="chevron-down" size="sm" />
        </ElButton>
      </ElButtonGroup>
      <span class="nt-readout">
        {{ t('chunked.block') }} {{ blockCount > 0 ? blockIdx + 1 : 0 }} / {{ blockCount }}
        <template v-if="blockInfo"> · {{ t('chunked.fromLine') }} {{ blockInfo.start_line }}</template>
      </span>
      <span class="nt-chunked-spacer" />
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

    <!-- 查找条：GB 级文件不能靠肉眼翻块，流式扫描一轮一轮往下要 -->
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

    <div v-show="!progress && !error" ref="host" class="nt-chunked-body" />
  </div>
</template>

<script lang="ts" setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { ElButton, ElButtonGroup, ElCheckbox, ElInput } from 'element-plus'
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
import NtIcon from '@/components/common/NtIcon.vue'

const props = defineProps<{ tab: Tab }>()
const { t } = useI18n()
const editor = useEditorStore()
const toast = useToastStore()

/** 元信息缓存：同一路径重复打开不重建索引。 */
const metaCache = new Map<string, BigFileMetaVO>()

const meta = ref<BigFileMetaVO | null>(null)
const blockIdx = ref(0)
const blockStartLine = ref(1)
const loading = ref(false)
const error = ref('')
/** 当前块的原文快照：脏标记与乐观锁基线都用它。 */
const blockBaseline = ref('')
/** 建索引进度（done/total 字节）；null = 不在扫描。 */
const progress = ref<{ done: number; total: number } | null>(null)
const gotoInput = ref('')
const blockInfo = computed(() =>
  meta.value && blockIdx.value < meta.value.block_count
    ? { start_line: blockStartLine.value }
    : null,
)
const blockCount = computed(() => meta.value?.block_count ?? 0)
const progressPct = computed(() =>
  progress.value && progress.value.total > 0
    ? `${Math.min(100, Math.round((progress.value.done / progress.value.total) * 100))}%`
    : '0%',
)

/** 编辑态：解锁当前块。 */
const editing = ref(false)
const saving = ref(false)
/** 当前块是否有未保存的改动。 */
const blockDirty = ref(false)

const host = ref<HTMLElement | null>(null)
const editableComp = new Compartment()
let view: EditorView | null = null
let unlistenProgress: UnlistenFn | null = null

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

/** 切换编辑态。切到只读前若有未保存改动，先问一句。 */
async function toggleEdit() {
  if (editing.value) {
    if (blockDirty.value) {
      const keep = await api.showConfirm(
        t('chunked.discardTitle'),
        t('chunked.discardBody'),
      )
      if (!keep) return
    }
    // 退回只读：把正文恢复成基线，避免「看起来改了但没存」。
    setDoc(blockBaseline.value)
    editing.value = false
    view?.dispatch({ effects: editableComp.reconfigure(EditorState.readOnly.of(true)) })
  } else {
    editing.value = true
    view?.dispatch({ effects: editableComp.reconfigure([]) })
    view?.focus()
  }
}

/** 丢弃当前块的改动。 */
function discardBlock() {
  setDoc(blockBaseline.value)
}

/** 写回当前块。成功后重建索引并按新块数刷新视图。 */
async function saveBlock() {
  if (!meta.value || !props.tab.path || !blockDirty.value) return
  saving.value = true
  try {
    const res = await api.bigFile.writeBlock({
      path: props.tab.path,
      block_index: blockIdx.value,
      old_text: blockBaseline.value,
      new_text: currentText(),
    })
    // 索引被重建了：块数与总行数都可能变。
    meta.value = {
      ...meta.value,
      size: res.size,
      block_count: res.block_count,
      total_lines: res.total_lines,
    }
    blockBaseline.value = currentText()
    blockDirty.value = false
    toast.ok(t('chunked.savedBlock', { name: props.tab.title }))
  } catch (e) {
    const err = normalizeError(e)
    // 乐观锁失败（1008）：磁盘上这块已经变了，提示重新加载这一块。
    if (err.code === '1008') {
      toast.warn(t('chunked.staleBlock'))
      await loadBlock(blockIdx.value)
    } else {
      toast.bad(t('error.writeFailed', { reason: err.message }))
    }
  } finally {
    saving.value = false
  }
}

/** 加载一个块（越界自动钳制）。cursorLine 是全局行号，装入时换算成块内行。 */
async function loadBlock(i: number, globalLine?: number) {
  if (!meta.value || !props.tab.path) return
  // 换块前有未保存的改动：问一句，不替用户丢内容。
  if (blockDirty.value) {
    const ok = await api.showConfirm(t('chunked.unsavedTitle'), t('chunked.unsavedBody'))
    if (!ok) return
  }
  const idx = Math.max(0, Math.min(i, meta.value.block_count - 1))
  loading.value = true
  try {
    const b = await api.bigFile.block(props.tab.path, idx)
    blockIdx.value = b.block_index
    blockStartLine.value = b.start_line
    const localLine = globalLine != null ? globalLine - b.start_line + 1 : undefined
    setDoc(b.text, localLine)
    if (b.stale) toast.warn(t('toast.bigFileStale', { name: props.tab.title }))
  } catch (e) {
    error.value = normalizeError(e).message
  } finally {
    loading.value = false
  }
}

/** 前一块 / 后一块。 */
function go(delta: number) {
  void loadBlock(blockIdx.value + delta)
}

/** 跳转到全局行号：定位所在块后把光标放到块内对应行。 */
async function onGoto() {
  if (!meta.value || !props.tab.path) return
  const n = Number.parseInt(gotoInput.value, 10)
  if (!Number.isFinite(n) || n < 1) return
  const line = Math.min(n, meta.value.total_lines)
  try {
    const idx = await api.bigFile.gotoLine(props.tab.path, line)
    if (idx !== blockIdx.value) {
      await loadBlock(idx, line)
    } else {
      // 已在目标块：光标直接落到块内对应行。
      if (view) {
        const local = Math.max(1, Math.min(line - blockStartLine.value + 1, view.state.doc.lines))
        const l = view.state.doc.line(local)
        view.dispatch({ selection: { anchor: l.from }, scrollIntoView: true })
      }
      syncCursorToTab()
    }
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
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

/** 跳到第 n 条命中所在的块，并把光标落到那一行。 */
async function jumpToHit(n: number) {
  const hit = findHits.value[n]
  if (!hit) return
  findCursor.value = n
  const idx = await api.bigFile.gotoLine(props.tab.path, hit.line)
  await loadBlock(idx, hit.line)
  // 命中行整行选中：只把光标放过去，用户还得自己找词在哪。
  if (view) {
    const local = Math.max(1, Math.min(hit.line - blockStartLine.value + 1, view.state.doc.lines))
    const line = view.state.doc.line(local)
    const from = Math.min(line.from + hit.col - 1, line.to)
    const to = Math.min(from + findText.value.length, line.to)
    view.dispatch({ selection: { anchor: from, head: to }, scrollIntoView: true })
    view.focus()
  }
}

onMounted(async () => {
  if (!host.value || !props.tab.path) return
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

  try {
    const cached = metaCache.get(props.tab.path)
    meta.value = cached ?? (await openWithProgress(props.tab.path))
    if (meta.value) {
      metaCache.set(props.tab.path, meta.value)
      // 恢复会话时回到上次的位置。
      await loadBlock(0, props.tab.position.line > 1 ? props.tab.position.line : undefined)
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

.nt-chunked-body {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
}

.nt-chunked-body :deep(.cm-editor) {
  flex: 1;
}
</style>
