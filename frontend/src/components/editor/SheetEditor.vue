<!-- 大表格可编辑网格：分页取数 + 虚拟滚动（空间压缩）+ 覆盖层编辑。
     1000 万行的 CSV 不整篇进内存：后端按记录边界建索引，前端按页取数。
     改动存在覆盖层里（文件一个字节都没动），Ctrl+S 时后端流式重写整个文件。 -->
<template>
  <div class="nt-sheet">
    <div class="nt-sheet-bar">
      <ElButton size="small" :disabled="loading || saving" @click="toggleHeader">
        <NtIcon name="list" size="sm" />
        <span class="nt-btn-text">{{ hasHeader ? t('sheet.headerOff') : t('sheet.headerOn') }}</span>
      </ElButton>
      <ElButton size="small" :disabled="!selectedRow || saving" @click="doInsertRow">
        <NtIcon name="plus" size="sm" />
        <span class="nt-btn-text">{{ t('sheet.addRow') }}</span>
      </ElButton>
      <ElButton size="small" :disabled="!selectedRow || saving" @click="doDeleteRow">
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
          <ElButton size="small" :disabled="!findText || finding" :loading="finding" @click="runFind">
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
      <ElButton size="small" :disabled="!findHits.length || saving" @click="replaceAll">
        {{ t('sheet.replaceAll', { n: findHits.length }) }}
      </ElButton>
      <span class="nt-toolbar-sep" />
      <ElInput
        :model-value="filterText"
        size="small"
        class="nt-sheet-filter"
        :placeholder="t('sheet.filterPlaceholder')"
        clearable
        @update:model-value="onFilterInput"
        @keydown.enter.prevent="void applyView()"
      >
        <template #prefix>
          <NtIcon name="search" size="sm" />
        </template>
      </ElInput>
      <ElButton
        size="small"
        :disabled="!hasView"
        :loading="viewBusy"
        @click="void clearViewNow()"
      >
        {{ t('sheet.clearView') }}
      </ElButton>
      <span class="nt-toolbar-sep" />
      <ElButton size="small" :disabled="!canUndo || saving" :title="t('common.undo')" @click="doUndo">
        <NtIcon name="undo" size="sm" />
      </ElButton>
      <ElButton size="small" :disabled="!canRedo || saving" :title="t('common.redo')" @click="doRedo">
        <NtIcon name="redo" size="sm" />
      </ElButton>
      <ElButton
        size="small"
        :type="frozenFirst ? 'primary' : undefined"
        :title="t('sheet.freezeHint')"
        @click="toggleFreeze"
      >
        <NtIcon name="lock" size="sm" />
        <span class="nt-btn-text">{{ t('sheet.freezeFirst') }}</span>
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
        <template v-if="view && view.filtering">
          {{ t('sheet.rowsOf', { n: viewDataRows, total: view.file_rows }) }}
        </template>
        <template v-else>{{ t('sheet.rows', { n: totalRows }) }}</template>
        · {{ t('sheet.cols', { n: totalCols }) }}
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

    <!-- 建索引期间给真实进度：GB 级表格要顺序扫一遍文件，只有转圈会被当成卡死 -->
    <div v-else-if="loading" class="nt-sheet-loading">
      <span class="nt-spinner" />
      <div class="nt-sheet-progress">
        <span class="nt-readout">
          {{ t('sheet.indexing') }}
          <template v-if="progress != null">{{ Math.round(progress * 100) }}%</template>
        </span>
        <div class="nt-sheet-progress-track">
          <div
            class="nt-sheet-progress-fill"
            :class="{ 'is-indeterminate': !progress }"
            :style="progress != null ? { width: `${Math.round(progress * 100)}%` } : undefined"
          />
        </div>
        <span class="nt-sheet-progress-hint">{{ t('sheet.indexingHint') }}</span>
      </div>
    </div>

    <!--
      ref 必须叫 scrollHost：useTableGrid 用这个名字量视口高度。
      绑错名字 → viewport 恒为 0 → 渲染窗口只剩 overscan 那几行。
    -->
    <div v-else ref="scrollHost" class="nt-sheet-body" @wheel="onWheel" @scroll="onScroll">
      <div
        class="nt-sheet-grid"
        :style="{ height: `${scrollHeight}px`, width: `${totalWidth + GUTTER}px` }"
      >
        <!--
          吸顶表头分两层，职责不同（Excel / WPS 同构）：
          上层是**列字母行**（A / B / C…），只做坐标参照，字号与字色都弱一档；
          下层是**表头值行**（CSV 首行），才是真正的列名，粗体且可点排序。
          没有表头值行时（首行是数据），列字母行自动成为唯一表头，这时才可点排序。
          两层都写「行号」会让用户以为表格有两个表头：角标只有最上层出现一次。
        -->
        <div class="nt-sheet-header">
          <div class="nt-sheet-cols nt-sheet-letters">
            <span class="nt-sheet-corner">{{ t('sheet.colLetters') }}</span>
            <div
              v-for="c in totalCols"
              :key="c - 1"
              class="nt-sheet-head-cell"
              :class="{
                'is-sortable': hasHeader,
                'is-sorted': sortMark(c - 1),
                'is-frozen': frozenFirst && c === 1,
              }"
              :title="hasHeader ? undefined : t('sheet.sortHint', { name: colName(c - 1) })"
              :style="
                frozenFirst && c === 1
                  ? { width: `${colWidth(c - 1)}px`, left: `${frozenLeft(GUTTER)}px` }
                  : { width: `${colWidth(c - 1)}px` }
              "
              @click="hasHeader ? undefined : void toggleSort(c - 1)"
            >
              <span class="nt-sheet-cell-text">{{ colName(c - 1) }}</span>
              <span v-if="!hasHeader && sortMark(c - 1)" class="nt-sheet-sort-mark">
                {{ sortMark(c - 1) === 'asc' ? '▲' : '▼' }}
              </span>
            </div>
          </div>
          <!-- 表头值行：首行真是表头时才出现，这才是列名 -->
          <div v-if="hasHeader" class="nt-sheet-cols nt-sheet-datalabels">
            <span class="nt-sheet-corner">{{ t('sheet.colLabel') }}</span>
            <div
              v-for="c in totalCols"
              :key="`d${c - 1}`"
              class="nt-sheet-head-cell is-sortable"
              :class="{ 'is-sorted': sortMark(c - 1), 'is-frozen': frozenFirst && c === 1 }"
              :title="t('sheet.sortHint', { name: header[c - 1] || colName(c - 1) })"
              :style="
                frozenFirst && c === 1
                  ? { width: `${colWidth(c - 1)}px`, left: `${frozenLeft(GUTTER)}px` }
                  : { width: `${colWidth(c - 1)}px` }
              "
              @click="void toggleSort(c - 1)"
            >
              <span class="nt-sheet-cell-text">{{ header[c - 1] || colName(c - 1) }}</span>
              <span v-if="sortMark(c - 1)" class="nt-sheet-sort-mark">
                {{ sortMark(c - 1) === 'asc' ? '▲' : '▼' }}
              </span>
            </div>
          </div>
        </div>

        <!-- 数据窗口：绝对定位 + translate，行本身按真实高度渲染 -->
        <div class="nt-sheet-window" :style="{ transform: `translateY(${windowTop}px)` }">
          <div
            v-for="row in windowRows"
            :key="row.index"
            class="nt-sheet-row"
            :class="{
              'is-inserted': row.inserted,
              'is-selected': row.orig != null && row.orig === selectedRow,
            }"
            @click="selectRow(row)"
            @mouseup="dragging = false"
          >
            <span
              class="nt-sheet-gutter"
              :class="{ 'is-selected': row.orig != null && row.orig === selectedRow }"
              @click.stop="selectRow(row)"
            >
              {{ row.label }}
            </span>
            <div
              v-for="c in totalCols"
              :key="c - 1"
              class="nt-sheet-cell"
              :class="{
                'is-frozen': frozenFirst && c === 1,
                'is-editing': isEditing(row, c - 1),
                'is-hit': isHit(row, c - 1),
                'is-dirty': row.orig != null && isDirtyCell(row.orig, c - 1),
                'is-focus': isFocused(row, c - 1),
                'is-selected': isInSelection(row, c - 1),
              }"
              :style="
                frozenFirst && c === 1
                  ? { width: `${colWidth(c - 1)}px`, left: `${frozenLeft(GUTTER)}px` }
                  : { width: `${colWidth(c - 1)}px` }
              "
              @click.stop="startEdit(row, c - 1)"
              @mousedown.stop="beginSelect(row, c - 1)"
              @mouseenter="extendSelect(row, c - 1)"
              @contextmenu.prevent="openCellMenu($event, row, c - 1)"
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
                @keydown.up.prevent="commitEdit(-2)"
                @keydown.down.prevent="commitEdit(2)"
                @blur="commitEdit(0)"
              />
              <span
                v-else
                class="nt-sheet-cell-text"
                :class="{ 'is-pending': row.pending }"
                :title="cellTitle(row.cells[c - 1])"
              >
                {{ row.cells[c - 1] }}
              </span>
              <!-- 列宽拖拽手柄：只放在最后一行的右侧不够用，每格右缘都可拖 -->
              <span
                class="nt-sheet-col-resize"
                :class="{ 'is-last': c === totalCols }"
                @mousedown.stop.prevent="startResize($event, c - 1)"
                @dblclick.stop.prevent="fitColumn(c - 1)"
              />
            </div>
          </div>
        </div>
      </div>
    </div>

    <ContextMenu
      v-if="cellMenu"
      :x="cellMenu.x"
      :y="cellMenu.y"
      :items="cellMenuItems"
      @close="cellMenu = null"
    />
  </div>
