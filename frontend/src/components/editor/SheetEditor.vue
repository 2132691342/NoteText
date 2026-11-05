// CSV / TSV 可编辑网格：解析 / 序列化在前端，保存走通用写盘链路。
// 与后端 service::file::parse_delimited 同一套规则（引号包裹、"" 转义、
// 字段内换行）；写回做最小引号化，保证除有意编辑外往返字节一致。
<template>
  <div class="nt-sheet">
    <div class="nt-sheet-bar">
      <ElButton size="small" @click="addRow">
        <NtIcon name="plus" size="sm" />
        <span>{{ t('sheet.addRow') }}</span>
      </ElButton>
      <ElButton size="small" :disabled="editing == null" @click="deleteRow">
        {{ t('sheet.deleteRow') }}
      </ElButton>
      <ElButton size="small" @click="addCol">{{ t('sheet.addCol') }}</ElButton>
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
      <ElButton size="small" :disabled="hits.length === 0" @click="replaceAll">
        {{ t('sheet.replaceAll', { n: hits.length }) }}
      </ElButton>
      <span class="nt-readout nt-sheet-meta">
        {{ t('sheet.rows', { n: rowCount }) }} · {{ t('sheet.cols', { n: cols }) }} ·
        {{ delimiterLabel }}
      </span>
      <span v-if="error" class="nt-sheet-error">{{ error }}</span>
    </div>

    <div v-if="mountError" class="nt-mount-error">
      <span class="nt-mount-error-title">{{ t('editor.openFailedTitle') }}</span>
      <span class="nt-mount-error-detail">{{ mountError }}</span>
    </div>

    <div v-else ref="vlHost" class="nt-sheet-body" @scroll="onScroll">
      <div class="nt-sheet-inner" :style="{ height: `${totalHeight}px` }">
        <div class="nt-sheet-window" :style="{ transform: `translateY(${offset}px)` }">
          <div v-for="ri in windowItems" :key="ri" class="nt-sheet-row">
            <span class="nt-sheet-gutter">{{ ri + 1 }}</span>
            <div
              v-for="ci in cols"
              :key="ci"
              class="nt-sheet-cell"
              :class="{
                'is-editing': editing?.r === ri && editing?.c === ci - 1,
                'is-hit': isHit(ri, ci - 1),
              }"
              @click="startEdit(ri, ci - 1)"
            >
              <input
                v-if="editing?.r === ri && editing?.c === ci - 1"
                ref="editEl"
                v-model="editing.draft"
                class="nt-sheet-input"
                spellcheck="false"
                @keydown.enter.prevent="commitEdit(ri, ci - 1, 0)"
                @keydown.tab.prevent="commitEdit(ri, ci - 1, 1)"
                @keydown.esc.prevent="cancelEdit"
                @blur="commitEdit(ri, ci - 1, 0)"
              />
              <span v-else class="nt-sheet-cell-text">{{ cell(ri, ci - 1) }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElInput } from 'element-plus'
import { api, normalizeError } from '@/api'
import type { Tab } from '@/stores/editor'
import { useEditorStore } from '@/stores/editor'
import {
  registerTextProvider,
  unregisterTextProvider,
  stashDoc,
  takeStashedDoc,
  consumePendingContent,
  isDirty,
  bumpDocVersion,
} from '@/composables/useEditorRegistry'
import { useVirtualList } from '@/composables/useVirtualList'
import { useToastStore } from '@/stores/toast'
import NtIcon from '@/components/common/NtIcon.vue'

/** 行高：与 CSS 的行高保持一致。 */
const ROW_H = 26

const props = defineProps<{ tab: Tab }>()
const { t } = useI18n()
const editor = useEditorStore()
const toast = useToastStore()

const rows = ref<string[][]>([])
const delimiter = ref(',')
/** 载入文本是否以换行收尾：写回时保持原样，往返才字节一致。 */
const endsWithNewline = ref(false)
const mountError = ref('')
const error = ref('')
/** 单元格编辑态：r/c 是 0 基行列，draft 是编辑中的草稿。 */
const editing = ref<{ r: number; c: number; draft: string } | null>(null)
const editEl = ref<HTMLInputElement | null>(null)

