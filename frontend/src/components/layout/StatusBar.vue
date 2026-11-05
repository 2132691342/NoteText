// 状态栏：修改指示、光标位置、语言、编码、行尾、缩放。
// 显示项可勾选（右键菜单）：有人只要光标位置，有人要全看。
<template>
  <div class="nt-statusbar" @contextmenu.prevent="showMenu = true">
    <span class="nt-status-state" :class="{ 'is-dirty': tab?.dirty }">
      <span class="nt-dot" :class="{ 'is-clean': !tab?.dirty }" />
      {{ tab?.dirty ? t('status.modified') : t('status.saved') }}
    </span>

    <template v-if="visible('cursor')">
      <span class="nt-readout">
        {{ t('status.line') }} {{ pos.line }}, {{ t('status.column') }} {{ pos.col }}
        <template v-if="pos.selection_len > 0">
          （{{ t('status.selected') }} {{ pos.selection_len }}）
        </template>
      </span>
    </template>

    <template v-if="visible('lines')">
      <span class="nt-readout" :title="statsTitle" @mouseenter="onLinesHover">
        {{ t('status.lines') }} {{ lines }}
      </span>
    </template>

    <div class="nt-statusbar-spacer" />

    <!-- 压缩任务指示：有活动任务才出现，点击开任务面板 -->
    <button v-if="archive.activeCount > 0" class="nt-status-btn nt-status-archive"
      :title="t('menu.archiveTasks')" @click="dialogs.open('archiveTasks')">
      <NtIcon name="archive" size="sm" />
      {{ archive.activeCount }}
    </button>

    <template v-if="visible('language')">
      <button class="nt-status-btn" @click="commands.run('view.language')">{{ langLabel }}</button>
    </template>

    <template v-if="visible('encoding')">
      <button class="nt-status-btn" :class="{ 'is-warn': tab?.encoding_uncertain }" @click="commands.run('view.encoding')">
        {{ encodingShort(tab?.encoding ?? 'utf-8') }}
        <span v-if="tab?.encoding_uncertain" :title="t('toast.encodingChanged')">?</span>
      </button>
    </template>

    <template v-if="visible('lineEnding')">
      <button class="nt-status-btn" @click="commands.run('view.lineEnding')">
        {{ endingLabel }}
      </button>
    </template>

    <template v-if="visible('zoom')">
      <button class="nt-status-btn" @click="commands.run('view.resetZoom')">
        {{ ui.zoom_level }}%
      </button>
    </template>

    <span v-if="tab?.truncated" class="nt-readout nt-status-readonly" :title="truncatedTitle">
      {{ t('status.partial') }}
    </span>
    <span v-else-if="tab?.read_only" class="nt-readout nt-status-readonly">
      {{ t('status.readOnly') }}
    </span>

    <!-- 显示项勾选 -->
    <Teleport to="body">
      <div
        v-if="showMenu"
        class="nt-ctx-menu"
        :style="{ right: '8px', bottom: 'calc(var(--nt-h-status) + 4px)', left: 'auto' }"
        @click="showMenu = false"
      >
        <label v-for="item in ITEMS" :key="item.key" class="nt-ctx-check">
          <input type="checkbox" :checked="visible(item.key)" @change="toggle(item.key)" />
          {{ t(item.labelKey) }}
        </label>
      </div>
    </Teleport>
  </div>
</template>

<script lang="ts" setup>
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useEditorStore } from '@/stores/editor'
import { useSettingsStore } from '@/stores/settings'
import { useCommandStore } from '@/stores/commands'
import { useArchiveStore } from '@/stores/archive'
import { useDialogStore } from '@/stores/dialogs'
import NtIcon from '@/components/common/NtIcon.vue'
import { getPosition } from '@/composables/useEditorRegistry'
import { get, docVersionOf } from '@/composables/useEditorRegistry'
import { api } from '@/api'
import { formatSize } from '@/utils'
import { encodingShort } from '@/utils/encoding'

const { t } = useI18n()
const editor = useEditorStore()
const settings = useSettingsStore()
const commands = useCommandStore()
const archive = useArchiveStore()
const dialogs = useDialogStore()

const tab = computed(() => editor.active)

/** 截断提示：告诉用户读到了哪里，以及为何不能保存。 */
const truncatedTitle = computed(() =>
  t('status.partialHint', { size: formatSize(tab.value?.truncated_bytes ?? 0) }),
)
const ui = computed(() => settings.config.ui)

const pos = ref({ line: 1, col: 1, selection_len: 0 })
const lines = ref(0)
const showMenu = ref(false)
/** 详细统计（字符 / 词 / 非空行）：按需向后端要，悬停或点击行数时才拉。 */
const detail = ref<{ chars: number; words: number; non_blank_lines: number } | null>(null)
/** 统计对应的正文版本号：正文变了旧统计就作废。 */
let detailVersion = -1

/** 详细统计的文档上限：超过它不再给字符 / 词数（4MB）。 */
const DETAIL_STATS_LIMIT = 4 * 1024 * 1024

/** 只读读数不套胶囊，但要给得出细节——悬停就够，不额外占一层 UI。 */
const statsTitle = computed(() => {
  if (!detail.value) return ''
  const d = detail.value
  return `${t('status.chars')} ${d.chars} · ${t('status.words')} ${d.words} · ${t(
    'status.nonBlank',
  )} ${d.non_blank_lines}`
})