</template>

<script lang="ts" setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue'
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
import { useSettingsStore } from '@/stores/settings'
import { baseName } from '@/utils'
import NtIcon from '@/components/common/NtIcon.vue'
import ContextMenu from '@/components/common/ContextMenu.vue'
import type { MenuItem } from '@/components/common/contextMenu'

const props = defineProps<{ tab: Tab }>()
const { t } = useI18n()
const editor = useEditorStore()
const toast = useToastStore()
const settings = useSettingsStore()

/** 行号栏宽度：与 CSS 的 .nt-sheet-gutter 必须同值。 */
const GUTTER = 64

const editing = ref<{ orig: number; col: number; draft: string } | null>(null)
/** 编辑框的双向绑定：v-model 不能直接绑可空的 editing.draft。 */
const editingDraft = computed({
  get: () => editing.value?.draft ?? '',
  set: (v: string) => {
    if (editing.value) editing.value.draft = v
  },
})
const editEl = ref<HTMLInputElement | null>(null)
/** 当前选中的行（原始行号）。 */
const selectedRow = ref<number | null>(null)
/** 键盘焦点格：方向键与 Tab 的定位锚点，也是区域选择的锚点。 */
const focused = ref<{ orig: number; col: number } | null>(null)
/** 区域选择的另一端：与 focused 组成矩形选区（拖动 / Shift+方向键扩展）。 */
const selectionEnd = ref<{ orig: number; col: number } | null>(null)
/** 是否正在拖选。 */
const dragging = ref(false)
/** 表头判定覆盖：null = 交给后端启发式。切换即按新判定重建索引。 */
const headerOverride = ref<boolean | null>(null)
/** 跳行输入。 */
const gotoInput = ref('')
const findInput = ref<{ focus: () => void } | null>(null)
const gotoEl = ref<{ focus: () => void } | null>(null)
const saving = ref(false)

