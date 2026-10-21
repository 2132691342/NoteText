// 文件树面板：懒展开的目录树。
//
// 展开状态在 store 里（切标签不丢），子项缓存按目录。
// 外部变更由 `fs:changed` 事件驱动重读（见 composables/useExternalChange.ts）。
<template>
  <div class="nt-tree" @contextmenu.prevent="onBlankMenu">
    <div v-if="!tree.root" class="nt-empty">
      <p>{{ t('sidebar.openFolderHint') }}</p>
      <ElButton size="small" @click="commands.run('file.openFolder')">
        {{ t('menu.openFolder') }}
      </ElButton>
    </div>
    <template v-else>
      <FileTreeNode v-for="e in tree.children.get(tree.root) ?? []" :key="e.path" :entry="e" :depth="0" />
    </template>
  </div>

  <Teleport to="body">
    <div v-if="menu" class="nt-ctx-menu" :style="{ left: `${menu.x}px`, top: `${menu.y}px` }" @click="menu = null">
      <button class="nt-ctx-item" @click="onOpen">{{ t('menu.open') }}</button>
      <button class="nt-ctx-item" @click="onReveal">{{ t('tab.revealInExplorer') }}</button>
      <div class="nt-menu-sep" />
      <button class="nt-ctx-item" @click="onCopy">{{ t('tab.copyPath') }}</button>
      <button class="nt-ctx-item" @click="onRefresh">{{ t('common.refresh') }}</button>
    </div>
  </Teleport>
</template>

<script lang="ts" setup>
import { h, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton } from 'element-plus'
import { type Component } from 'vue'
import { useFileTreeStore } from '@/stores/fileTree'
import { useEditorStore } from '@/stores/editor'
import { useCommandStore } from '@/stores/commands'
import { useToastStore } from '@/stores/toast'
import { api } from '@/api'
import { copyText, dirName } from '@/utils'
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
        await tree.refresh(props.entry.path)
        expanded.value = true
      } finally {
        loading.value = false
      }
    }

    return () =>
      h('div', {}, [
        h(
          'div',
          {
            class: ['nt-tree-row', { 'is-on': editor.active?.path === props.entry.path }],
            style: { paddingLeft: `${8 + props.depth * 14}px` },
            onClick: toggle,
            onContextmenu: (e: MouseEvent) => {
              e.preventDefault()
              menuEntry.value = props.entry
              menu.value = { x: e.clientX, y: e.clientY }
            },
          },
          [
            props.entry.is_dir
              ? h('span', { class: ['nt-tree-twisty', { 'is-open': expanded.value }] }, '▸')
              : h('span', { class: 'nt-tree-twisty' }),
            h('span', { class: 'nt-tree-label' }, props.entry.name),
            tree.dirtyPaths.has(props.entry.path)
              ? h('span', { class: 'nt-tree-dirty' }, '●')
              : null,
          ],
        ),
        expanded.value && props.entry.is_dir
          ? (tree.children.get(props.entry.path) ?? []).map((child) =>
              h(FileTreeNode, { key: child.path, entry: child, depth: props.depth + 1 }),
            )
          : null,
      ])
  },
}

function onBlankMenu(e: MouseEvent) {
  if (!tree.root) return
  menuEntry.value = null
  menu.value = { x: e.clientX, y: e.clientY }
}

async function onOpen() {
  const e = menuEntry.value
  menu.value = null
  if (!e) return
  if (e.is_dir) await tree.refresh(e.path)
  else await editor.openPath(e.path)
}

function onReveal() {
  const e = menuEntry.value
  menu.value = null
  if (!e) return
  void api.openPath(e.is_dir ? e.path : dirName(e.path))
}

async function onCopy() {
  const e = menuEntry.value
  menu.value = null
  if (!e) return
  await copyText(e.path)
  toast.ok(t('common.copied'))
}

function onRefresh() {
  menu.value = null
  if (tree.root) void tree.refresh(tree.root)
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
</style>
