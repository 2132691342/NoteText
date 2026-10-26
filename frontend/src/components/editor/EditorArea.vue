// 编辑区：所有标签的编辑器实例常驻，只切换可见性。
// 关键不变量：**一个标签一个 CodeMirror 实例，切标签不销毁**。
<script lang="ts" setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton } from 'element-plus'
import CodeEditor from '@/components/editor/CodeEditor.vue'
import Minimap from '@/components/editor/Minimap.vue'
import MarkdownEditor from '@/components/editor/MarkdownEditor.vue'
import DocPreview from '@/components/editor/DocPreview.vue'
import HexView from '@/components/editor/HexView.vue'
import NtIcon from '@/components/common/NtIcon.vue'
import { useEditorStore, viewTypeOf } from '@/stores/editor'
import { useCommandStore } from '@/stores/commands'
import { useSettingsStore } from '@/stores/settings'
import { get, docVersionOf } from '@/composables/useEditorRegistry'
import { doGotoLine } from '@/composables/useEditorActions'
import { api } from '@/api'
import { ellipsisMiddle } from '@/utils'
import { hasRichPreview, isTextKind, kindOf } from '@/utils/fileKind'
import type { RecentFileVO } from '@/types/api'
import type { Tab } from '@/stores/editor'

const { t } = useI18n()
const editor = useEditorStore()
const commands = useCommandStore()
const settings = useSettingsStore()

/** 欢迎页的最近文件：只显示前 6 条，全量走浮窗。 */
const recent = ref<RecentFileVO[]>([])

async function loadRecent() {
  if (editor.tabs.length > 0) return
  try {
    const list = (await api.recentFiles()) ?? []
    recent.value = list.filter((f) => f.exists).slice(0, 6)
  } catch {
    /* 最近文件读不出来不该挡住新建 / 打开 */
  }
}

async function onOpenRecent(path: string) {
  await editor.openPath(path)
}

/**
 * 标签该用哪个视图组件。
 *
 * 「这个文件是什么形态」由 `utils/fileKind` 判定，组件只按形态选渲染器：
 * 二进制 → Hex；有专属预览器 → DocPreview；Markdown 默认进三态编辑器，
 * 用户按了「预览」才落到只读预览；其余进 CodeMirror。
 */
function viewOfTab(tab: Tab): 'hex' | 'markdown' | 'doc' | 'code' {
  const kind = kindOf(tab.path, tab.view_type === 'hex')
  if (tab.view_type === 'hex' || kind === 'binary') return 'hex'
  // 分屏时一律回到三态编辑器：只读预览器没有第二栏，分屏按钮会变成假动作。
  if (kind === 'markdown') return tab.showing_preview && !tab.split ? 'doc' : 'markdown'
  if (hasRichPreview(kind)) return 'doc'
  return tab.showing_preview ? 'doc' : 'code'
}

/** 切到十六进制：预览器给的是「这种格式读不了」时的兜底入口。 */
function toHex(tab: Tab) {
  tab.view_type = 'hex'
  tab.showing_preview = false
}

/** 从十六进制切回编辑器：只有文本文件给这个入口。 */
function toEditor(tab: Tab) {
  tab.view_type = viewTypeOf(tab.path)
  tab.showing_preview = false
}

/**
 * 已经挂载过的标签 id。
 *
 * 为什么要记：v-for 全量挂载会让启动时把恢复出来的十几个文件全部读进内存
 * （每个都是完整 CodeMirror 实例）。改成「首次激活才挂载，之后常驻」：
 * 既不为没看过的文件付代价，也保证看过的不被销毁。
 */
const mounted = ref<Set<number>>(new Set())

watch(
  () => editor.activeId,
  (id) => {
    if (id != null && !mounted.value.has(id)) {
      const next = new Set(mounted.value)
      next.add(id)
      mounted.value = next
    }
  },
  { immediate: true },
)
// 标签被关掉后清掉记录：id 不复用，但集合不该无限增长。
watch(
  () => editor.tabs.length,
  () => {
    const alive = new Set(editor.tabs.map((x) => x.id))
    let changed = false
    const next = new Set<number>()
    for (const id of mounted.value) {
      if (alive.has(id)) next.add(id)
      else changed = true
    }
    if (changed) mounted.value = next
  },
)

/** 活跃标签的主视图（Minimap 要拿它）。 */
const mainView = computed(() => {
  const id = editor.activeId
  return id != null ? (get(id) ?? null) : null
})

