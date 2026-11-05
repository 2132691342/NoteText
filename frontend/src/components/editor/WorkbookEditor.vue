<!-- XLSX 可编辑网格：值与公式可改，样式由后端「原地改 XML」保住。
     分页取数 + 虚拟滚动：百万行的工作表也不整块进内存。 -->
<template>
  <div class="nt-sheet">
    <div class="nt-sheet-bar">
      <ElInput
        ref="findInput"
        v-model="findText"
        size="small"
        class="nt-sheet-find"
        :placeholder="t('sheet.findPlaceholder')"
        clearable
        @keydown.enter.prevent="runFind"
      >
        <template #append>
          <ElButton size="small" :disabled="!findText" @click="runFind">
            {{ t('sheet.find') }}
          </ElButton>
        </template>
      </ElInput>
      <ElButton size="small" :disabled="!findHits.length" @click="stepFind(1)">
        {{ t('sheet.findNext') }}
      </ElButton>
      <ElButton size="small" :disabled="!findHits.length" @click="stepFind(-1)">
        {{ t('sheet.findPrev') }}
      </ElButton>
      <span class="nt-toolbar-sep" />
      <ElInput
        ref="gotoEl"
        v-model="gotoInput"
        size="small"
        class="nt-sheet-goto"
        :placeholder="t('sheet.gotoRowPlaceholder')"
        @keydown.enter.prevent="onGoto"
      >
        <template #append>
          <ElButton size="small" :disabled="!gotoInput" @click="onGoto">
            {{ t('sheet.gotoRow') }}
          </ElButton>
        </template>
      </ElInput>
      <span class="nt-toolbar-spacer" />
      <span class="nt-readout nt-sheet-meta">
        {{ t('sheet.rows', { n: totalRows }) }} · {{ t('sheet.cols', { n: totalCols }) }} ·
        {{ t('sheet.sheets', { n: sheets.length }) }}
        <template v-if="dirtyCount > 0">
          · <span class="nt-sheet-dirty">{{ t('sheet.pending', { n: dirtyCount }) }}</span>
        </template>
      </span>
      <ElButton
        size="small"
        type="primary"
        :disabled="dirtyCount === 0 || saving"
        :loading="saving"
        @click="save"
      >
        {{ t('sheet.saveWorkbook') }}
      </ElButton>
      <ElButton size="small" text :disabled="dirtyCount === 0 || saving" @click="discard">
        {{ t('sheet.discardShort') }}
      </ElButton>
    </div>

    <div v-if="error" class="nt-mount-error">
      <span class="nt-mount-error-title">{{ t('editor.openFailedTitle') }}</span>
      <span class="nt-mount-error-detail">{{ error }}</span>
    </div>

    <div
      v-else-if="loading"
      class="nt-sheet-loading"
    >
      <span class="nt-spinner" />
      <span class="nt-readout">{{ t('sheet.indexing') }}</span>
    </div>

    <!--
      ref 必须叫 scrollHost：useSheetGrid 用这个名字量视口高度。
      绑错名字 → viewport 恒为 0 → 渲染窗口只剩 overscan 那几行（25 行）。
    -->
    <div v-else ref="scrollHost" class="nt-sheet-body" @wheel="onWheel" @scroll="onScroll">
      <div
        class="nt-sheet-grid"
        :style="{ height: `${scrollHeight}px`, width: `${totalWidth + GUTTER}px` }"
      >
        <!-- 列字母行：吸顶。宽表横向滚动时定位全靠它 -->
        <div class="nt-sheet-header">
          <div class="nt-sheet-cols">
            <span class="nt-sheet-corner">{{ t('sheet.colLabel') }}</span>
            <div
              v-for="c in totalCols"
              :key="c - 1"
              class="nt-sheet-head-cell"
              :style="{ width: `${colWidth(c - 1)}px` }"
            >
              {{ colName(c - 1) }}
            </div>
          </div>
        </div>

        <div class="nt-sheet-window" :style="{ transform: `translateY(${windowTop}px)` }">
          <div v-for="row in windowRows" :key="row.index" class="nt-sheet-row">
            <span class="nt-sheet-gutter">{{ row.index + 1 }}</span>
            <div
              v-for="c in totalCols"
              :key="c - 1"
              class="nt-sheet-cell"
              :class="{
                'is-editing': isEditing(row, c - 1),
                'is-hit': isHit(row.index, c - 1),
                'is-dirty': isDirtyCell(row.index, c - 1),
              }"
              :style="{ width: `${colWidth(c - 1)}px` }"
              @click="startEdit(row.index, c - 1, row.cells[c - 1], row.cells)"
            >
              <input
                v-if="isEditing(row, c - 1)"
                ref="editEl"
                v-model="editingDraft"
                class="nt-sheet-input"
                spellcheck="false"
                @keydown.enter.prevent="commitEdit(0)"
                @keydown.tab.prevent="commitEdit($event.shiftKey ? -1 : 1)"
                @keydown.esc.prevent="cancelEdit"
                @blur="commitEdit(0)"
              />
              <span v-else class="nt-sheet-cell-text" :title="displayText(row.cells[c - 1])">
                {{ displayText(row.cells[c - 1]) }}
              </span>
              <span class="nt-sheet-col-resize" @mousedown.stop.prevent="startResize($event, c - 1)" />
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 工作表页签：与 Excel 一致放底部 -->
    <div v-if="sheets.length > 1" class="nt-doc-sheet-tabs">
      <button
        v-for="s in sheets"
        :key="s.index"
        class="nt-sheet-tab"
        :class="{ 'is-on': s.index === activeSheet }"
        @click="selectSheet(s.index)"
      >
        {{ s.name }}
      </button>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElInput } from 'element-plus'
