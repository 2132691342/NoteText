<!-- XLSX 可编辑网格：值与公式可改，样式由后端「原地改 XML」保住。
     与 CSV 的 SheetEditor 分开：CSV 的正文就是文本、保存走写文件链路；
     XLSX 是二进制容器，保存走 open_workbook / save_workbook 的单元格补丁。 -->
<template>
  <div class="nt-sheet">
    <div class="nt-sheet-bar">
      <ElButton size="small" :disabled="loading" @click="addRow">
        <NtIcon name="plus" size="sm" />
        <span>{{ t('sheet.addRow') }}</span>
      </ElButton>
      <ElButton size="small" :disabled="!editingCell" @click="deleteRow">
        {{ t('sheet.deleteRow') }}
      </ElButton>
      <ElButton size="small" :disabled="loading" @click="addCol">{{ t('sheet.addCol') }}</ElButton>
      <ElButton size="small" :disabled="cols === 0" @click="deleteCol">
        {{ t('sheet.deleteCol') }}
      </ElButton>
      <span class="nt-toolbar-sep" />
      <ElInput
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
      <ElInput
        v-model="replaceText"
        size="small"
        class="nt-sheet-find"
        :placeholder="t('sheet.replacePlaceholder')"
      />
      <ElButton size="small" :disabled="findHits.length === 0" @click="replaceAll">
        {{ t('sheet.replaceAll', { n: findHits.length }) }}
      </ElButton>
      <span class="nt-toolbar-spacer" />
      <span class="nt-readout nt-sheet-meta">
        {{ t('sheet.rows', { n: rowCount }) }} · {{ t('sheet.cols', { n: cols }) }} ·
        {{ t('sheet.sheets', { n: workbook?.sheets.length ?? 0 }) }}
        <template v-if="dirtyCount > 0">
          · <span class="nt-sheet-dirty">{{ t('sheet.pending', { n: dirtyCount }) }}</span>
        </template>
      </span>
      <ElButton
        size="small"
        type="primary"
        :disabled="dirtyCount === 0 || loading"
        :loading="saving"
        @click="save"
      >
        {{ t('sheet.saveWorkbook') }}
      </ElButton>
      <ElButton size="small" text :disabled="dirtyCount === 0 || saving" @click="discard">
        {{ t('sheet.discardShort') }}
      </ElButton>
    </div>

    <div v-if="mountError" class="nt-mount-error">
      <span class="nt-mount-error-title">{{ t('editor.openFailedTitle') }}</span>
      <span class="nt-mount-error-detail">{{ mountError }}</span>
    </div>

    <template v-else>
      <div ref="vlHost" class="nt-sheet-body" @scroll="onScroll">
        <div class="nt-sheet-inner" :style="{ height: `${totalHeight}px` }">
          <div class="nt-sheet-window" :style="{ transform: `translateY(${offset}px)` }">
            <div v-for="ri in windowItems" :key="ri" class="nt-sheet-row">
              <span class="nt-sheet-gutter">{{ ri + 1 }}</span>
              <div
                v-for="ci in cols"
                :key="ci"
                class="nt-sheet-cell"
                :class="{
                  'is-editing': editingCell?.r === ri && editingCell?.c === ci - 1,
                  'is-hit': isHit(ri, ci - 1),
                }"
                :style="{ width: cellWidth(ci - 1) }"
                @click="startEdit(ri, ci - 1)"
              >
                <input
                  v-if="editingCell?.r === ri && editingCell?.c === ci - 1"
                  ref="editEl"
                  v-model="editingCell.draft"
                  class="nt-sheet-input"
                  spellcheck="false"
                  @keydown.enter.prevent="commitEdit(ri, ci - 1, 0)"
                  @keydown.tab.prevent="commitEdit(ri, ci - 1, 1)"
                  @keydown.esc.prevent="cancelEdit"
                  @blur="commitEdit(ri, ci - 1, 0)"
                />
                <span v-else class="nt-sheet-cell-text" :title="cell(ri, ci - 1)">
                  {{ cell(ri, ci - 1) }}
                </span>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- 工作表页签：与 Excel 一致放底部 -->
      <div v-if="(workbook?.sheets.length ?? 0) > 1" class="nt-doc-sheet-tabs">
        <button
          v-for="s in workbook?.sheets ?? []"
          :key="s.index"
          class="nt-sheet-tab"
          :class="{ 'is-on': s.index === activeIndex }"
          @click="selectSheet(s.index)"
        >
          {{ s.name }}
        </button>
      </div>
    </template>
  </div>