const lineCount = computed(() => {
  const id = editor.activeId
  if (id == null) return 0
  // 先读版本号建立依赖：注册表是普通 Map，直接 getText 的话值永远停在首帧。
  docVersionOf(id)
  // 行数从 EditorState 取：几 MB 的正文每次按键都 split 一遍会卡输入。
  return get(id)?.state.doc.lines ?? 0
})

// ---- 分屏比例 ----
/** 标签槽在分屏时的固定宽度：主栏占 `1 - splitRatio`，副栏吃满剩余。 */
function slotStyle(tab: Tab) {
  // Markdown 的分屏由 MarkdownEditor 自己排版，这里不给它定宽。
  const on = editor.active?.id === tab.id && tab.split && viewOfTab(tab) === 'code'
  return on ? { flex: `0 0 ${((1 - editor.splitRatio) * 100).toFixed(2)}%` } : undefined
}

function startSplitDrag(e: MouseEvent) {
  const startX = e.clientX
  const startRatio = editor.splitRatio
  const width = (e.currentTarget as HTMLElement).parentElement?.clientWidth ?? 1
  const onMove = (ev: MouseEvent) => {
    const next = startRatio + (ev.clientX - startX) / width
    editor.splitRatio = Math.min(0.8, Math.max(0.2, next))
  }
  const onUp = () => {
    document.removeEventListener('mousemove', onMove)
    document.removeEventListener('mouseup', onUp)
  }
  document.addEventListener('mousemove', onMove)
  document.addEventListener('mouseup', onUp)
}

/**
 * 面板里的「跳到某行」（书签 / 符号 / 查找结果）。
 *
 * 面板只发意图，跳转必须落在这里：它知道标签有没有挂载。首次激活的标签
 * 还没注册编辑器实例，等几帧再跳，否则事件发出去时没人接。
 */
async function onGotoLine(e: Event) {
  const d = (e as CustomEvent).detail as { tabId?: number; line?: number } | undefined
  if (d?.tabId == null || !d.line) return
  editor.activate(d.tabId)
  for (let i = 0; i < 12; i += 1) {
    if (get(d.tabId)) {
      doGotoLine(d.tabId, d.line)
      return
    }
    await new Promise((r) => requestAnimationFrame(r))
  }
}

// ---- 拖放提示层 ----
// 外部拖放的监听与打开链路在 useFileDrop（WebView 原生 drop 事件），这里只消费它的开关信号。
const dragging = ref(false)

function onDragState(e: Event) {
  dragging.value = !!(e as CustomEvent).detail?.active
}

onMounted(() => {
  void loadRecent()
  document.addEventListener('nt-drag-state', onDragState)
  document.addEventListener('editor-goto-line', onGotoLine)
})
onBeforeUnmount(() => {
  document.removeEventListener('nt-drag-state', onDragState)
  document.removeEventListener('editor-goto-line', onGotoLine)
})
</script>

