// 文件树面板：懒展开的目录树。
//
// 展开状态在 store 里（切标签不丢），子项缓存按目录。
// 外部变更由 `fs:changed` 事件驱动重读（见 composables/useExternalChange.ts）。
//
// 右键菜单覆盖资源管理器的常用项：新建（文件 / 文件夹）、重命名、删除、
// 剪切 / 复制 / 粘贴、创建副本、复制路径、在资源管理器显示。
// 树内拖拽把节点移进目标文件夹（高亮目标 + 禁止拖进自身子目录）。

<template>
  <div class="nt-tree" @contextmenu.prevent="onBlankMenu">
    <div v-if="!tree.root" class="nt-empty">
      <p>{{ t('sidebar.openFolderHint') }}</p>
      <ElButton size="small" @click="commands.run('file.openFolder')">
        {{ t('menu.openFolder') }}
      </ElButton>
    </div>
    <template v-else>
      <FileTreeNode
        v-for="e in tree.children.get(tree.root) ?? []"
        :key="e.path"
        :entry="e"
        :depth="0"
      />
    </template>
  </div>

  <!-- 右键菜单：与编辑器 / 标签栏共用 ContextMenu -->
  <ContextMenu
    v-if="menu"
    :x="menu.x"
    :y="menu.y"
    :items="treeMenuItems"
    @close="menu = null"
  />

  <!-- 树内拖拽：源节点与目标文件夹之间画一条反馈，落点由 drop 定 -->
  <div
    v-if="dropTarget"
    class="nt-tree-drop-hint"
    :style="{ top: `${dropTarget.y}px`, left: `${dropTarget.x}px` }"
  >
    {{ t('tree.dropHint', { name: dropTarget.name }) }}
  </div>
</template>

<script lang="ts" setup>
import { computed, h, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton } from 'element-plus'
import { type Component } from 'vue'
import { useFileTreeStore } from '@/stores/fileTree'
import { useEditorStore } from '@/stores/editor'
import { useCommandStore } from '@/stores/commands'
import { useToastStore } from '@/stores/toast'
import { api, normalizeError } from '@/api'
import { copyText, dirName, joinPath } from '@/utils'
import ContextMenu from '@/components/common/ContextMenu.vue'
import type { MenuItem } from '@/components/common/contextMenu'
import type { DirEntry } from '@/types/api'

const { t } = useI18n()
const tree = useFileTreeStore()
const editor = useEditorStore()
const commands = useCommandStore()
const toast = useToastStore()

const menu = ref<{ x: number; y: number } | null>(null)
const menuEntry = ref<DirEntry | null>(null)

/**
 * 树节点组件：递归渲染。
 *
 * 用 defineAsyncComponent 自引用会形成循环依赖，所以这里用一个工厂函数
 * 在 setup 里造一个本地组件——Vue 的 SFC 递归可以直接 <FileTreeNode>，
 * 但为了把「懒加载子目录」的逻辑收在一起，还是显式造一个。
 */