const pathRef = computed(() => props.tab.path)
const {
  meta,
  loading,
  error,
  progress,
  header,
  hasHeader,
  totalRows,
  viewDataRows,
  totalCols,
  editCount,
  scrollHost,
  scrollHeight,
  windowTop,
  windowRows,
  totalWidth,
  colWidth,
  setColWidth,
  autoFitColumn,
  open: openGrid,
  close: closeGrid,
  onWheel,
  onScroll,
  scrollToCell,
  scrollToOriginalRow,
  displayIndexOf,
  cellValue,
  setCell,
  isDirtyCell,
  deleteRow,
  insertRowAfter,
  resetEdits,
  buildOps,
  view,
  viewBusy,
  filterText,
  applyView,
  clearView,
  toggleSort,
  dataStart,
  sortMark,
  frozenFirst,
  toggleFreeze,
  frozenLeft,
  undo,
  redo,
  canUndo,
  canRedo,
} = useTableGrid({
  path: pathRef,
  encoding: computed(() => props.tab.encoding),
  hasHeader: headerOverride,
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

/** 悬浮提示截断：几 MB 的单元格文本进 DOM title 会拖慢整窗绘制。 */
function cellTitle(v: string | undefined): string {
  if (!v) return ''
  return v.length > 500 ? `${v.slice(0, 500)}…` : v
}

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

/** 扫全表找一次。一页 500 条，扫不完可以「下一个」续扫。 */
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
    findHits.value = res.hits ?? []
    findMore.value = !res.done
    if (findHits.value.length === 0) {
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

/** 上一个 / 下一个命中。走到末尾还有没扫完的部分就续扫一轮。 */
async function stepFind(delta: number) {
  if (!findHits.value.length) {
    await runFind()
    return
  }
  const next = findCursor.value + delta
  if (next < 0) {
    findCursor.value = findHits.value.length - 1
  } else if (next >= findHits.value.length) {
    if (findMore.value && delta > 0) await extendFind()
    findCursor.value = findHits.value.length ? 0 : 0
    if (delta < 0) findCursor.value = findHits.value.length - 1
  } else {
    findCursor.value = next
  }
  await jumpToHit(findCursor.value)
}

/** 续扫一轮：从最后一条命中之后接着找。 */
async function extendFind() {
  const last = findHits.value[findHits.value.length - 1]
  if (!last || !props.tab.path) return
  finding.value = true
  try {
    const res = await api.table.find({
      path: props.tab.path,
      query: findText.value,
      match_case: false,
      from_row: last.row + 1,
      limit: 500,
    })
    const more = res.hits ?? []
    if (more.length) findHits.value = [...findHits.value, ...more]
    findMore.value = !res.done
  } catch (e) {
    toast.bad(normalizeError(e).message)
    findMore.value = false
  } finally {
    finding.value = false
  }
}

/** 滚到第 n 条命中：横纵向都定位到该格。 */
async function jumpToHit(n: number) {
  const hit = findHits.value[n]
  if (!hit) return
  findCursor.value = n
  scrollToCell(hit.row, hit.col)
  selectedRow.value = hit.row
  focused.value = { orig: hit.row, col: hit.col }
}

/** 全部替换：改命中格的值（进覆盖层，不动文件）。 */
function replaceAll() {
  if (!findHits.value.length) return
  const needle = findText.value
  if (!needle) return
  const repl = replaceText.value
  let n = 0
  for (const h of findHits.value) {
    // 用 cellValue 而不是 windowRows：命中的格很可能不在当前渲染窗口里。
    const cur = cellValue(h.row, h.col)
    if (!cur.includes(needle)) continue
    setCell(h.row, h.col, cur.split(needle).join(repl))
    n += 1
  }
  findHits.value = []
  syncDirty()
  if (n > 0) toast.ok(t('sheet.replaced', { n }))
  else toast.info(t('dialog.noMatches'))
}

const replaceText = ref('')

// ---- 单元格编辑 ----

type GridRow = { orig: number | null; cells: string[] }

function isEditing(row: GridRow, col: number): boolean {
  return row.orig != null && editing.value?.orig === row.orig && editing.value?.col === col
}

function isFocused(row: GridRow, col: number): boolean {
  return row.orig != null && focused.value?.orig === row.orig && focused.value?.col === col
}

/** 按下：定位焦点格并开始拖选。 */
function beginSelect(row: GridRow, col: number) {
  if (row.orig == null) return
  focused.value = { orig: row.orig, col }
  selectionEnd.value = { orig: row.orig, col }
  selectedRow.value = row.orig
  dragging.value = true
}

/** 拖过一格：扩展选区（只更新端点，矩形在 computed 里算）。 */
function extendSelect(row: GridRow, col: number) {
  if (!dragging.value || row.orig == null) return
  selectionEnd.value = { orig: row.orig, col }
}

function startEdit(row: GridRow, col: number) {
  if (row.orig == null || col >= totalCols.value) return
  focused.value = { orig: row.orig, col }
  editing.value = { orig: row.orig, col, draft: row.cells[col] ?? '' }
  void nextTick(() => {
    editEl.value?.focus()
    editEl.value?.select()
  })
}

function cancelEdit() {
  editing.value = null
}

/**
 * 提交编辑。
 * move：0 = 原地收尾（Enter / 失焦）；1 = 右移（Tab）；-1 = 左移（Shift+Tab）；
 * 2 = 下移（↓）；-2 = 上移（↑）。
 */
function commitEdit(move: number) {
  const ed = editing.value
  if (!ed) return
  const cur = cellValue(ed.orig, ed.col)
  if (cur !== ed.draft) {
    setCell(ed.orig, ed.col, ed.draft)
    syncDirty()
  }
  editing.value = null
  if (move === 0) return
  let row = ed.orig
  let col = ed.col
  if (move === 1 || move === -1) {
    col += move
    // 走到行尾就换行，符合表格里一路 Tab 的预期。
    if (col >= totalCols.value) {
      col = 0
      row = Math.min(row + 1, totalRows.value - 1)
    } else if (col < 0) {
      col = totalCols.value - 1
      row = Math.max(row - 1, 0)
    }
  } else {
    row = Math.min(Math.max(row + (move === 2 ? 1 : -1), 0), totalRows.value - 1)
  }
  scrollToCell(row, col)
  selectedRow.value = row
  focused.value = { orig: row, col }
  void nextTick(() => startEdit({ orig: row, cells: [] }, col))
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

/** 双击列分隔线：按已缓存的内容自适应宽度。 */
function fitColumn(col: number) {
  autoFitColumn(col)
}

// ---- 行操作 ----

function selectRow(row: GridRow) {
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

/**
 * 切换「首行是否表头」：按新判定重建索引。
 * 表头误判（首行其实是数据）时用户在这里纠正；重建会丢弃未写回的改动。
 */
function toggleHeader() {
  headerOverride.value = !hasHeader.value
  void openGrid()
}

/** 跳到指定行号（1 基，与行号栏显示一致）。 */
function onGoto() {
  const n = Number.parseInt(gotoInput.value, 10)
  if (!Number.isFinite(n) || n < 1) return
  // 行号栏显示 orig + 1，所以跳转也按 1 基换算。
  const orig = Math.min(Math.max(n - 1, 0), totalRows.value - 1)
  scrollToOriginalRow(orig)
  selectedRow.value = orig
  focused.value = { orig, col: 0 }
}

// ---- 右键菜单 ----

/** 菜单位置与作用的对象：右键点的位置 + 那一格。 */
const cellMenu = ref<{ x: number; y: number } | null>(null)
const menuRow = shallowRef<GridRow | null>(null)
const menuCol = ref(0)

/** 菜单条目：按点击位置动态生成，每个条目自带回调。 */
const cellMenuItems = computed<MenuItem[]>(() => {
  const row = menuRow.value
  const col = menuCol.value
  const hasCell = row?.orig != null
  const text = row?.cells[col] ?? ''
  return [
    {
      label: t('menu.copy'),
      hint: 'Ctrl+C',
      disabled: !row,
      run: () => void copyCells(text),
    },
    {
      label: t('sheet.copyRow'),
      disabled: !row,
      run: () => void copyCells(row?.cells.join('\t') ?? ''),
    },
    { sep: true },
    {
      label: t('sheet.addRow'),
      disabled: !hasCell,
      run: () => {
        if (row?.orig == null) return
        insertRowAfter(row.orig, new Array(totalCols.value).fill(''))
        syncDirty()
      },
    },
    {
      label: t('sheet.deleteRow'),
      danger: true,
      disabled: !hasCell,
      run: () => {
        if (row?.orig == null) return
        deleteRow(row.orig)
        selectedRow.value = null
        syncDirty()
      },
    },
    { sep: true },
    { label: t('sheet.copyColName'), run: () => void copyCells(colName(col)) },
    {
      label: t('sheet.selectColumn'),
      run: () => selectColumn(col),
    },
  ]
})

function openCellMenu(e: MouseEvent, row: GridRow, col: number) {
  menuRow.value = row
  menuCol.value = col
  if (row.orig != null) {
    selectedRow.value = row.orig
    focused.value = { orig: row.orig, col }
  }
  cellMenu.value = { x: e.clientX, y: e.clientY }
}

/** 复制到系统剪贴板：表格里复制的是纯文本，粘进 Excel 直接分列。 */
async function copyCells(text: string) {
  if (!text) return
  try {
    await api.writeClipboard(text)
    toast.ok(t('toast.copied'))
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}

// ---- 区域选择 ----

/** 选区矩形（原始行号区间 + 列区间），没有选区时为 null。 */
const selectionRect = computed(() => {
  const a = focused.value
  const b = selectionEnd.value
  if (!a || !b) return null
  return {
    r0: Math.min(a.orig, b.orig),
    r1: Math.max(a.orig, b.orig),
    c0: Math.min(a.col, b.col),
    c1: Math.max(a.col, b.col),
  }
})

/** 某格是否在选区内（渲染时每格都问一次，先做边界短路）。 */
function isInSelection(row: { orig: number | null }, col: number): boolean {
  if (row.orig == null) return false
  const r = selectionRect.value
  if (!r) return false
  return row.orig >= r.r0 && row.orig <= r.r1 && col >= r.c0 && col <= r.c1
}

/** 选区文本的分隔符：制表符分行、换行分记录——粘进 Excel 直接分列。 */
const TSV_SEP = String.fromCharCode(9)
const LINE_SEP = String.fromCharCode(10)

/** 选区里的文本（TSV）：粘进 Excel 直接分列。 */
function selectionText(): string {
  const r = selectionRect.value
  if (!r) return ''
  const lines: string[] = []
  for (let orig = r.r0; orig <= r.r1; orig += 1) {
    const cells: string[] = []
    for (let c = r.c0; c <= r.c1; c += 1) cells.push(cellValue(orig, c))
    lines.push(cells.join(TSV_SEP))
  }
  return lines.join(LINE_SEP)
}

/** 复制选区（没有选区时复制焦点格）。 */
async function copySelection() {
  const f = focused.value
  const text = selectionRect.value ? selectionText() : f ? cellValue(f.orig, f.col) : ''
  if (!text) return
  await copyCells(text)
}

/** 选中整列（右键菜单用一个入口，不抢列头的点击排序）。 */
function selectColumn(col: number) {
  if (totalRows.value === 0) return
  const r0 = dataStart.value
  const r1 = totalRows.value - 1
  focused.value = { orig: r0, col }
  selectionEnd.value = { orig: r1, col }
}

// ---- 视图与撤销 ----

/** 有没有生效的排序 / 筛选（决定「取消视图」按钮是否可用）。 */
const hasView = computed(() => view.value != null)

/** 筛选框输入：防抖 350ms 后重建视图（每敲一个字都扫一遍文件太浪费）。 */
let filterTimer = 0
function onFilterInput(v: string) {
  filterText.value = v
  window.clearTimeout(filterTimer)
  filterTimer = window.setTimeout(() => {
    void applyView()
  }, 350)
}

/** 清掉排序与筛选，回到文件顺序。 */
async function clearViewNow() {
  await clearView()
  syncDirty()
}

function doUndo() {
  undo()
  syncDirty()
}

function doRedo() {
  redo()
  syncDirty()
}

// ---- 保存 ----

function syncDirty() {
  const dirty = editCount.value > 0
  if (props.tab.dirty !== dirty) editor.markDirty(props.tab.id, dirty)
}

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
      props.tab.path = res.path
      props.tab.title = baseName(res.path)
      toast.ok(t('sheet.savedRows', { n: res.changed }))
      return
    }
    resetEdits()
    syncDirty()
    toast.ok(t('sheet.savedRows', { n: res.changed }))
    // 写回后索引失效：重建并刷新读数（列宽覆盖层保留，用户拖过的宽度不该被重置）。
    await openGrid()
  } catch (e) {
    toast.bad(normalizeError(e).message)
  } finally {
    saving.value = false
  }
}

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

/** 视图重建后行数会变（筛选）：读数跟着刷。 */
watch(totalRows, (n) => announceLines(n))

/** 状态栏行数：表格的行数走同一事件通道。 */
function announceLines(lines: number) {
  document.dispatchEvent(new CustomEvent('nt-stats', { detail: { tabId: props.tab.id, lines } }))
}

/** 方向键在非编辑态下移动焦点格：表格里这是最基本的导航。 */
function onKeyNav(e: KeyboardEvent) {
  const f = focused.value
  if (!f || editing.value) return
  const target = e.target as HTMLElement | null
  // 输入框里的方向键与 Ctrl+Z 归输入框自己。
  if (target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA') return

  // Ctrl/Cmd 组合：撤销 / 重做 / 复制选区。
  if (e.ctrlKey || e.metaKey) {
    const k = e.key.toLowerCase()
    if (k === 'z' && !e.shiftKey) {
      e.preventDefault()
      doUndo()
      return
    }
    if ((k === 'z' && e.shiftKey) || k === 'y') {
      e.preventDefault()
      doRedo()
      return
    }
    if (k === 'c') {
      e.preventDefault()
      void copySelection()
      return
    }
    if (k === 'a') {
      // 全选：整张表进选区，粘出去就是一整张表。
      e.preventDefault()
      const r0 = dataStart.value
      const r1 = Math.max(r0, totalRows.value - 1)
      focused.value = { orig: r0, col: 0 }
      selectionEnd.value = { orig: r1, col: Math.max(0, totalCols.value - 1) }
      return
    }
    return
  }

  let { orig, col } = f
  switch (e.key) {
    case 'ArrowUp':
      orig = Math.max(orig - 1, 0)
      break
    case 'ArrowDown':
      orig = Math.min(orig + 1, totalRows.value - 1)
      break
    case 'ArrowLeft':
      col = Math.max(col - 1, 0)
      break
    case 'ArrowRight':
      col = Math.min(col + 1, totalCols.value - 1)
      break
    case 'Enter':
    case 'F2':
      e.preventDefault()
      startEdit({ orig, cells: [] }, col)
      return
    case 'Escape':
      e.preventDefault()
      selectionEnd.value = null
      return
    default:
      return
  }
  e.preventDefault()
  scrollToCell(orig, col)
  focused.value = { orig, col }
  selectedRow.value = orig
  // 不按 Shift 移动焦点 = 重新开始选区。
  selectionEnd.value = e.shiftKey ? selectionEnd.value ?? { orig, col } : { orig, col }
}

onMounted(async () => {
  await openGrid()
  // 首页取回后把焦点落在第一格：用户按方向键就能走。
  if (totalRows.value > 0) {
    const orig = hasHeader.value ? 1 : 0
    focused.value = { orig: Math.min(orig, totalRows.value - 1), col: 0 }
  }
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
    // 按指定编码重建索引：表格不走全量读，编码切换只能走这条路。
    reopen: async (enc: string) => {
      if (props.tab.encoding === enc) return
      props.tab.encoding = enc
      headerOverride.value = null
      await openGrid()
    },
  })
  window.addEventListener('keydown', onKeyNav)
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeyNav)
  unregisterTableProvider(props.tab.id)
  void closeGrid()
})

// 行高常量供样式使用（与 CSS 同值）。
void ROW_H
void displayIndexOf
void settings
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
  width: 160px;
  flex: none;
}

