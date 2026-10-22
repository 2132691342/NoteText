// 状态栏：修改指示、光标位置、语言、编码、行尾、缩放。
//
// 显示项可勾选（右键菜单）：有人只要光标位置，有人要全看。
// 读数一律等宽数字（tabular-nums），否则每次刷新宽度都在跳。
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
      <span class="nt-readout" :title="statsTitle">{{ t('status.lines') }} {{ lines }}</span>
    </template>

    <div class="nt-statusbar-spacer" />

    <template v-if="visible('language')">
      <button class="nt-status-btn" @click="commands.run('view.language')">{{ langLabel }}</button>
    </template>

    <template v-if="visible('encoding')">
      <button class="nt-status-btn" :class="{ 'is-warn': tab?.encoding_uncertain }" @click="commands.run('view.encoding')">
        {{ tab?.encoding ?? 'UTF-8' }}{{ tab?.has_bom ? ' BOM' : '' }}
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
import { getPosition } from '@/composables/useEditorRegistry'
import { getText } from '@/composables/useEditorRegistry'
import { api } from '@/api'
import { debounce, formatSize } from '@/utils'

const { t } = useI18n()
const editor = useEditorStore()
const settings = useSettingsStore()
const commands = useCommandStore()

const tab = computed(() => editor.active)

/** 截断提示：告诉用户读到了哪里，以及为何不能保存。 */
const truncatedTitle = computed(() =>
  t('status.partialHint', { size: formatSize(tab.value?.truncated_bytes ?? 0) }),
)
const ui = computed(() => settings.config.ui)

const pos = ref({ line: 1, col: 1, selection_len: 0 })
const lines = ref(0)
const showMenu = ref(false)
/** 详细统计（字符 / 词 / 非空行）：走后端 `get_text_stats`，悬停行数时给。 */
const detail = ref<{ chars: number; words: number; non_blank_lines: number } | null>(null)

/** 只读读数不套胶囊，但要给得出细节——悬停就够，不额外占一层 UI。 */
const statsTitle = computed(() => {
  if (!detail.value) return ''
  const d = detail.value
  return `${t('status.chars')} ${d.chars} · ${t('status.words')} ${d.words} · ${t(
    'status.nonBlank',
  )} ${d.non_blank_lines}`
})

/**
 * 统计走后端且防抖：每次按键都算一遍全文，大文件上会把输入卡住。
 * 400ms 与配置回写同一档，用户停手才拉。
 */
const fetchStats = debounce(async () => {
  const id = editor.activeId
  if (id == null) {
    detail.value = null
    return
  }
  try {
    const s = await api.textStats(getText(id))
    detail.value = {
      chars: s.chars,
      words: s.words,
      non_blank_lines: s.non_blank_lines,
    }
  } catch {
    // 统计失败不影响编辑：静默降级为不显示细节。
    detail.value = null
  }
}, 400)

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
  void fetchStats()
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
  lines.value = getText(id) === '' ? 0 : getText(id).split('\n').length
  void fetchStats()
}

/**
 * 编辑器挂载完成时同步一次。
 *
 * 首屏必须靠这个事件：状态栏比编辑器先挂载，此时注册表还是空的，
 * 只在 onMounted 里同步一次会停在「行 1, 列 1 / 总行数 0」，
 * 直到用户敲第一个字才对得上。
 */
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