const FileTreeNode: Component = {
  name: 'FileTreeNode',
  props: { entry: { type: Object as () => DirEntry, required: true }, depth: { type: Number, default: 0 } },
  setup(props) {
    const expanded = ref(false)
    const loading = ref(false)
    /** 行内新建 / 重命名的输入框状态。 */
    const editing = ref<null | { mode: 'rename' | 'new-file' | 'new-dir'; draft: string }>(null)

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

    /** 右键：记住被点的节点，菜单项按它算。 */
    function onContextMenu(e: MouseEvent) {
      e.preventDefault()
      e.stopPropagation()
      menuEntry.value = props.entry
      menu.value = { x: e.clientX, y: e.clientY }
    }

    // ---- 菜单项触发的行内编辑 ----
    // 菜单是全局浮层，拿不到本节点的 setup 作用域，所以用事件把意图传下来。
    function onTreeEdit(e: Event) {
      const d = (e as CustomEvent).detail as { path: string; mode: 'rename' | 'new-file' | 'new-dir' }
      if (d.path !== props.entry.path) return
      startEdit(d.mode)
    }

    // ---- 行内编辑：新建 / 重命名 ----
    function startEdit(mode: 'rename' | 'new-file' | 'new-dir') {
      menu.value = null
      editing.value = { mode, draft: mode === 'rename' ? props.entry.name : '' }
      if (mode !== 'rename' && props.entry.is_dir && !expanded.value) void toggle()
    }

    function cancelEdit() {
      editing.value = null
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

    return {
      expanded,
      loading,
      toggle,
      editing,
      startEdit,
      cancelEdit,
      commitEdit,
      onContextMenu,
      onTreeEdit,
      onDragStart,
      onDragEnd,
      onDragOver,
      onDragLeave,
      onDrop,
      render: () =>
        h(
          'div',
          {
            class: [
              'nt-tree-row',
              {
                'is-on': editor.active?.path === props.entry.path,
                'is-cut': tree.clipboard?.cut && tree.clipboard.paths.includes(props.entry.path),
                'is-drop': tree.dropTarget === props.entry.path,
              },
            ],
            'data-path': props.entry.path,
            style: { paddingLeft: `calc(var(--nt-sp-2) + ${props.depth * 12}px)` },
            draggable: 'true',
            onClick: toggle,
            onContextmenu: onContextMenu,
            onDragstart: onDragStart,
            onDragend: onDragEnd,
            onDragover: onDragOver,
            onDragleave: onDragLeave,
            onDrop: onDrop,
          },
          [
            editing.value
              ? h('input', {
                  class: 'nt-tree-input',
                  value: editing.value.draft,
                  autofocus: true,
                  onClick: (ev: Event) => ev.stopPropagation(),
                  onKeydown: (ev: KeyboardEvent) => {
                    ev.stopPropagation()
                    if (ev.key === 'Enter') void commitEdit()
                    if (ev.key === 'Escape') cancelEdit()
                  },
                  onBlur: () => void commitEdit(),
                  onInput: (ev: Event) => {
                    if (editing.value) editing.value.draft = (ev.target as HTMLInputElement).value
                  },
                })
              : [
                  props.entry.is_dir
                    ? h('span', { class: ['nt-tree-twisty', { 'is-open': expanded.value }] }, '▸')
                    : h('span', { class: 'nt-tree-twisty' }),
                  h('span', { class: 'nt-tree-label' }, props.entry.name),
                  tree.dirtyPaths.has(props.entry.path)
                    ? h('span', { class: 'nt-tree-dirty' }, '●')
                    : null,
                ],
            expanded.value && props.entry.is_dir
              ? (tree.children.get(props.entry.path) ?? []).map((child) =>
                  h(FileTreeNode, { key: child.path, entry: child, depth: props.depth + 1 }),
                )
              : null,
          ],
        ),
    }
  },
}

/** 一组文件操作的结果统一报一次：失败项逐条说，成功项只说条数。 */
function report(res: { succeeded: number; failed: number; results: { ok: boolean; message: string }[] }, onOk?: () => void) {
  const bad = res.results.filter((r) => !r.ok)
  if (res.succeeded > 0) {
    toast.ok(t('toast.opDone', { n: res.succeeded }))
    onOk?.()
  }
  if (bad.length > 0) toast.bad(bad.map((r) => r.message).join('\n'))
}

function onBlankMenu(e: MouseEvent) {
  if (!tree.root) return
  menuEntry.value = null
  menu.value = { x: e.clientX, y: e.clientY }
}

/** 右键菜单条目：按被点中的节点算（空白处只给目录级动作）。 */
const treeMenuItems = computed<MenuItem[]>(() => {
  const entry = menuEntry.value
  const isDir = entry?.is_dir ?? false
  const targetDir = isDir ? entry!.path : entry ? dirName(entry.path) : tree.root
  return [
    {
      label: t('menu.open'),
      disabled: !entry,
      run: async () => {
        if (!entry) return
        if (entry.is_dir) await tree.refresh(entry.path)
        else await editor.openPath(entry.path)
      },
    },
    { sep: true },
    {
      label: t('tree.newFile'),
      disabled: !entry || !isDir,
      run: () => void startMenuEdit('new-file'),
    },
    {
      label: t('tree.newFolder'),
      disabled: !entry || !isDir,
      run: () => void startMenuEdit('new-dir'),
    },
    { sep: true },
    {
      label: t('menu.cut'),
      hint: 'Ctrl+X',
      disabled: !entry,
      run: () => {
        if (entry) tree.setClipboard([entry.path], true)
      },
    },
    {
      label: t('menu.copy'),
      hint: 'Ctrl+C',
      disabled: !entry,
      run: () => {
        if (entry) tree.setClipboard([entry.path], false)
      },
    },
    {
      label: t('tree.pasteTo', { name: entry ? entry.name : '' }),
      disabled: !tree.clipboard || !entry,
      run: () => void paste(targetDir),
    },
    {
      label: t('tree.duplicate'),
      disabled: !entry,
      run: () => void paste(targetDir, true),
    },
    {
      label: t('menu.rename'),
      hint: 'F2',
      disabled: !entry,
      run: () => void startMenuEdit('rename'),
    },
    { sep: true },
    {
      label: t('tab.copyPath'),
      disabled: !entry,
      run: async () => {
        if (!entry) return
        await copyText(entry.path)
        toast.ok(t('common.copied'))
      },
    },
    {
      label: t('tab.revealInExplorer'),
      disabled: !entry,
      run: () => {
        if (entry) void api.openPath(entry.is_dir ? entry.path : dirName(entry.path))
      },
    },
    { sep: true },
    {
      label: t('common.refresh'),
      run: () => {
        if (tree.root) void tree.refresh(tree.root)
      },
    },
    {
      label: t('tree.delete'),
      danger: true,
      disabled: !entry,
      run: () => void removeEntry(),
    },
  ]
})

/**
 * 菜单项触发的行内编辑：菜单是全局浮层，拿不到树节点的 setup 作用域，
 * 所以这里用事件把意图传下去，节点自己在监听里开输入框。
 */
function startMenuEdit(mode: 'rename' | 'new-file' | 'new-dir') {
  menu.value = null
  const path = menuEntry.value?.path
  if (!path) return
  window.dispatchEvent(new CustomEvent('nt-tree-edit', { detail: { path, mode } }))
}

/** 粘贴：剪切 = 移动，复制 = 复制，「创建副本」= 复制到同目录并加序号。 */
async function paste(destDir: string, autoSuffix = false) {
  const cb = tree.clipboard
  if (!cb || !destDir) return
  try {
    const res = cb.cut
      ? await api.fileOps.move(cb.paths, destDir, autoSuffix)
      : await api.fileOps.copy(cb.paths, destDir, autoSuffix)
    report(res, () => {
      if (cb.cut) for (const p of cb.paths) editor.renamePath(p, '')
      tree.setClipboard([], false)
    })
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
  if (tree.root) await tree.refresh(tree.root)
}

/** 删除：目录递归删，删前问一次。 */
async function removeEntry() {
  const entry = menuEntry.value
  if (!entry) return
  menu.value = null
  const ok = await api.showConfirm(
    t('tree.deleteConfirmTitle'),
    t(entry.is_dir ? 'tree.deleteFolderBody' : 'tree.deleteFileBody', { name: entry.name }),
  )
  if (!ok) return
  try {
    const res = await api.fileOps.remove([entry.path])
    report(res, () => {
      editor.renamePath(entry.path, '')
      tree.markDirty(entry.path, false)
    })
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
  if (tree.root) await tree.refresh(tree.root)
}

/** 拖拽时的落点提示：跟随目标目录行。 */
const dropTarget = computed(() => {
  const path = tree.dropTarget
  if (!path) return null
  const el = document.querySelector<HTMLElement>(`.nt-tree-row[data-path="${cssEscape(path)}"]`)
  if (!el) return null
  const r = el.getBoundingClientRect()
  return { x: r.right - 12, y: r.bottom + 4, name: baseName(path) }
})

function cssEscape(s: string): string {
  return s.replace(/["\\]/g, '\\$&')
}

function baseName(p: string): string {
  return p.split(/[\\/]/).pop() ?? p
}
</script>

<style scoped>
.nt-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--nt-sp-3);
  padding: var(--nt-sp-6) var(--nt-sp-3);
  text-align: center;
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-muted);
}

/* 拖拽落点：目标行整体给一层描边，用户看得见「会掉到哪」 */
.nt-tree-row.is-drop {
  background: var(--nt-primary-soft);
  box-shadow: inset 0 0 0 1px var(--nt-primary);
}

/* 剪切待粘贴态：半透明，与资源管理器一致 */
.nt-tree-row.is-cut {
  opacity: 0.45;
}

.nt-tree-input {
  flex: 1;
  min-width: 0;
  height: calc(var(--nt-ctl-h-sm) - 4px);
  padding: 0 var(--nt-sp-1);
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink);
  background: var(--nt-control-bg);
  border: 1px solid var(--nt-primary);
  border-radius: var(--nt-radius-xs);
}

.nt-tree-drop-hint {
  position: fixed;
  z-index: var(--nt-z-pop);
  padding: var(--nt-sp-1) var(--nt-sp-2);
  font-size: var(--nt-fs-hint);
  color: var(--nt-primary);
  background: var(--nt-elev-raised);
  border: 1px solid var(--nt-primary);
  border-radius: var(--nt-radius-xs);
  box-shadow: var(--nt-card-shadow);
  pointer-events: none;
}
</style>