.nt-sheet-filter {
  width: 180px;
  flex: none;
}

/* 列头可排序：整格可点，悬停只换底色（不缩放——表头缩放会让整列跟着抖） */
.nt-sheet-head-cell.is-sortable {
  cursor: pointer;
  transition: background var(--nt-dur-fast) var(--nt-ease);
}

.nt-sheet-head-cell.is-sortable:hover {
  background: var(--nt-elev-hover);
}

.nt-sheet-head-cell.is-sorted {
  color: var(--nt-primary);
}

.nt-sheet-sort-mark {
  flex: none;
  margin-left: var(--nt-sp-1);
  font-size: var(--nt-fs-2xs);
  color: var(--nt-primary);
}

.nt-sheet-goto {
  width: 150px;
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

.nt-sheet-loading {
  display: flex;
  flex: 1;
  align-items: center;
  justify-content: center;
  gap: var(--nt-sp-3);
}

/* 建索引进度：条 + 百分比 + 一句「在忙什么」，让用户知道该等还是该取消 */
.nt-sheet-progress {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-1);
  min-width: calc(220px * var(--nt-vw-scale));
}

.nt-sheet-progress-track {
  height: 4px;
  border-radius: var(--nt-radius-full);
  background: var(--nt-elev-selected);
  overflow: hidden;
}

