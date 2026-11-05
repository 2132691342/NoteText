// 文件树节点：递归渲染的一行，展开与右键操作由本组件自管。

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

    <!-- 子目录：懒加载，children 到位后才渲染。contextmenu 必须向上转发：
         菜单在面板层，不转发的话子节点右键就是死的。
         子项超过一批时增量渲染：一次铺上万条 DOM 是大目录卡顿的直接原因。 -->
    <template v-if="expanded && entry.is_dir">
      <FileTreeNode
        v-for="child in shownChildren"
        :key="child.path"
        :entry="child"
        :depth="depth + 1"
        @contextmenu="onChildMenu"
      />
      <div v-if="shownChildren.length < childrenList.length" ref="sentinel" class="nt-tree-more" />
    </template>
  </div>
</template>

<script lang="ts" setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useFileTreeStore } from '@/stores/fileTree'
import { useEditorStore } from '@/stores/editor'
import { useToastStore } from '@/stores/toast'
import { api, normalizeError } from '@/api'
import { useI18n } from 'vue-i18n'
import { dirName, joinPath } from '@/utils'
import type { DirEntry, FileOpVO } from '@/types/api'

const props = defineProps<{
  entry: DirEntry
  depth: number
  /** 挂载即展开：树的根节点用（子项由 open() 预先读好）。 */
  initiallyExpanded?: boolean
}>()

const { t } = useI18n()
const tree = useFileTreeStore()
const editor = useEditorStore()
const toast = useToastStore()

const emit = defineEmits<{
  (e: 'contextmenu', payload: { x: number; y: number; entry: DirEntry }): void
}>()

const expanded = ref(props.initiallyExpanded ?? false)
const loading = ref(false)
/** 行内新建 / 重命名的输入框状态。 */
const editing = ref<null | { mode: 'rename' | 'new-file' | 'new-dir'; draft: string }>(null)
const inputEl = ref<HTMLInputElement | null>(null)

// ---- 大目录增量渲染：哨兵元素进入视口就多渲染一批 ----
// 不把递归树拍平成虚拟列表，也不依赖面板的滚动容器（任意深度都一样工作）。
const RENDER_CHUNK = 200
const renderLimit = ref(RENDER_CHUNK)
const childrenList = computed(() => tree.children.get(props.entry.path) ?? [])
const shownChildren = computed(() => childrenList.value.slice(0, renderLimit.value))
const sentinel = ref<HTMLElement | null>(null)
let observer: IntersectionObserver | null = null

watch(sentinel, (el) => {
  if (!observer) return
  if (el) observer.observe(el)
  else observer.disconnect()
})

onMounted(() => {
  window.addEventListener('nt-tree-edit', onTreeEdit)
  window.addEventListener('nt-tree-expand', onTreeExpand)
  observer = new IntersectionObserver(
    (entries) => {
      if (entries.some((e) => e.isIntersecting)) renderLimit.value += RENDER_CHUNK
    },
    { rootMargin: '300px' },
  )
  window.addEventListener('nt-tree-reveal', onTreeReveal)
})
onBeforeUnmount(() => {
  window.removeEventListener('nt-tree-edit', onTreeEdit)
  window.removeEventListener('nt-tree-expand', onTreeExpand)
  observer?.disconnect()
  observer = null
  window.removeEventListener('nt-tree-reveal', onTreeReveal)
})

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

/**
 * 「在树里定位当前文件」：面板发出 reveal 事件，节点自己判断是否在路径上。
 *
 * 逐级展开再滚动到可视区——打开深层文件后侧栏还停在根目录，
 * 是「这个文件到底在哪」这类问题的唯一答案。
 */
async function onTreeReveal(e: Event) {
  const d = (e as CustomEvent).detail as { path?: string } | undefined
  const target = d?.path
  if (!target) return
  const norm = (v: string) => v.replaceAll(String.fromCharCode(92), '/').toLowerCase()
  const mine = norm(props.entry.path)
  const want = norm(target)
  if (!want.startsWith(mine + '/')) return
  if (props.entry.is_dir && !expanded.value) {
    await toggle()
  }
  // 滚动到可视区：等子节点渲染完再滚，否则位置还算不出来。
  void nextTick(() => {
    const row = document.querySelector<HTMLElement>(
      `.nt-tree-row[data-path="${CSS.escape(props.entry.path)}"]`,
    )
    if (want === mine) row?.scrollIntoView({ block: 'nearest' })
  })
}

/** 右键：交给父面板统一弹菜单（父层才有 ContextMenu 与完整菜单项）。 */
function onContextMenu(e: MouseEvent) {
  e.preventDefault()
  e.stopPropagation()
  emit('contextmenu', { x: e.clientX, y: e.clientY, entry: props.entry })
}

/** 子节点冒泡上来的右键意图：原样继续向面板转发。 */
function onChildMenu(payload: { x: number; y: number; entry: DirEntry }) {
  emit('contextmenu', payload)
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

/** 菜单里的「打开」对目录是展开：节点自己翻展开态（面板只发意图）。 */
function onTreeExpand(e: Event) {
  const d = (e as CustomEvent).detail as { path: string }
  if (d?.path !== props.entry.path) return
  if (props.entry.is_dir && !expanded.value) void toggle()
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
  // 只在受影响的目录上刷一次，不发多余的 readDir。
  const parent = props.entry.is_dir ? props.entry.path : dirName(props.entry.path)
  try {
    if (ed.mode === 'rename') {
      await renameTo(name)
    } else {
      const target = joinPath(parent, name)
      if (ed.mode === 'new-dir') await api.fileOps.createDir(target)
      else await api.fileOps.createFile(target)
      if (ed.mode === 'new-file') await editor.openPath(target)
    }
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
  await tree.refresh(parent)
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
  // 源目录与目标目录各刷一次：移动会同时改变两边的内容。
  const srcParent = dirName(src)
  try {
    const res = await api.fileOps.move([src], props.entry.path)
    report(res, () => {
      editor.renamePath(src, joinPath(props.entry.path, src.split(/[\\/]/).pop() ?? ''))
    })
  } catch (err) {
    toast.bad(normalizeError(err).message)
  }
  await Promise.all([tree.refresh(props.entry.path), tree.refresh(srcParent)])
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
</script>