import { api, normalizeError } from '@/api'
import type { CellEdit, CellValue, SheetHit } from '@/types/api'
import type { Tab } from '@/stores/editor'
import { useEditorStore } from '@/stores/editor'
import {
  registerTableFocus,
  registerTableProvider,
  unregisterTableProvider,
} from '@/composables/useEditorRegistry'
import { useSheetGrid } from '@/composables/useSheetGrid'
import { useToastStore } from '@/stores/toast'
import { baseName } from '@/utils'

const props = defineProps<{ tab: Tab }>()
const { t } = useI18n()
const editor = useEditorStore()
const toast = useToastStore()

/** 行号栏宽度：与 CSS 的 .nt-sheet-gutter 必须同值。 */
const GUTTER = 64

const editing = ref<{ row: number; col: number; draft: string } | null>(null)
/** 编辑框的双向绑定：v-model 不能直接绑可空的 editing.draft。 */
const editingDraft = computed({
  get: () => editing.value?.draft ?? '',
  set: (v: string) => {
    if (editing.value) editing.value.draft = v
  },
})
const editEl = ref<HTMLInputElement | null>(null)
const gotoInput = ref('')
/** 查找框：Ctrl+F 聚焦它。 */
const findInput = ref<{ focus: () => void } | null>(null)
/** 跳行框：Ctrl+G 聚焦它。 */
const gotoEl = ref<{ focus: () => void } | null>(null)
const saving = ref(false)

const pathRef = computed(() => props.tab.path)
const {
  loading,
  error,
  sheets,
  activeSheet,
  totalRows,
  totalCols,
  pendingGroups,
  dirtyCount,
  scrollHost,
  scrollHeight,
  windowTop,
  windowRows,
  totalWidth,
  colWidth,
  setColWidth,
  open: openGrid,
  selectSheet,
  onWheel,
  onScroll,
  scrollToRow,
  setCell,
  isDirtyCell,
  resetEdits,
} = useSheetGrid({
  path: pathRef,
  // 表头带高度：列字母行 24px + 1px 分隔线。
  headerOffset: computed(() => 25),
  onError: (msg) => toast.bad(msg),
})

/** 列号 0 → A、26 → AA。 */
function colName(i: number): string {
  let n = i
  let s = ''
  do {
    s = String.fromCharCode(65 + (n % 26)) + s
    n = Math.floor(n / 26) - 1
  } while (n >= 0)
  return s
}

function isEditing(row: { index: number }, col: number): boolean {
  return editing.value?.row === row.index && editing.value?.col === col
}

// ---- 列宽拖拽 ----

let resizing: { col: number; startX: number; startW: number } | null = null

function startResize(e: MouseEvent, col: number) {
  resizing = { col, startX: e.clientX, startW: colWidth(col) }
  const onMove = (ev: MouseEvent) => {
    if (!resizing) return
    setColWidth(resizing.col, resizing.startW + (ev.clientX - resizing.startX))
  }
  const onUp = () => {
    resizing = null
    document.removeEventListener('mousemove', onMove)
    document.removeEventListener('mouseup', onUp)
    document.body.style.cursor = ''
  }
  document.addEventListener('mousemove', onMove)
  document.addEventListener('mouseup', onUp)
  document.body.style.cursor = 'col-resize'
}

/** 单元格值的显示文本。 */
function displayText(v: CellValue | null | undefined): string {
  if (!v) return ''
  switch (v.kind) {
    case 'text':
      return v.text
    case 'number':
      return v.raw
    case 'boolean':
      return v.value ? 'TRUE' : 'FALSE'
    case 'formula':
      return v.cached || `=${v.formula}`
    case 'error':
      return v.text
  }
}

