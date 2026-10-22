// 编辑区：按 view_type 分发到不同容器。
//
// 一个标签一个编辑器实例（组件按 tab.id 缓存），切换标签不销毁——
// 那样会丢撤销历史。分屏时同一标签渲染两个视图，第二个只读同步。
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

    <template v-else>
      <!-- 非代码视图：二进制走 Hex，文档走预览分流，都不进 CodeMirror -->
      <div v-if="editor.active.view_type === 'hex'" class="nt-inline-view">
        <HexView :path="editor.active.path" />
      </div>
      <DocPreview
        v-else-if="isDoc"
        :path="editor.active.path"
        @toggle-edit="editor.active.showing_preview = false"
      />
      <div v-else class="nt-split" :style="splitStyle">
        <CodeEditor :tab="editor.active" :view-id="'main'" />
        <Minimap
          v-if="settings.config.editor.minimap && mainView"
          :view="mainView"
          :line-count="lineCount"
          :visible="true"
        />
        <div
          v-if="editor.active.split"
          class="nt-split-handle"
          @mousedown="startSplitDrag"
        />
        <CodeEditor
          v-if="editor.active.split"
          :tab="editor.active"
          :view-id="'second'"
          :readonly="true"
        />
      </div>
    </template>

    <!-- 拖放打开的提示层 -->
    <div v-if="dragging" class="nt-drop-target">{{ t('drop.hint') }}</div>
  </div>
</template>

<script lang="ts" setup>
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton } from 'element-plus'
import CodeEditor from '@/components/editor/CodeEditor.vue'
import Minimap from '@/components/editor/Minimap.vue'
import DocPreview from '@/components/editor/DocPreview.vue'
import HexView from '@/components/editor/HexView.vue'
import NtIcon from '@/components/common/NtIcon.vue'
import { useEditorStore } from '@/stores/editor'
import { useCommandStore } from '@/stores/commands'
import { useSettingsStore } from '@/stores/settings'
import { get } from '@/composables/useEditorRegistry'
import { getText } from '@/composables/useEditorRegistry'
import { api } from '@/api'
import { ellipsisMiddle } from '@/utils'
import type { RecentFileVO } from '@/types/api'

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

/** 文档预览的扩展名：这些格式有专门的预览器，不该进代码编辑器。 */
const DOC_EXTS = new Set([
  'xlsx', 'xlsm', 'csv', 'docx', 'pdf',
  'png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'ico',
])

const isDoc = computed(() => {
  const p = editor.active?.path
  if (!p) return false
  // 手动切到预览时一律走预览器（用户自己按的按钮）；
  // 否则按扩展名判定，二进制文档不往 CodeMirror 里塞。
  if (editor.active?.showing_preview) return true
  const ext = p.split('.').pop()?.toLowerCase() ?? ''
  return DOC_EXTS.has(ext)
})

/** Minimap 要拿主视图的滚动与文档：注册表按 tabId 取。 */
const mainView = computed(() => {
  const id = editor.activeId
  return id != null ? (get(id) ?? null) : null
})

const lineCount = computed(() => {
  const id = editor.activeId
  if (id == null) return 0
  return getText(id).split('\n').length
})

const splitStyle = computed(() => {
  const r = editor.splitRatio
  return editor.active?.split
    ? { gridTemplateColumns: `${r * 100}% 4px ${(1 - r) * 100}%` }
    : { gridTemplateColumns: '1fr' }
})

// ---- 分屏比例拖拽 ----
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

// ---- 拖放打开 ----
// Tauri 的 dragDropEnabled=false（见 tauri.conf.json）：文件拖进来由
// 前端接管，否则 WebView 会把它当成导航直接跳走。
const dragging = ref(false)
let depth = 0

function onDragEnter(e: DragEvent) {
  if (!e.dataTransfer?.types.includes('Files')) return
  depth += 1
  dragging.value = true
}
function onDragOver(e: DragEvent) {
  if (!e.dataTransfer?.types.includes('Files')) return
  e.preventDefault()
}
function onDragLeave() {
  depth -= 1
  if (depth <= 0) {
    depth = 0
    dragging.value = false
  }
}
async function onDrop(e: DragEvent) {
  depth = 0
  dragging.value = false
  const files = e.dataTransfer?.files
  if (!files?.length) return
  e.preventDefault()
  // Tauri 2 的 webview 里 File.path 已废弃，用 webview 的 convertFileSrc 更稳；
  // 这里用 dataTransfer 里可拿到的 name + 用户选择的目录不可靠，所以走
  // `webviewWindow` 的 onDragDropEvent（见 composables/useFileDrop.ts）。
  void files
}

onMounted(() => {
  void loadRecent()
  window.addEventListener('dragenter', onDragEnter)
  window.addEventListener('dragover', onDragOver)
  window.addEventListener('dragleave', onDragLeave)
  window.addEventListener('drop', onDrop)
})
onUnmounted(() => {
  window.removeEventListener('dragenter', onDragEnter)
  window.removeEventListener('dragover', onDragOver)
  window.removeEventListener('dragleave', onDragLeave)
  window.removeEventListener('drop', onDrop)
})
</script>

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

.nt-split {
  display: grid;
  flex: 1;
  min-height: 0;
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
