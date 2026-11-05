// 大表格可编辑网格：分页取数 + 虚拟滚动（空间压缩）+ 覆盖层编辑。
//
// 1000 万行的 CSV 不整篇进内存：后端按记录边界建索引，前端按页取数。
// 改动存在覆盖层里（文件一个字节都没动），Ctrl+S 时后端流式重写整个文件。
<template>
  <div class="nt-sheet">
    <div class="nt-sheet-bar">
      <ElButton size="small" :disabled="!hasHeader" @click="toggleHeader">
        <NtIcon name="list" size="sm" />
        <span>{{ t('sheet.showHeader') }}</span>
      </ElButton>
      <ElButton size="small" :disabled="!selectedRow" @click="doInsertRow">
        <NtIcon name="plus" size="sm" />
        <span>{{ t('sheet.addRow') }}</span>
      </ElButton>
      <ElButton size="small" :disabled="!selectedRow" @click="doDeleteRow">
        {{ t('sheet.deleteRow') }}
      </ElButton>
      <span class="nt-toolbar-sep" />
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
      <ElButton size="small" :disabled="!findHits.length" @click="replaceAll">
        {{ t('sheet.replaceAll', { n: findHits.length }) }}
      </ElButton>
      <span class="nt-toolbar-spacer" />
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
      <span class="nt-readout nt-sheet-meta">
        {{ t('sheet.rows', { n: totalRows }) }} · {{ t('sheet.cols', { n: totalCols }) }}
        <template v-if="editCount > 0"> · {{ t('sheet.pending', { n: editCount }) }}</template>
      </span>
      <ElButton
        size="small"
        type="primary"
        :disabled="editCount === 0 || saving"
        :loading="saving"
        @click="save"
      >
        {{ t('sheet.saveWorkbook') }}
      </ElButton>
      <ElButton size="small" text :disabled="editCount === 0 || saving" @click="discard">
        {{ t('sheet.discardShort') }}
      </ElButton>
    </div>

    <div v-if="error" class="nt-mount-error">
      <span class="nt-mount-error-title">{{ t('editor.openFailedTitle') }}</span>
      <span class="nt-mount-error-detail">{{ error }}</span>
    </div>

    <div
      v-else
      ref="scrollEl"
      class="nt-sheet-body"
      @wheel="onWheel"
      @scroll="onScroll"
    >
      <!-- 表头行：吸顶 -->
      <div v-if="showHeader" class="nt-sheet-header-row">
        <span class="nt-sheet-gutter">{{ t('sheet.colLabel') }}</span>
        <div v-for="(h, ci) in header" :key="ci" class="nt-sheet-cell nt-sheet-head-cell">
          <span class="nt-sheet-cell-text">{{ h || colName(ci) }}</span>
        </div>
      </div>

      <div class="nt-sheet-inner" :style="{ height: `${scrollHeight}px` }">
        <div class="nt-sheet-window" :style="{ transform: `translateY(${windowTop}px)` }">
          <div
            v-for="row in windowRows"
            :key="row.index"
            class="nt-sheet-row"
            :class="{ 'is-inserted': row.inserted }"
            @click="selectRow(row)"
          >
            <span class="nt-sheet-gutter">{{ row.label }}</span>
            <div
              v-for="ci in totalCols"
              :key="ci"
              class="nt-sheet-cell"
              :class="{
                'is-editing': editing?.orig === row.orig && editing?.col === ci - 1,
                'is-hit': isHit(row, ci - 1),
                'is-dirty': row.orig != null && isDirtyCell(row.orig, ci - 1),
              }"
              @click.stop="startEdit(row, ci - 1)"
            >
              <input
                v-if="editing?.orig === row.orig && editing?.col === ci - 1"
                ref="editEl"
                v-model="editing.draft"
                class="nt-sheet-input"
                spellcheck="false"
                @keydown.enter.prevent="commitEdit(0)"
                @keydown.tab.prevent="commitEdit(1)"
                @keydown.esc.prevent="cancelEdit"
                @blur="commitEdit(0)"
              />
              <span v-else class="nt-sheet-cell-text" :title="row.cells[ci - 1]">
                {{ row.cells[ci - 1] }}
              </span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElInput } from 'element-plus'
import { api, normalizeError } from '@/api'
import type { TableHitVO, TableMetaVO } from '@/types/api'
import type { Tab } from '@/stores/editor'
import { useEditorStore } from '@/stores/editor'
import {
  registerTableFocus,
  registerTableProvider,
  unregisterTableProvider,
} from '@/composables/useEditorRegistry'
import { useTableGrid, ROW_H } from '@/composables/useTableGrid'
import { useToastStore } from '@/stores/toast'
import { baseName } from '@/utils'

