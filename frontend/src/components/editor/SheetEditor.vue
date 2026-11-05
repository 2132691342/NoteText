<!-- 大表格可编辑网格：分页取数 + 虚拟滚动（空间压缩）+ 覆盖层编辑。
     1000 万行的 CSV 不整篇进内存：后端按记录边界建索引，前端按页取数。
     改动存在覆盖层里（文件一个字节都没动），Ctrl+S 时后端流式重写整个文件。 -->
<template>
  <div class="nt-sheet">
    <div class="nt-sheet-bar">
      <ElButton size="small" :disabled="!hasHeader || saving" @click="toggleHeader">
        <NtIcon name="list" size="sm" />
        <span class="nt-btn-text">{{ t('sheet.showHeader') }}</span>
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

    <div v-else-if="loading" class="nt-sheet-loading">
      <span class="nt-spinner" />
      <span class="nt-readout">{{ t('sheet.indexing') }}</span>
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
        <!-- 表头行：sticky 吸顶。列字母行在外，表头值在内，两层都要吸 -->
        <div v-if="showHeader" class="nt-sheet-header">
          <div class="nt-sheet-cols">
            <span class="nt-sheet-corner">{{ t('sheet.colLabel') }}</span>
            <div
              v-for="c in totalCols"
              :key="c - 1"
              class="nt-sheet-head-cell"
              :style="{ width: `${colWidth(c - 1)}px` }"
            >
              <span class="nt-sheet-cell-text">{{ header[c - 1] || colName(c - 1) }}</span>
            </div>
          </div>
          <!-- 表头值行：has_header 才是真数据，否则这行只是列字母的补充 -->
          <div class="nt-sheet-cols nt-sheet-datalabels">
            <span class="nt-sheet-corner">{{ t('sheet.colLabel') }}</span>
            <div
              v-for="c in totalCols"
              :key="`d${c - 1}`"
              class="nt-sheet-head-cell"
              :style="{ width: `${colWidth(c - 1)}px` }"
            >
              <span class="nt-sheet-cell-text">{{ colName(c - 1) }}</span>
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
          >
            <span class="nt-sheet-gutter" :class="{ 'is-selected': row.orig != null && row.orig === selectedRow }">
              {{ row.label }}
            </span>
            <div
              v-for="c in totalCols"
              :key="c - 1"
              class="nt-sheet-cell"
              :class="{
                'is-editing': isEditing(row, c - 1),
                'is-hit': isHit(row, c - 1),
                'is-dirty': row.orig != null && isDirtyCell(row.orig, c - 1),
                'is-focus': isFocused(row, c - 1),
              }"
              :style="{ width: `${colWidth(c - 1)}px` }"
              @click.stop="startEdit(row, c - 1)"
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
              <span v-else class="nt-sheet-cell-text" :title="row.cells[c - 1]">
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
/** 键盘焦点格：方向键与 Tab 的定位锚点。 */
const focused = ref<{ orig: number; col: number } | null>(null)
/** 表头显示开关。 */
const showHeader = ref(true)
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
  header,
  hasHeader,
  totalRows,
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

function toggleHeader() {
  showHeader.value = !showHeader.value
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

/** 状态栏行数：表格的行数走同一事件通道。 */
function announceLines(lines: number) {
  document.dispatchEvent(new CustomEvent('nt-stats', { detail: { tabId: props.tab.id, lines } }))
}

/** 方向键在非编辑态下移动焦点格：表格里这是最基本的导航。 */
function onKeyNav(e: KeyboardEvent) {
  const f = focused.value
  if (!f || editing.value) return
  const target = e.target as HTMLElement | null
  // 输入框里的方向键归输入框自己。
  if (target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA') return
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
    default:
      return
  }
  e.preventDefault()
  scrollToCell(orig, col)
  focused.value = { orig, col }
  selectedRow.value = orig
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

.nt-sheet-datalabels {
  border-top: 1px solid var(--nt-border);
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