.nt-sheet-progress-fill {
  height: 100%;
  background: var(--nt-primary);
  border-radius: var(--nt-radius-full);
  transition: width var(--nt-dur) var(--nt-ease);
}

/* 进度未知时来回滑动：条不动等于「没在做事」 */
.nt-sheet-progress-fill.is-indeterminate {
  width: 40%;
  animation: nt-sheet-indeterminate 1.1s var(--nt-ease) infinite;
}

@keyframes nt-sheet-indeterminate {
  0% {
    transform: translateX(-100%);
  }
  100% {
    transform: translateX(250%);
  }
}

.nt-sheet-progress-hint {
  font-size: var(--nt-fs-2xs);
  color: var(--nt-ink-muted);
}

/* 滚动容器：横向与纵向都由它承担，虚拟滚动只换纵向内容高度 */
.nt-sheet-body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  font-size: var(--nt-fs-sm);
  position: relative;
}

.nt-sheet-grid {
  position: relative;
  min-width: 100%;
}

/* 表头：两层都吸顶。列字母行永远在，表头值行跟着 has_header */
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

/* 表头值行：真列名，压在列字母行下面，底色比字母行亮一档 */
.nt-sheet-datalabels {
  border-top: 1px solid var(--nt-border);
  background: var(--nt-elev-surface);
}