const rowCount = computed(() => rows.value.length)
const cols = computed(() => rows.value.reduce((m, r) => Math.max(m, r.length), 0))
const delimiterLabel = computed(() =>
  delimiter.value === '\t'
    ? 'TAB'
    : delimiter.value === ';'
      ? t('sheet.delimSemicolon')
      : delimiter.value === '|'
        ? t('sheet.delimPipe')
        : t('sheet.delimComma'),
)

function cell(r: number, c: number): string {
  return rows.value[r]?.[c] ?? ''
}

// ---- 解析 / 序列化（与后端 parse_delimited 同规则） ----

function parseCsv(text: string, delim: string): string[][] {
  const out: string[][] = []
  let row: string[] = []
  let field = ''
  let quoted = false
  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i]
    if (quoted) {
      if (ch === '"') {
        if (text[i + 1] === '"') {
          field += '"'
          i += 1
        } else {
          quoted = false
        }
      } else {
        field += ch
      }
      continue
    }
    if (ch === '"' && field === '') quoted = true
    else if (ch === delim) {
      row.push(field)
      field = ''
    } else if (ch === '\r') continue
    else if (ch === '\n') {
      row.push(field)
      out.push(row)
      row = []
      field = ''
    } else field += ch
  }
  if (field !== '' || row.length > 0) {
    row.push(field)
    out.push(row)
  }
  return out
}

/** 最小引号化：含分隔符 / 引号 / 换行的字段才包引号。 */
function serialize(list: string[][], delim: string): string {
  const body = list
    .map((r) =>
      r
        .map((f) =>
          f.includes(delim) || f.includes('"') || f.includes('\n') || f.includes('\r')
            ? `"${f.replace(/"/g, '""')}"`
            : f,
        )
        .join(delim),
    )
    .join('\n')
  return body.length > 0 && endsWithNewline.value ? `${body}\n` : body
}

/** 分隔符嗅探：前 10 行里出现次数最多的候选（与后端 sniff_delimiter 同构）。 */
function sniffDelim(text: string): string {
  const cands = [',', '\t', ';', '|']
  let bestHits = 0
  let best = ','
  for (const c of cands) {
    const hits = text
      .split('\n')
      .slice(0, 10)
      .reduce((n, l) => n + l.split(c).length - 1, 0)
    if (hits > bestHits) {
      bestHits = hits
      best = c
    }
  }
  return best
}

function loadFromText(text: string) {
  delimiter.value = sniffDelim(text)
  endsWithNewline.value = text.endsWith('\n')
  rows.value = parseCsv(text, delimiter.value)
}

// ---- 保存链路接入：注册文本适配器，Ctrl+S / 退出确认原样可用 ----

function serialized(): string {
  return serialize(rows.value, delimiter.value)
}

function syncDirty() {
  const dirty = isDirty(props.tab.id)
  if (props.tab.dirty !== dirty) editor.markDirty(props.tab.id, dirty)
  bumpDocVersion(props.tab.id)
}

onMounted(async () => {
  // 正文来源：路由往返的暂存（未保存修改）> openPath 已读到的暂存 > 读文件。
  const stashed = takeStashedDoc(props.tab.id)
  const pending = stashed != null ? null : consumePendingContent(props.tab.id)
  const text = stashed ?? pending
  if (text != null) {
    loadFromText(text)
  } else if (props.tab.path && !props.tab.path.startsWith('draft:')) {
    try {
      const c = await api.readFile(props.tab.path)
      loadFromText(c.text)
      props.tab.encoding = c.encoding
      props.tab.line_ending = c.line_ending
      props.tab.has_bom = c.has_bom
    } catch (e) {
      mountError.value = normalizeError(e).message
      return
    }
  } else {
    loadFromText('')
  }

  registerTextProvider(
    props.tab.id,
    {
      getText: () => serialized(),
      replaceText: (next) => loadFromText(next),
    },
    serialized(),
  )
  editor.markDirty(props.tab.id, false)
  announceLines()
})

onBeforeUnmount(() => {
  // 脏正文先暂存：路由离开编辑器视图再回来时从暂存找回。
  if (props.tab.id != null && isDirty(props.tab.id)) stashDoc(props.tab.id, serialized())
  unregisterTextProvider(props.tab.id)
})

/** 状态栏行数：表格的行数就是 CSV 的行数，走同一事件通道。 */
function announceLines() {
  document.dispatchEvent(
    new CustomEvent('nt-stats', { detail: { tabId: props.tab.id, lines: rowCount.value } }),
  )
}

// ---- 虚拟滚动 ----

