// 文件树节点：递归渲染的一行。
//
// 之前这里是一个「setup 返回带 render 字段的对象」的内联组件，那种写法在 Vue 3
// 里不成立：只有 setup 返回**函数**才会被当成 render 函数，返回对象时它会变成
// setupState，于是 instance.render 落空，整棵树渲染成空占位符——打开文件夹
// 「没反应」的根因。详见 docs 的错误处理章节。拆成真正的 SFC 是唯一稳的写法。
//
// 递归：Vue SFC 可以按文件名自引用（<FileTreeNode>），不需要工厂闭包。
// 这段 Logic 原本内联在 FileTreePanel 里，抽出来时行为逐条保留：
// 懒展开、行内新建/重命名、右键委托、树内拖拽移动（禁止拖进自身子目录）。

<template>
  <div>
    <div
      class="nt-tree-row"
      :class="{
        'is-on': editor.active?.path === entry.path,
        'is-cut': tree.clipboard?.cut && tree.clipboard.paths.includes(entry.path),
        'is-drop': tree.dropTarget === entry.path,
      }"
      :data-path="entry.path"
      :style="{ paddingLeft: `calc(var(--nt-sp-2) + ${depth * 12}px)` }"
      draggable="true"
      @click="toggle"
      @contextmenu="onContextMenu"
      @dragstart="onDragStart"
      @dragend="onDragEnd"
      @dragover="onDragOver"
      @dragleave="onDragLeave"
      @drop="onDrop"
    >
      <template v-if="editing">
        <input
          ref="inputEl"
          v-model="editing.draft"
          class="nt-tree-input"
          @click.stop
          @keydown.stop="onEditKey"
          @blur="commitEdit"
        />
      </template>
      <template v-else>
        <span v-if="entry.is_dir" class="nt-tree-twisty" :class="{ 'is-open': expanded }">▸</span>
        <span v-else class="nt-tree-twisty" />
        <span class="nt-tree-label">{{ entry.name }}</span>
        <span v-if="tree.dirtyPaths.has(entry.path)" class="nt-tree-dirty">●</span>
      </template>
    </div>

    <!-- 子目录：懒加载，children 到位后才渲染 -->
    <template v-if="expanded && entry.is_dir">
      <FileTreeNode
        v-for="child in tree.children.get(entry.path) ?? []"
        :key="child.path"
        :entry="child"
        :depth="depth + 1"
      />
    </template>
  </div>
</template>

<script lang="ts" setup>
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useFileTreeStore } from '@/stores/fileTree'
import { useEditorStore } from '@/stores/editor'
import { useToastStore } from '@/stores/toast'
import { api, normalizeError } from '@/api'
import { useI18n } from 'vue-i18n'
import { dirName, joinPath } from '@/utils'
import type { FileOpVO } from '@/types/api'

const props = defineProps<{
  entry: { path: string; name: string; is_dir: boolean }
  depth?: number
}>()

const { t } = useI18n()
const tree = useFileTreeStore()
const editor = useEditorStore()
const toast = useToastStore()

const emit = defineEmits<{
  (e: 'contextmenu', payload: { x: number; y: number; entry: unknown }): void
}>()

const expanded = ref(false)
const loading = ref(false)
/** 行内新建 / 重命名的输入框状态。 */
const editing = ref<null | { mode: 'rename' | 'new-file' | 'new-dir'; draft: string }>(null)
const inputEl = ref<HTMLInputElement | null>(null)

onMounted(() => window.addEventListener('nt-tree-edit', onTreeEdit))
onBeforeUnmount(() => window.removeEventListener('nt-tree-edit', onTreeEdit))

async function toggle() {
  if (!props.entry.is_dir) {
    await editor.openPath(props.entry.path)
    return
  }
  if (expanded.value) {
    expanded.value = false
    return
  }
  loading.value = true
  try {
    await tree.toggle(props.entry.path)
    expanded.value = true
  } finally {
    loading.value = false
  }
}

/** 右键：交给父面板统一弹菜单（父层才有 ContextMenu 与完整菜单项）。 */
function onContextMenu(e: MouseEvent) {
  e.preventDefault()
  e.stopPropagation()
  emit('contextmenu', { x: e.clientX, y: e.clientY, entry: props.entry })
}