// ---- 查找 ----

const findText = ref('')
const findHits = shallowRef<SheetHit[]>([])
const findCursor = ref(0)

/** 命中位置集合：`r:c`。 */
const hitSet = computed(() => {
  const s = new Set<string>()
  for (const h of findHits.value) s.add(`${h.row}:${h.col}`)
  return s
})

function isHit(row: number, col: number): boolean {
  return hitSet.value.has(`${row}:${col}`)
}

async function runFind() {
  if (!findText.value || !props.tab.path) return
  try {
    const res = await api.sheet.find({
      path: props.tab.path,
      sheet_index: activeSheet.value,
      find: findText.value,
      match_case: false,
      whole_cell: false,
    })
    findHits.value = res.hits
    findCursor.value = 0
    if (res.hits.length === 0) {
      toast.info(t('dialog.noMatches'))
    } else {
      toast.ok(t('sheet.findHits', { n: res.total }))
      scrollToRow(res.hits[0].row)
    }
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}

function stepFind(delta: number) {
  if (!findHits.value.length) return
  const next = findCursor.value + delta
  if (next < 0) findCursor.value = findHits.value.length - 1
  else if (next >= findHits.value.length) findCursor.value = 0
  else findCursor.value = next
  scrollToRow(findHits.value[findCursor.value].row)
}

// ---- 单元格编辑 ----

function startEdit(
  row: number,
  col: number,
  value: CellValue | null | undefined,
  _cells?: (CellValue | null)[],
) {
  if (col >= totalCols.value) return
  editing.value = { row, col, draft: displayText(value) }
  void nextTick(() => {
    editEl.value?.focus()
    editEl.value?.select()
  })
}

function cancelEdit() {
  editing.value = null
}

/**
 * 提交编辑。move：0 = 原地收尾；1 = 右移（Tab）；-1 = 左移（Shift+Tab）。
 * 值一律存成文本：公式的编辑要走公式栏，网格只改显示值。
 */
function commitEdit(move: number) {
  const ed = editing.value
  if (!ed) return
  setCell(ed.row, ed.col, { kind: 'text', text: ed.draft })
  editing.value = null
  syncDirty()
  if (move === 0) return
  let col = ed.col + move
  let row = ed.row
  if (col >= totalCols.value) {
    col = 0
    row = row + 1
  } else if (col < 0) {
    col = totalCols.value - 1
    row = Math.max(row - 1, 0)
  }
  scrollToRow(row)
  startEdit(row, col, null)
}

function onGoto() {
  const n = Number.parseInt(gotoInput.value, 10)
  if (!Number.isFinite(n) || n < 1) return
  scrollToRow(n - 1)
}

// ---- 保存 ----

function syncDirty() {
  const dirty = dirtyCount.value > 0
  if (props.tab.dirty !== dirty) editor.markDirty(props.tab.id, dirty)
}

/** 原地写回。 */
async function save() {
  await writeBack(undefined)
}

/** 写回。`destPath` 为空 = 原地写回；给了 = 写到新路径（另存为）。 */
async function writeBack(destPath?: string) {
  if (!props.tab.path || dirtyCount.value === 0) return
  saving.value = true
  try {
    // 按工作表分组提交：一张表一组改动。
    const groups = new Map<number, CellEdit[]>()
    for (const [sheetIdx, map] of sheetEdits()) {
      const edits: CellEdit[] = []
      for (const [key, value] of map) {
        const [row, col] = key.split(':').map(Number)
        edits.push({ row, col, value })
      }
      if (edits.length) groups.set(sheetIdx, edits)
    }
    for (const [sheetIdx, edits] of groups) {
      await api.sheet.save({
        path: props.tab.path,
        sheet_index: sheetIdx,
        edits,
        backup: false,
      })
    }
    if (destPath) {
      // 另存为：源文件不动，改动已落到新文件；标签跟到新路径。
      props.tab.path = destPath
      props.tab.title = baseName(destPath)
    }
    resetEdits()
    syncDirty()
    toast.ok(t('sheet.saved', { n: dirtyCount.value }))
    await openGrid()
  } catch (e) {
    toast.bad(normalizeError(e).message)
  } finally {
    saving.value = false
  }
}

function discard() {
  resetEdits()
  syncDirty()
}

/** 另存为：写到新文件，源文件不动。 */
async function saveAs() {
  if (!props.tab.path || dirtyCount.value === 0) return
  const dest = await api.saveFileDialog(baseName(props.tab.path))
  if (!dest) return
  await writeBack(dest)
}

onMounted(async () => {
  await openGrid()
  registerTableFocus(props.tab.id, (what) => {
    if (what === 'goto') {
      gotoInput.value = ''
      gotoEl.value?.focus()
    } else {
      findInput.value?.focus()
    }
  })
  registerTableProvider(props.tab.id, {
    save: async () => {
      await writeBack()
      return !props.tab.dirty
    },
    saveAs: async () => {
      await saveAs()
      return props.tab.path
    },
    isDirty: () => dirtyCount.value > 0,
  })
})

onBeforeUnmount(() => {
  unregisterTableProvider(props.tab.id)
})

// 取全部工作表的改动分组（保存时按表提交）。
function sheetEdits(): Map<number, Map<string, CellValue | null>> {
  return pendingGroups.value
}
</script>

<style scoped>
.nt-sheet {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  min-width: 0;
}

.nt-sheet-bar {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  flex: none;
  height: var(--nt-h-toolbar);
  padding: 0 var(--nt-sp-2);
  border-bottom: 1px solid var(--nt-border);
  background: var(--nt-elev-surface);
  overflow-x: auto;
}

.nt-sheet-meta {
  color: var(--nt-ink-muted);
  white-space: nowrap;
}

.nt-sheet-dirty {
  color: var(--nt-primary);
}

.nt-sheet-find {
  width: 150px;
  flex: none;
}

.nt-sheet-goto {
  width: 140px;
  flex: none;
}

.nt-toolbar-sep {
  width: 1px;
  height: calc(var(--nt-h-toolbar) - 12px);
  background: var(--nt-border);
  flex: none;
}

.nt-toolbar-spacer {
  flex: 1;
}

.nt-sheet-body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  font-size: var(--nt-fs-sm);
  position: relative;
}