/** 稳定的行号数组：只在行数变化时重建（每次编辑重建会触发滚动归零）。 */
const indices = ref<number[]>([])
watch(rowCount, (n) => {
  indices.value = Array.from({ length: n }, (_, i) => i)
  announceLines()
})
const { host: vlHost, totalHeight, windowItems, offset, onScroll } = useVirtualList(
  indices,
  ROW_H,
)

// ---- 单元格编辑 ----

function startEdit(r: number, c: number) {
  editing.value = { r, c, draft: cell(r, c) }
  void nextTick(() => editEl.value?.focus())
}

function cancelEdit() {
  editing.value = null
}

/** 提交编辑。`move` 为 1 时（Tab 键）顺手跳到右边一格。 */
function commitEdit(r: number, c: number, move: number) {
  const ed = editing.value
  if (!ed) return
  const row = rows.value[r] ?? []
  while (row.length <= c) row.push('')
  const changed = row[c] !== ed.draft
  row[c] = ed.draft
  rows.value[r] = row
  editing.value = null
  if (changed) syncDirty()
  if (move === 1) startEdit(r, c + 1)
}

// ---- 行列增删 ----

function addRow() {
  const width = Math.max(cols.value, 1)
  const blank = Array.from({ length: width }, () => '')
  if (editing.value) {
    rows.value.splice(editing.value.r + 1, 0, blank)
  } else {
    rows.value.push(blank)
  }
  syncDirty()
}

function deleteRow() {
  if (!editing.value) return
  rows.value.splice(editing.value.r, 1)
  editing.value = null
  syncDirty()
}

function addCol() {
  for (const row of rows.value) row.push('')
  syncDirty()
}

function deleteCol() {
  const n = cols.value
  if (n === 0) return
  for (const row of rows.value) row.splice(n - 1)
  syncDirty()
}

// ---- 查找 / 替换 ----

const findText = ref('')
const replaceText = ref('')
/** 命中位置：`r:c`。前端已全量在内存里，不必走 IPC。 */
const hits = ref<Array<{ r: number; c: number }>>([])

function isHit(r: number, c: number): boolean {
  return hits.value.some((h) => h.r === r && h.c === c)
}

/** 扫描全表：包含匹配、不区分大小写（与 XLSX 那边的默认行为一致）。 */
function runFind() {
  const needle = findText.value
  if (!needle) {
    hits.value = []
    return
  }
  const lower = needle.toLowerCase()
  const found: Array<{ r: number; c: number }> = []
  for (let r = 0; r < rows.value.length; r += 1) {
    const row = rows.value[r] ?? []
    for (let c = 0; c < row.length; c += 1) {
      if ((row[c] ?? '').toLowerCase().includes(lower)) found.push({ r, c })
    }
  }
  hits.value = found
  if (found.length === 0) {
    toast.info(t('dialog.noMatches'))
  } else {
    toast.ok(t('sheet.findHits', { n: found.length }))
    // 滚到第一条：长表上只高亮不滚动等于没反馈。
    const first = found[0]
    if (vlHost.value) {
      vlHost.value.scrollTop = Math.max(0, first.r * ROW_H - ROW_H * 3)
    }
  }
}

/** 全部替换：只改命中格的文本，保留其余字节。 */
function replaceAll() {
  if (hits.value.length === 0) return
  const needle = findText.value
  for (const h of hits.value) {
    const row = rows.value[h.r]
    if (!row) continue
    row[h.c] = (row[h.c] ?? '').split(needle).join(replaceText.value)
  }
  const n = hits.value.length
  hits.value = []
  syncDirty()
  toast.ok(t('sheet.replaced', { n }))
}

// 编码探测失败这类运行期问题给条内联提示，不打断网格。
watch(error, (v) => {
  if (v) toast.bad(v)
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

.nt-sheet-meta {
  margin-left: var(--nt-sp-2);
  color: var(--nt-ink-muted);
  white-space: nowrap;
}

.nt-sheet-find {
  width: 160px;
  flex: none;
}

.nt-toolbar-sep {
  width: 1px;
  height: calc(var(--nt-h-toolbar) - 12px);
  background: var(--nt-border);
  flex: none;
}

.nt-sheet-cell.is-hit {
  background: var(--nt-warning-bg);
}

.nt-sheet-error {
  color: var(--nt-danger);
  font-size: var(--nt-fs-hint);
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