.nt-sheet-corner {
  flex: none;
  position: sticky;
  left: 0;
  z-index: 5;
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

/* 列字母行只做坐标参照（Excel 同构）：居中、等宽、弱一档。
   与表头值行同粗同色会让两层读起来是两个表头——用户分不清哪个是真的列名。 */
.nt-sheet-letters .nt-sheet-head-cell {
  justify-content: center;
  padding: 0;
  font-weight: 400;
  font-size: var(--nt-fs-2xs);
  font-variant-numeric: tabular-nums;
  color: var(--nt-ink-muted);
  background: var(--nt-editor-gutter-bg);
}

/* 数据窗口：绝对定位，靠 translateY 落到正确位置 */
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

.nt-sheet-row.is-selected {
  background: var(--nt-surface-selected);
}

.nt-sheet-gutter {
  flex: none;
  /* 行号是「这行是谁」的唯一线索：横向滚动时必须常驻，否则滚到第 20 列就丢了定位 */
  position: sticky;
  left: 0;
  z-index: 3;
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

.nt-sheet-gutter.is-selected {
  color: var(--nt-primary);
  background: var(--nt-primary-soft);
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

/* 冻结首列：横向滚动时常驻，左偏移由行号栏宽度给（行号栏不冻结，所以首列粘在它右侧） */
.nt-sheet-cell.is-frozen {
  position: sticky;
  z-index: 2;
  background: var(--nt-elev-surface);
  border-right: 1px solid var(--nt-border-strong);
}

.nt-sheet-row:hover .nt-sheet-cell.is-frozen {
  background: var(--nt-surface-hover);
}

.nt-sheet-head-cell.is-frozen {
  position: sticky;
  z-index: 4;
  background: var(--nt-elev-raised);
  border-right: 1px solid var(--nt-border-strong);
}

/* 选区：连续矩形必须与「查找命中」区分开——命中是单格描边，选区是整片浅底 */
.nt-sheet-cell.is-selected {
  background: var(--nt-primary-soft);
}

.nt-sheet-cell.is-selected.is-focus {
  outline: 1px solid var(--nt-primary);
  outline-offset: -1px;
}

.nt-sheet-cell.is-hit {
  background: var(--nt-warning-bg);
}

.nt-sheet-cell.is-dirty {
  background: var(--nt-primary-soft);
}

.nt-sheet-cell.is-focus {
  outline: 2px solid var(--nt-primary);
  outline-offset: -2px;
}

.nt-sheet-cell-text {
  width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--nt-ink-2);
}

/* 页还没到的占位：低频呼吸，几何不变（行高固定，不跳） */
.nt-sheet-cell-text.is-pending {
  background: var(--nt-elev-hover);
  animation: nt-sheet-pending 1.2s ease-in-out infinite;
}

@keyframes nt-sheet-pending {
  50% {
    opacity: 0.5;
  }
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

/* 列宽拖拽手柄：骑在右边界上，命中区 7px 够抓 */
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