</template>

<script lang="ts" setup>
import { computed, nextTick, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElInput } from 'element-plus'
import { api, normalizeError } from '@/api'
import type { CellEdit, CellValue, SheetHit, SheetVO, WorkbookVO } from '@/types/api'
import type { Tab } from '@/stores/editor'
import { useEditorStore } from '@/stores/editor'
import { useVirtualList } from '@/composables/useVirtualList'
import { useToastStore } from '@/stores/toast'
import NtIcon from '@/components/common/NtIcon.vue'

const ROW_H = 26
/** 默认列宽（px）与上限：与后端 col_widths 的语义对齐（Excel 字符宽 × 7.4）。 */
const DEFAULT_COL_W = 120
const MAX_COL_W = 360

const props = defineProps<{ tab: Tab }>()
const { t } = useI18n()
const editor = useEditorStore()
const toast = useToastStore()

const workbook = ref<WorkbookVO | null>(null)
const activeIndex = ref(0)
const mountError = ref('')
const loading = ref(false)
const saving = ref(false)

/** 单元格编辑态：r/c 是 0 基行列。 */
const editingCell = ref<{ r: number; c: number; draft: string } | null>(null)
const editEl = ref<HTMLInputElement | null>(null)

/** 待写回的改动：key = `r:c`。只在保存时一次性提交。 */
const pending = ref<Map<string, CellValue | null>>(new Map())
const dirtyCount = computed(() => pending.value.size)

/** 查找状态。 */
const findText = ref('')
const replaceText = ref('')
const findHits = ref<SheetHit[]>([])

const sheet = computed<SheetVO | null>(
  () => workbook.value?.sheets.find((s) => s.index === activeIndex.value) ?? null,
)

/** 网格尺寸：按 extent 与「用户加过行 / 列」取大。 */
const rowCount = ref(0)
const cols = computed(() => {
  const s = sheet.value
  let n = s?.extent ? s.extent[3] + 1 : 0
  // 取加过的最大列
  for (const [key] of pending.value) {
    const c = Number(key.split(':')[1]) + 1
    if (c > n) n = c
  }
  return Math.max(n, 1) + (extraCols.value || 0)
})
const extraCols = ref(0)
const extraRows = ref(0)

/** 单元格取值：待写回改动优先，其次工作簿里的值。 */
function cell(r: number, c: number): string {
  const k = `${r}:${c}`
  const p = pending.value.get(k)
  if (p !== undefined) return p === null ? '' : display(p)
  const s = sheet.value
  if (!s) return ''
  const hit = s.cells.find((x) => x.row === r && x.col === c)
  return hit ? display(hit.value) : ''
}