<template>
  <div class="nt-editor-area">
    <div v-if="!editor.active" class="nt-welcome">
      <div class="nt-welcome-logo">N</div>
      <h2 class="nt-welcome-title">NoteText</h2>
      <p class="nt-welcome-hint">{{ t('welcome.hint') }}</p>
      <div class="nt-welcome-actions">
        <ElButton type="primary" @click="commands.run('file.new')">
          <NtIcon name="plus" size="sm" />
          <span class="nt-btn-text">{{ t('welcome.newFile') }}</span>
        </ElButton>
        <ElButton @click="commands.run('file.open')">
          <NtIcon name="folder-open" size="sm" />
          <span class="nt-btn-text">{{ t('welcome.openFile') }}</span>
        </ElButton>
        <ElButton @click="commands.run('file.openFolder')">
          <NtIcon name="folder" size="sm" />
          <span class="nt-btn-text">{{ t('welcome.openFolder') }}</span>
        </ElButton>
      </div>

      <!-- 最近文件：真实使用场景里「接着上次干」比「新建」更常见 -->
      <div v-if="recent.length > 0" class="nt-welcome-recent">
        <div class="nt-welcome-recent-head">
          <span>{{ t('welcome.recent') }}</span>
          <button class="nt-text-btn nt-recent-more" @click="commands.run('file.openRecent')">
            {{ t('common.all') }}
          </button>
        </div>
        <button
          v-for="f in recent"
          :key="f.path"
          class="nt-row-item nt-welcome-recent-row"
          :title="f.path"
          @click="onOpenRecent(f.path)"
        >
          <NtIcon name="file" size="sm" class="nt-welcome-recent-icon" />
          <span class="nt-welcome-recent-name">{{ f.file_name }}</span>
          <span class="nt-welcome-recent-dir">{{ ellipsisMiddle(f.dir, 42) }}</span>
        </button>
      </div>
    </div>

    <!--
      每个标签一个独立实例，只切 visibility（v-show 而非 v-if）：
      v-if 会在切走时销毁视图，撤销历史与未保存的修改随之蒸发。
      首次激活才挂载：恢复十几个文件时不必把每篇正文都读进内存。
    -->
    <div v-else class="nt-editor-stack">
      <div
        v-for="tab in editor.tabs"
        v-show="tab.id === editor.activeId"
        :key="tab.id"
        class="nt-editor-slot"
        :style="slotStyle(tab)"
      >
        <template v-if="mounted.has(tab.id)">
          <div v-if="viewOfTab(tab) === 'hex'" class="nt-inline-view">
            <HexView
              :path="tab.path"
              :can-edit="isTextKind(kindOf(tab.path))"
              @edit="toEditor(tab)"
            />
          </div>
          <MarkdownEditor
            v-else-if="viewOfTab(tab) === 'markdown'"
            :path="tab.path"
            :tab="tab"
          />
          <DocPreview
            v-else-if="viewOfTab(tab) === 'doc'"
            :path="tab.path"
            :tab-id="tab.id"
            @toggle-edit="tab.showing_preview = false"
            @open-hex="toHex(tab)"
          />
          <CodeEditor v-else :tab="tab" :view-id="'main'" />
        </template>
        <div v-else class="nt-editor-loading">
          <span class="nt-spinner" />
        </div>
      </div>

      <!-- 分屏：只在活跃标签上渲染第二个只读视图 -->
      <template v-if="editor.active && editor.active.split && viewOfTab(editor.active) === 'code'">
        <div class="nt-split-handle" @mousedown="startSplitDrag" />
        <div class="nt-split-second">
          <CodeEditor :tab="editor.active" :view-id="'second'" :readonly="true" />
        </div>
      </template>

      <Minimap
        v-if="settings.config.editor.minimap && mainView"
        :view="mainView"
        :line-count="lineCount"
        :visible="true"
      />
    </div>

    <!-- 拖放打开的提示层 -->
    <div v-if="dragging" class="nt-drop-target">{{ t('drop.hint') }}</div>
  </div>
</template>

<style scoped>
.nt-editor-area {
  position: relative;
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  min-width: 0;
  background: var(--nt-editor-bg);
}

/*
  栈布局：编辑区 + 可选的分屏第二栏 + 小地图。
  用 flex 而不是 grid：分屏宽度比例与最小宽度在窄窗口里的行为更可预期。
*/
.nt-editor-stack {
  position: relative;
  display: flex;
  flex: 1;
  min-height: 0;
  min-width: 0;
}

/* 标签槽：占满整块，切走时只是不显示，实例留着 */
.nt-editor-slot {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.nt-editor-slot[style*='display: none'] {
  flex: 0;
}

.nt-editor-loading {
  display: flex;
  flex: 1;
  align-items: center;
  justify-content: center;
}

.nt-inline-view {
  flex: 1;
  min-height: 0;
  overflow: auto;
}

.nt-split-handle {
  width: 4px;
  flex: none;
  cursor: col-resize;
  background: var(--nt-border);
  transition: background-color var(--nt-dur-fast) var(--nt-ease);
}

.nt-split-handle:hover {
  background: var(--nt-primary);
}

.nt-split-second {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
}

.nt-welcome-hint {
  font-size: var(--nt-fs-md);
  color: var(--nt-ink-2);
}

.nt-welcome {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--nt-sp-5);
  height: 100%;
}

.nt-welcome-recent {
  width: min(520px, 80%);
  margin-top: var(--nt-sp-6);
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.nt-welcome-recent-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 var(--nt-sp-2) var(--nt-sp-1);
  font-size: var(--nt-fs-label);
  color: var(--nt-ink-muted);
  border-bottom: 1px solid var(--nt-border);
  margin-bottom: var(--nt-sp-1);
}

.nt-recent-more {
  height: auto;
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-3);
}

.nt-welcome-recent-row {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  width: 100%;
  padding: var(--nt-sp-1) var(--nt-sp-2);
  border-radius: var(--nt-radius-xs);
  text-align: left;
  font-size: var(--nt-fs-sm);
}

.nt-welcome-recent-icon {
  color: var(--nt-ink-muted);
}

.nt-welcome-recent-name {
  color: var(--nt-ink-2);
  flex: none;
  max-width: 45%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.nt-welcome-recent-dir {
  color: var(--nt-ink-muted);
  font-size: var(--nt-fs-hint);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}
</style>