// ---- 菜单项触发的行内编辑 ----
// 菜单是全局浮层，够不到本节点的作用域，所以用事件把意图传下来。
function onTreeEdit(e: Event) {
  const d = (e as CustomEvent).detail as {
    path: string
    mode: 'rename' | 'new-file' | 'new-dir'
  }
  if (d.path !== props.entry.path) return
  startEdit(d.mode)
}

function startEdit(mode: 'rename' | 'new-file' | 'new-dir') {
  editing.value = { mode, draft: mode === 'rename' ? props.entry.name : '' }
  if (mode !== 'rename' && props.entry.is_dir && !expanded.value) void toggle()
  // 输入框是本帧才渲染出来的，focus 要等它进 DOM。
  void nextTick(() => inputEl.value?.focus())
}

function cancelEdit() {
  editing.value = null
}

function onEditKey(e: KeyboardEvent) {
  if (e.key === 'Enter') void commitEdit()
  if (e.key === 'Escape') cancelEdit()
}

async function commitEdit() {
  const ed = editing.value
  if (!ed) return
  const name = ed.draft.trim()
  editing.value = null
  if (!name) return
  if (/[\\/:*?"<>|]/.test(name)) {
    toast.bad(t('toast.badFileName'))
    return
  }
  try {
    if (ed.mode === 'rename') {
      await renameTo(name)
    } else {
      const parent = props.entry.is_dir ? props.entry.path : dirName(props.entry.path)
      const target = joinPath(parent, name)
      if (ed.mode === 'new-dir') await api.fileOps.createDir(target)
      else await api.fileOps.createFile(target)
      await tree.refresh(tree.root)
      if (ed.mode === 'new-file') await editor.openPath(target)
    }
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
  if (tree.root) await tree.refresh(tree.root)
}

/** 重命名：同目录下落新名字，同步已打开标签的路径。 */
async function renameTo(name: string) {
  const target = joinPath(dirName(props.entry.path), name)
  if (target === props.entry.path) return
  const res = await api.fileOps.rename(props.entry.path, target)
  report(res, () => {
    editor.renamePath(props.entry.path, target)
    tree.markDirty(props.entry.path, false)
  })
}

// ---- 拖拽移动：源是本节点，目标是目录 ----
function onDragStart(e: DragEvent) {
  tree.setDragPath(props.entry.path)
  e.dataTransfer?.setData('text/plain', props.entry.path)
  if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move'
}

function onDragEnd() {
  tree.setDragPath(null)
  tree.setDropTarget(null)
}

function canDropOn(): boolean {
  const src = tree.dragPath
  if (!src || !props.entry.is_dir) return false
  if (src === props.entry.path) return false
  // 不能拖进自己的子目录：目录树会成环，文件树读到尽头。
  const sep = props.entry.path.includes('\\') ? '\\' : '/'
  return !props.entry.path.startsWith(src + sep)
}

function onDragOver(e: DragEvent) {
  if (!canDropOn()) return
  e.preventDefault()
  if (e.dataTransfer) e.dataTransfer.dropEffect = 'move'
  tree.setDropTarget(props.entry.path)
}

function onDragLeave() {
  if (tree.dropTarget === props.entry.path) tree.setDropTarget(null)
}

async function onDrop(e: DragEvent) {
  if (!canDropOn()) return
  e.preventDefault()
  e.stopPropagation()
  const src = tree.dragPath
  tree.setDragPath(null)
  tree.setDropTarget(null)
  if (!src) return
  try {
    const res = await api.fileOps.move([src], props.entry.path)
    report(res, () => {
      editor.renamePath(src, joinPath(props.entry.path, src.split(/[\\/]/).pop() ?? ''))
    })
  } catch (err) {
    toast.bad(normalizeError(err).message)
  }
  if (tree.root) await tree.refresh(tree.root)
}

/** 一组文件操作的结果统一报一次：失败项逐条说，成功项只说条数。 */
function report(res: FileOpVO, onOk?: () => void) {
  const bad = res.results.filter((r) => !r.ok)
  if (res.succeeded > 0) {
    toast.ok(t('toast.opDone', { n: res.succeeded }))
    onOk?.()
  }
  if (bad.length > 0) toast.bad(bad.map((r) => r.message).join('\n'))
}

// 外部（FS 事件 / 拖拽移动）重读后本节点可能被卸载；这里只在还活着时兜住 loading 态。
watch(loading, (v) => {
  if (!v) return
})
</script>