function display(v: CellValue): string {
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

/** 列宽：取后端的 col_widths，缺省给默认宽。 */
function cellWidth(c: number): string {
  const w = sheet.value?.col_widths[c]
  if (!w) return `${DEFAULT_COL_W}px`
  return `${Math.min(MAX_COL_W, Math.max(56, Math.round(w * 7.4) + 16))}px`
}

/** 查找命中高亮。 */
function isHit(r: number, c: number): boolean {
  return findHits.value.some((h) => h.sheet_index === activeIndex.value && h.row === r && h.col === c)
}

// ---- 虚拟滚动 ----

const indices = ref<number[]>([])
function rebuildIndices() {
  indices.value = Array.from({ length: rowCount.value }, (_, i) => i)
}
const { host: vlHost, totalHeight, windowItems, offset, onScroll } = useVirtualList(indices, ROW_H)

function syncRowCount() {
  const s = sheet.value
  const base = s?.extent ? s.extent[1] + 1 : 0
  rowCount.value = Math.max(base + extraRows.value, 1)
  rebuildIndices()
}

// ---- 载入 / 切换工作表 ----

async function load() {
  if (!props.tab.path) {
    mountError.value = t('sheet.needFile')
    return
  }
  loading.value = true
  try {
    workbook.value = await api.sheet.open(props.tab.path)
    activeIndex.value = workbook.value.sheets[0]?.index ?? 0
    pending.value = new Map()
    extraRows.value = 0
    extraCols.value = 0
    findHits.value = []
    syncRowCount()
  } catch (e) {
    mountError.value = normalizeError(e).message
  } finally {
    loading.value = false
  }
}

function selectSheet(index: number) {
  if (pending.value.size > 0) {
    // 有未保存的改动：提醒一句，但允许切（改动按表分开存，不会串）。
    toast.info(t('sheet.switchWithPending'))
  }
  activeIndex.value = index
  syncRowCount()
}

// ---- 单元格编辑 ----

function startEdit(r: number, c: number) {
  editingCell.value = { r, c, draft: cell(r, c) }
  void nextTick(() => editEl.value?.focus())
}

function cancelEdit() {
  editingCell.value = null
}

/** 提交编辑。`move` 为 1 时（Tab）顺手跳到右边一格。 */
function commitEdit(r: number, c: number, move: number) {
  const ed = editingCell.value
  if (!ed) return
  const text = ed.draft
  editingCell.value = null
  const before = cell(r, c)
  if (text === before) {
    if (move === 1) startEdit(r, c + 1)
    return
  }
  // 值类型推断：纯数字存成数值，其余按文本。公式以 = 开头。
  const next = parseInput(text)
  const map = new Map(pending.value)
  map.set(`${r}:${c}`, next)
  pending.value = map
  if (move === 1) startEdit(r, c + 1)
}

/** 输入文本 → 单元格值。 */
function parseInput(text: string): CellValue | null {
  if (text === '') return null
  if (text.startsWith('=') && text.length > 1) {
    return { kind: 'formula', formula: text.slice(1), cached: '' }
  }
  if (/^-?\d+(\.\d+)?([eE][+-]?\d+)?$/.test(text.trim())) {
    return { kind: 'number', raw: text.trim() }
  }
  if (/^(TRUE|FALSE)$/i.test(text.trim())) {
    return { kind: 'boolean', value: text.trim().toUpperCase() === 'TRUE' }
  }
  return { kind: 'text', text }
}

// ---- 行列增删 ----

function addRow() {
  extraRows.value += 1
  syncRowCount()
}

function deleteRow() {
  const ed = editingCell.value
  if (!ed) return
  // 删行 = 把这一整行所有单元格清空（XLSX 没有「物理删行」的轻量语义）。
  const map = new Map(pending.value)
  const s = sheet.value
  for (let c = 0; c < cols.value; c += 1) {
    const has = s?.cells.some((x) => x.row === ed.r && x.col === c)
    if (has || map.has(`${ed.r}:${c}`)) map.set(`${ed.r}:${c}`, null)
  }
  pending.value = map
  editingCell.value = null
  toast.info(t('sheet.rowCleared'))
}

function addCol() {
  extraCols.value += 1
}

function deleteCol() {
  const n = cols.value
  if (n <= 1) return
  // 删末列 = 把该列所有单元格清空。
  const map = new Map(pending.value)
  const s = sheet.value
  for (let r = 0; r < rowCount.value; r += 1) {
    const has = s?.cells.some((x) => x.row === r && x.col === n - 1)
    if (has || map.has(`${r}:${n - 1}`)) map.set(`${r}:${n - 1}`, null)
  }
  pending.value = map
  if (extraCols.value > 0) extraCols.value -= 1
}

// ---- 查找 / 替换 ----

async function runFind() {
  if (!findText.value || !props.tab.path) {
    findHits.value = []
    return
  }
  try {
    const res = await api.sheet.find({
      path: props.tab.path,
      sheet_index: null,
      find: findText.value,
      match_case: false,
      whole_cell: false,
    })
    findHits.value = res.hits ?? []
    if (findHits.value.length === 0) {
      toast.info(t('dialog.noMatches'))
    } else {
      // 跳到第一个命中所在地的表
      const first = findHits.value[0]
      if (first.sheet_index !== activeIndex.value) {
        activeIndex.value = first.sheet_index
        syncRowCount()
      }
      toast.ok(t('sheet.findHits', { n: res.total }))
    }
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}

/** 全部替换：把命中格的文本里的查找词换掉（不动公式格，避免改坏表达式）。 */
function replaceAll() {
  if (findHits.value.length === 0) return
  const map = new Map(pending.value)
  let n = 0
  for (const h of findHits.value) {
    // 只替换当前表的命中：跨表替换容易改错，用户切到那张表再替换一次更安全。
    if (h.sheet_index !== activeIndex.value) continue
    const cur = cell(h.row, h.col)
    if (cur.includes('=')) continue
    const nextText = cur.split(findText.value).join(replaceText.value)
    map.set(`${h.row}:${h.col}`, nextText === '' ? null : { kind: 'text', text: nextText })
    n += 1
  }
  pending.value = map
  findHits.value = []
  toast.ok(t('sheet.replaced', { n }))
}

// ---- 保存 ----

async function save() {
  if (!workbook.value || !props.tab.path || pending.value.size === 0) return
  saving.value = true
  try {
    const edits: CellEdit[] = []
    for (const [key, value] of pending.value) {
      const [r, c] = key.split(':').map(Number)
      edits.push({ row: r, col: c, value })
    }
    const res = await api.sheet.save({
      path: props.tab.path,
      sheet_index: activeIndex.value,
      edits,
      backup: true,
    })
    toast.ok(
      res.backup_path
        ? t('sheet.savedWithBackup', { n: res.changed })
        : t('sheet.saved', { n: res.changed }),
    )
    // 保存后重读：让网格与磁盘一致（新值类型可能变了）。
    await load()
    editor.markDirty(props.tab.id, false)
  } catch (e) {
    toast.bad(t('error.writeFailed', { reason: normalizeError(e).message }))
  } finally {
    saving.value = false
  }
}

function discard() {
  pending.value = new Map()
  toast.info(t('sheet.discarded'))
}

onMounted(() => {
  void load()
})
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

.nt-toolbar-sep {
  width: 1px;
  height: calc(var(--nt-h-toolbar) - 12px);
  background: var(--nt-border);
  flex: none;
}

.nt-toolbar-spacer {
  flex: 1;
}

.nt-sheet-find {
  width: 160px;
  flex: none;
}

.nt-sheet-meta {
  margin-left: var(--nt-sp-2);
  color: var(--nt-ink-muted);
  white-space: nowrap;
}

.nt-sheet-dirty {
  color: var(--nt-warning);
}

.nt-sheet-body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  font-size: var(--nt-fs-sm);
}

.nt-sheet-inner {
  position: relative;
}

.nt-sheet-window {
  position: absolute;
  left: 0;
  right: 0;
}

.nt-sheet-row {
  display: flex;
  height: 26px;
}

.nt-sheet-gutter {
  flex: none;
  width: 56px;
  padding: 0 var(--nt-sp-2);
  display: flex;
  align-items: center;
  justify-content: flex-end;
  color: var(--nt-ink-muted);
  background: var(--nt-editor-gutter-bg);
  border-bottom: 1px solid var(--nt-border);
  border-right: 1px solid var(--nt-border);
  font-size: var(--nt-fs-hint);
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
}

.nt-sheet-cell:hover {
  background: var(--nt-surface-hover);
}

.nt-sheet-cell.is-hit {
  background: var(--nt-warning-bg);
}

.nt-sheet-cell.is-editing {
  padding: 0;
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

/* 工作表页签：与 DocPreview 同一套视觉 */
.nt-doc-sheet-tabs {
  flex: none;
  display: flex;
  align-items: center;
  gap: 2px;
  padding: var(--nt-sp-1) var(--nt-sp-2);
  overflow-x: auto;
  background: var(--nt-chrome);
  border-top: 1px solid var(--nt-border);
}

.nt-sheet-tab {
  flex: none;
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  height: calc(var(--nt-ctl-h-sm) - 2px);
  padding: 0 var(--nt-sp-3);
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-2);
  background: transparent;
  border: 1px solid transparent;
  border-radius: var(--nt-radius-xs);
  cursor: pointer;
  transition:
    color var(--nt-dur-fast) var(--nt-ease),
    background-color var(--nt-dur-fast) var(--nt-ease),
    border-color var(--nt-dur-fast) var(--nt-ease);
}

.nt-sheet-tab:hover {
  color: var(--nt-ink);
  background: var(--nt-surface-hover);
}

.nt-sheet-tab.is-on {
  color: var(--nt-primary);
  background: var(--nt-primary-soft);
  border-color: var(--nt-primary-soft);
}
</style>