/** 详细统计按需拉取（悬停 / 点击行数读数），不随每次按键重算全文。 */
async function fetchStats() {
  const id = editor.activeId
  if (id == null) {
    detail.value = null
    return
  }
  const doc = get(id)?.state.doc
  if (!doc || doc.length > DETAIL_STATS_LIMIT) {
    detail.value = null
    return
  }
  const version = docVersionOf(id)
  if (version === detailVersion) return
  try {
    const s = await api.textStats(doc.toString())
    detailVersion = version
    detail.value = {
      chars: s.chars,
      words: s.words,
      non_blank_lines: s.non_blank_lines,
    }
  } catch {
    // 统计失败不影响编辑：静默降级为不显示细节。
    detail.value = null
  }
}

/** 悬停行数读数：这时候才真的需要字数 / 词数。 */
function onLinesHover() {
  if (!detail.value) void fetchStats()
}

/** 状态栏显示项：缺省全显示，用户在右键菜单里关。 */
const ITEMS = [
  { key: 'cursor', labelKey: 'status.cursorPos' },
  { key: 'lines', labelKey: 'status.lines' },
  { key: 'language', labelKey: 'status.language' },
  { key: 'encoding', labelKey: 'status.encoding' },
  { key: 'lineEnding', labelKey: 'status.lineEnding' },
  { key: 'zoom', labelKey: 'status.zoom' },
] as const

function visible(key: string): boolean {
  const items = ui.value.status_bar_items
  return items[key] !== false
}

function toggle(key: string) {
  settings.updateUI({
    status_bar_items: { ...ui.value.status_bar_items, [key]: !visible(key) },
  })
}

const langLabel = computed(() => (tab.value?.language ?? 'text').toUpperCase())

const endingLabel = computed(() => {
  switch (tab.value?.line_ending) {
    case 'lf':
      return 'LF'
    case 'cr':
      return 'CR'
    default:
      return 'CRLF'
  }
})

// 光标与行数由编辑器组件通过自定义事件推：状态栏不轮询。
/** 事件一律带 tabId：分屏与多标签同时挂载，别的标签的读数不能串过来。 */
function onCursor(e: Event) {
  const d = (e as CustomEvent).detail as { tabId?: number; line: number; col: number; selection_len: number }
  if (!d || d.tabId !== editor.activeId) return
  pos.value = { line: d.line, col: d.col, selection_len: d.selection_len }
}

function onStats(e: Event) {
  const d = (e as CustomEvent).detail as { tabId?: number; lines: number }
  if (!d || d.tabId !== editor.activeId) return
  lines.value = d.lines
}

/** 活跃标签变了主动拉一次：切换标签时编辑器不会重发光标事件。 */
function syncFromEditor() {
  const id = editor.activeId
  if (id == null) {
    pos.value = { line: 1, col: 1, selection_len: 0 }
    lines.value = 0
    return
  }
  pos.value = getPosition(id)
  const doc = get(id)?.state.doc
  lines.value = doc ? (doc.length === 0 ? 0 : doc.lines) : 0
  // 统计不进这里：切标签不该触发一次全文过 IPC，旧数据直接作废即可。
  detail.value = null
  detailVersion = -1
}

/** 编辑器挂载完成时同步一次读数，覆盖状态栏先于编辑器挂载的首屏。 */
function onEditorReady(e: Event) {
  const d = (e as CustomEvent).detail as { tabId?: number }
  if (!d || d.tabId !== editor.activeId) return
  syncFromEditor()
}

// 标签元数据里没有正文（AGENTS.md §1.2），所以编辑器挂载前读不出真实行数。
// 不猜：等 nt-editor-ready 或用户首次输入，读数随即被覆盖。

onMounted(() => {
  document.addEventListener('nt-cursor', onCursor)
  document.addEventListener('nt-stats', onStats)
  document.addEventListener('nt-editor-ready', onEditorReady)
  syncFromEditor()
})
onUnmounted(() => {
  document.removeEventListener('nt-cursor', onCursor)
  document.removeEventListener('nt-stats', onStats)
  document.removeEventListener('nt-editor-ready', onEditorReady)
})

// 切标签主动同步一次：编辑器在显示之前发的读数可能被过滤掉了。
watch(() => editor.activeId, () => syncFromEditor())
</script>

<style scoped>
.nt-status-state {
  display: inline-flex;
  align-items: center;
  gap: var(--nt-sp-1);
  flex: none;
}

.nt-status-state.is-dirty {
  color: var(--nt-primary);
}

.nt-statusbar-spacer {
  flex: 1;
}

.nt-status-btn {
  height: calc(var(--nt-h-status) - 4px);
  padding: 0 var(--nt-sp-2);
  border-radius: var(--nt-radius-xs);
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-2);
  font-variant-numeric: tabular-nums;
  transition:
    background-color var(--nt-dur-fast) var(--nt-ease),
    color var(--nt-dur-fast) var(--nt-ease);
}

.nt-status-btn:hover {
  background: var(--nt-surface-hover);
  color: var(--nt-ink);
}

/* 状态栏是读数带：按下只换底，不缩放（一行读数跟着颤很奇怪） */
.nt-status-btn:active {
  background: var(--nt-surface-selected);
  color: var(--nt-primary);
}

.nt-status-btn.is-warn {
  color: var(--nt-warning);
}

/* 压缩任务进行中：给一个有动作的指示（点击开面板），主色让它跳出来 */
.nt-status-archive {
  display: inline-flex;
  align-items: center;
  gap: var(--nt-sp-1);
  color: var(--nt-primary);
}

.nt-status-readonly {
  color: var(--nt-warning);
  flex: none;
}

.nt-ctx-check {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  height: calc(var(--nt-ctl-h) - 4px);
  padding: 0 var(--nt-sp-2);
  border-radius: var(--nt-radius-xs);
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink);
  cursor: pointer;
}

.nt-ctx-check:hover {
  background: var(--nt-surface-hover);
}
</style>
