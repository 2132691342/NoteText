// 编辑区：按 view_type 分发到不同容器。
//
// 一个标签一个编辑器实例（组件按 tab.id 缓存），切换标签不销毁——
// 那样会丢撤销历史。分屏时同一标签渲染两个视图，第二个只读同步。
<template>
  <div class="nt-editor-area">
    <div v-if="!editor.active" class="nt-welcome">
      <div class="nt-welcome-logo">N</div>
      <p class="nt-welcome-hint">打开一个文件，或新建一个开始编辑</p>
      <div class="nt-welcome-actions">
        <ElButton type="primary" @click="commands.run('file.new')">新建</ElButton>
        <ElButton @click="commands.run('file.open')">打开…</ElButton>
        <ElButton @click="commands.run('file.openFolder')">打开文件夹…</ElButton>
      </div>
    </div>

    <template v-else>
      <!-- 非代码视图：二进制走 Hex，文档走预览分流，都不进 CodeMirror -->
      <div v-if="editor.active.view_type === 'hex'" class="nt-inline-view">
        <HexView :path="editor.active.path" />
      </div>
      <DocPreview v-else-if="isDoc" :path="editor.active.path" />
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
    <div v-if="dragging" class="nt-drop-target">松手即用 NoteText 打开</div>
  </div>
</template>

<script lang="ts" setup>
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { ElButton } from 'element-plus'
import CodeEditor from '@/components/editor/CodeEditor.vue'
import Minimap from '@/components/editor/Minimap.vue'
import DocPreview from '@/components/editor/DocPreview.vue'
import HexView from '@/components/editor/HexView.vue'
import { useEditorStore } from '@/stores/editor'
import { useCommandStore } from '@/stores/commands'
import { useSettingsStore } from '@/stores/settings'
import { get } from '@/composables/useEditorRegistry'
import { getText } from '@/composables/useEditorRegistry'

const editor = useEditorStore()
const commands = useCommandStore()
const settings = useSettingsStore()

/** 文档预览的扩展名：这些格式有专门的预览器，不该进代码编辑器。 */
const DOC_EXTS = new Set([
  'xlsx', 'xlsm', 'csv', 'docx', 'pdf',
  'png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'ico',
])

const isDoc = computed(() => {
  const p = editor.active?.path
  if (!p) return false
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
</style>