const props = defineProps<{ tab: Tab }>()
const { t } = useI18n()
const editor = useEditorStore()
const toast = useToastStore()

const scrollEl = ref<HTMLElement | null>(null)
const editing = ref<{ orig: number; col: number; draft: string } | null>(null)
const editEl = ref<HTMLInputElement | null>(null)
/** 当前选中的行（原始行号）。 */
const selectedRow = ref<number | null>(null)
/** 表头显示开关。 */
const showHeader = ref(true)
/** 跳行输入。 */
const gotoInput = ref('')
/** 查找框：Ctrl+F 聚焦它。 */
const findInput = ref<{ focus: () => void } | null>(null)
/** 跳行框：Ctrl+G 聚焦它。 */
const gotoEl = ref<{ focus: () => void } | null>(null)
/** 保存中。 */
const saving = ref(false)

const pathRef = computed(() => props.tab.path)
const {
  meta,
  error,
  header,
  hasHeader,
  totalRows,
  totalCols,
  editCount,
  scrollHeight,
  windowTop,
  windowRows,
  open: openGrid,
  close: closeGrid,
  onWheel,
  onScroll,
  scrollToOriginalRow,
  setCell,
  isDirtyCell,
  deleteRow,
  insertRowAfter,
  resetEdits,
  buildOps,
} = useTableGrid({
  path: pathRef,
  encoding: computed(() => props.tab.encoding),
  onMeta: (m: TableMetaVO) => {
    props.tab.encoding = m.encoding
    props.tab.line_ending = m.line_ending as Tab['line_ending']
    props.tab.has_bom = m.has_bom
    announceLines(m.total_rows)
  },
  onError: (msg) => {
    if (msg && msg !== 'stale') toast.bad(msg)
    else if (msg === 'stale') toast.warn(t('toast.bigFileStale', { name: props.tab.title }))
  },
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

// ---- 查找 ----

const findText = ref('')
const findHits = shallowRef<TableHitVO[]>([])
const findCursor = ref(0)
/** 后端还有没扫完的部分。 */
const findMore = ref(false)
const finding = ref(false)

/** 命中集合：渲染时每个格子都要问一次，Set 比数组 some() 快一个量级。 */
const hitSet = computed(() => {
  const s = new Set<string>()
  for (const h of findHits.value) s.add(`${h.row}:${h.col}`)
  return s
})

function isHit(row: { orig: number | null }, col: number): boolean {
  if (row.orig == null) return false
  return hitSet.value.has(`${row.orig}:${col}`)
}

/** 扫全表找一次。 */
async function runFind() {
  if (!findText.value || !props.tab.path) return
  finding.value = true
  findHits.value = []
  findCursor.value = 0
  try {
    const res = await api.table.find({
      path: props.tab.path,
      query: findText.value,
      match_case: false,
      from_row: 0,
      limit: 500,
    })
    findHits.value = res.hits
    findMore.value = !res.done
    if (res.hits.length === 0) {
      toast.info(t('dialog.noMatches'))
    } else {
      toast.ok(t('sheet.findHits', { n: res.total }))
      await jumpToHit(0)
    }
  } catch (e) {
    toast.bad(normalizeError(e).message)
  } finally {
    finding.value = false
  }
}

/** 上一个 / 下一个命中。 */
async function stepFind(delta: number) {
  if (!findHits.value.length) {
    await runFind()
    return
  }
  const next = findCursor.value + delta
  if (next < 0) findCursor.value = findHits.value.length - 1
  else if (next >= findHits.value.length) findCursor.value = 0
  else findCursor.value = next
  await jumpToHit(findCursor.value)
}

/** 滚到第 n 条命中。 */
async function jumpToHit(n: number) {
  const hit = findHits.value[n]
  if (!hit) return
  findCursor.value = n
  scrollToOriginalRow(hit.row)
}

/** 全部替换：改命中格的值（进覆盖层，不动文件）。 */
function replaceAll() {
  if (!findHits.value.length) return
  const needle = findText.value
  for (const h of findHits.value) {
    const cur = cellValue(h.row, h.col)
    const next = cur.split(needle).join(replaceText.value)
    setCell(h.row, h.col, next)
  }
  const n = findHits.value.length
  findHits.value = []
  syncDirty()
  toast.ok(t('sheet.replaced', { n }))
}

const replaceText = ref('')

/** 某格当前值（含覆盖层）。 */
function cellValue(orig: number, col: number): string {
  const row = windowRows.value.find((r) => r.orig === orig)
  return row?.cells[col] ?? ''
}

// ---- 单元格编辑 ----

/** 网格行：原始行号 + 字段值。插入行的 orig 为 null（不可编辑）。 */
type GridRow = { orig: number | null; cells: string[] }

function startEdit(row: GridRow, col: number) {
  if (row.orig == null) return
  editing.value = { orig: row.orig, col, draft: row.cells[col] ?? '' }
  void nextTick(() => editEl.value?.focus())
}

function cancelEdit() {
  editing.value = null
}

/** 提交编辑。move=1 时（Tab 键）顺手跳到右边一格。 */
function commitEdit(move: number) {
  const ed = editing.value
  if (!ed) return
  const row = windowRows.value.find((r) => r.orig === ed.orig)
  const cur = row?.cells[ed.col] ?? ''
  if (cur !== ed.draft) {
    setCell(ed.orig, ed.col, ed.draft)
    syncDirty()
  }
  editing.value = null
  if (move === 1) startEdit({ orig: ed.orig, cells: [] }, ed.col + 1)
}

// ---- 行操作 ----

function selectRow(row: { orig: number | null }) {
  if (row.orig == null) return
  selectedRow.value = row.orig
}

function doInsertRow() {
  if (selectedRow.value == null) return
  insertRowAfter(selectedRow.value, new Array(totalCols.value).fill(''))
  syncDirty()
}

function doDeleteRow() {
  if (selectedRow.value == null) return
  deleteRow(selectedRow.value)
  selectedRow.value = null
  syncDirty()
}

function toggleHeader() {
  showHeader.value = !showHeader.value
}

/** 跳到指定原始行号。 */
function onGoto() {
  const n = Number.parseInt(gotoInput.value, 10)
  if (!Number.isFinite(n) || n < 1) return
  scrollToOriginalRow(n)
}

// ---- 保存 ----

/** 脏标记。 */
function syncDirty() {
  const dirty = editCount.value > 0
  if (props.tab.dirty !== dirty) editor.markDirty(props.tab.id, dirty)
}

/** 原地写回。 */
async function save() {
  await writeBack(undefined)
}

/** 写回。`destPath` 为空 = 原地写回；给了 = 写到新路径（另存为）。 */
async function writeBack(destPath?: string) {
  if (!props.tab.path || editCount.value === 0) return
  saving.value = true
  try {
    const res = await api.table.save({
      path: props.tab.path,
      dest_path: destPath ?? null,
      ops: buildOps(),
      encoding: props.tab.encoding,
      delimiter: meta.value?.delimiter ?? null,
      backup: false,
    })
    if (destPath) {
      // 另存为：源文件不动，改动已落到新文件；标签跟到新路径。
      props.tab.path = res.path
      props.tab.title = baseName(res.path)
      toast.ok(t('sheet.savedRows', { n: res.changed }))
      return
    }
    resetEdits()
    syncDirty()
    toast.ok(t('sheet.savedRows', { n: res.changed }))
    // 写回后索引失效：重建并刷新读数。
    await openGrid()
  } catch (e) {
    toast.bad(normalizeError(e).message)
  } finally {
    saving.value = false
  }
}

/** 另存为：写到新文件，源文件不动。 */
async function saveAs() {
  if (!props.tab.path || editCount.value === 0) return
  const dest = await api.saveFileDialog(baseName(props.tab.path))
  if (!dest) return
  await writeBack(dest)
}

function discard() {
  resetEdits()
  syncDirty()
}

/** 状态栏行数：表格的行数走同一事件通道。 */
function announceLines(lines: number) {
  document.dispatchEvent(
    new CustomEvent('nt-stats', { detail: { tabId: props.tab.id, lines } }),
  )
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
    isDirty: () => editCount.value > 0,
  })
})

onBeforeUnmount(() => {
  unregisterTableProvider(props.tab.id)
  void closeGrid()
})

// 行高常量供样式使用。
void ROW_H
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

.nt-sheet-header-row {
  display: flex;
  height: 26px;
  position: sticky;
  top: 0;
  z-index: 2;
  background: var(--nt-elev-raised);
  border-bottom: 1px solid var(--nt-border-strong);
}

.nt-sheet-head-cell {
  font-weight: 600;
  color: var(--nt-ink);
}

.nt-sheet-inner {
  position: relative;
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

.nt-sheet-row.is-inserted {
  background: var(--nt-primary-soft);
}

.nt-sheet-gutter {
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

.nt-sheet-cell {
  flex: none;
  width: 180px;
  display: flex;
  align-items: center;
  padding: 0 var(--nt-sp-2);
  border-bottom: 1px solid var(--nt-border);
  border-right: 1px solid var(--nt-border);
  overflow: hidden;
  cursor: cell;
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
  outline: 1px solid var(--nt-primary);
  padding: 0 var(--nt-sp-2);
  background: var(--nt-elev-raised);
  color: var(--nt-ink-1);
  font: inherit;
}
</style>