.nt-sheet-loading {
  display: flex;
  flex: 1;
  align-items: center;
  justify-content: center;
  gap: var(--nt-sp-3);
}

.nt-sheet-grid {
  position: relative;
  min-width: 100%;
}

/* 表头：列字母行吸顶，宽表横滚时靠它定位 */
.nt-sheet-header {
  position: sticky;
  top: 0;
  z-index: 3;
  background: var(--nt-elev-raised);
  border-bottom: 1px solid var(--nt-border-strong);
}

.nt-sheet-cols {
  display: flex;
  height: 24px;
}

.nt-sheet-corner {
  flex: none;
  width: 64px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--nt-ink-muted);
  background: var(--nt-editor-gutter-bg);
  border-right: 1px solid var(--nt-border);
  font-size: var(--nt-fs-2xs);
  user-select: none;
}

.nt-sheet-head-cell {
  flex: none;
  display: flex;
  align-items: center;
  padding: 0 var(--nt-sp-2);
  border-right: 1px solid var(--nt-border);
  overflow: hidden;
  font-weight: 600;
  color: var(--nt-ink);
}

.nt-sheet-window {
  position: absolute;
  left: 0;
  right: 0;
  top: 0;
}

.nt-sheet-row {
  display: flex;
  height: 26px;
}

.nt-sheet-gutter {
  flex: none;
  width: 64px;
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

.nt-sheet-cell {
  flex: none;
  display: flex;
  align-items: center;
  padding: 0 var(--nt-sp-2);
  border-bottom: 1px solid var(--nt-border);
  border-right: 1px solid var(--nt-border);
  overflow: hidden;
  cursor: cell;
  position: relative;
}

.nt-sheet-cell:hover {
  background: var(--nt-surface-hover);
}

.nt-sheet-cell.is-editing {
  padding: 0;
}

.nt-sheet-cell.is-hit {
  background: var(--nt-warning-bg);
}

.nt-sheet-cell.is-dirty {
  background: var(--nt-primary-soft);
}

.nt-sheet-cell-text {
  width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--nt-ink-2);
}

.nt-sheet-input {
  width: 100%;
  height: 100%;
  border: none;
  outline: 2px solid var(--nt-primary);
  outline-offset: -2px;
  padding: 0 var(--nt-sp-2);
  background: var(--nt-elev-raised);
  color: var(--nt-ink);
  font: inherit;
}

/* 列宽拖拽手柄：骑在右边界上，命中区 7px 够拓 */
.nt-sheet-col-resize {
  position: absolute;
  top: 0;
  right: -3px;
  width: 7px;
  height: 100%;
  cursor: col-resize;
  z-index: 1;
}

.nt-sheet-col-resize:hover {
  background: var(--nt-primary);
  opacity: 0.4;
}
</style>
