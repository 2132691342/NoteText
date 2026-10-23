// 标签栏：横向滚动的标签 + 脏标记 + 关闭钮。
// 中键关闭、右键菜单（关闭其他 / 关闭全部 / 复制路径）都在这。
<template>
  <div class="nt-tabbar" @wheel="onWheel">
    <div
      v-for="tab in editor.tabs"
      :key="tab.id"
      class="nt-tab"
      :class="{ 'is-on': tab.id === editor.activeId }"
      @click="editor.activate(tab.id)"
      @click.middle="onClose(tab)"
      @contextmenu.prevent="onMenu($event, tab)"
    >
      <span class="nt-dot" :class="{ 'is-clean': !tab.dirty }" />
      <span class="nt-tab-label" :title="tab.path || tab.title">{{ displayName(tab) }}</span>
      <button class="nt-tab-close" @click.stop="onClose(tab)">
        <NtIcon name="x" size="sm" />
      </button>
    </div>
    <div class="nt-tabbar-spacer" />
    <button class="nt-icon-btn nt-tabbar-new" :title="t('toolbar.new')" @click="commands.run('file.new')">
      <NtIcon name="plus" />
    </button>
  </div>

  <!-- 标签右键菜单：与编辑器 / 文件树共用 ContextMenu -->
  <ContextMenu
    v-if="menu"
    :x="menu.x"
    :y="menu.y"
    :items="tabMenuItems"
    @close="menu = null"
  />
</template>

<script lang="ts" setup>
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { api } from '@/api'
import { copyText, dirName } from '@/utils'
import NtIcon from '@/components/common/NtIcon.vue'
import { useEditorStore, type Tab } from '@/stores/editor'
import { useCommandStore } from '@/stores/commands'
import { useToastStore } from '@/stores/toast'
import { closeTabById } from '@/composables/useCloseTab'
import ContextMenu from '@/components/common/ContextMenu.vue'
import type { MenuItem } from '@/components/common/contextMenu'

const { t } = useI18n()
const editor = useEditorStore()
const commands = useCommandStore()
const toast = useToastStore()

const menu = ref<{ x: number; y: number } | null>(null)
const menuTab = ref<Tab | null>(null)

function displayName(tab: Tab): string {
  return tab.title || t('tab.untitled')
}

function dirOf(path: string) {
  return dirName(path)
}

/** 横向滚轮转成标签滚动：触控板横向滑动已是原生行为，竖滚轮要手动转。 */
function onWheel(e: WheelEvent) {
  if (e.deltaY === 0) return
  const el = e.currentTarget as HTMLElement
  el.scrollLeft += e.deltaY
  e.preventDefault()
}

/** 关闭一个标签：干净的直接关，脏的走三选一确认。 */
function onClose(tab: Tab) {
  if (!editor.close(tab.id)) void closeTabById(tab.id)
}

/** 右键菜单条目：每次打开按被点中的标签算，不依赖「当前活跃」。 */
const tabMenuItems = computed<MenuItem[]>(() => {
  const tab = menuTab.value
  const path = tab?.path ?? ''
  return [
    { label: t('tab.close'), hint: 'Ctrl+W', run: () => tab && onClose(tab) },
    { label: t('tab.closeOthers'), run: () => tab && editor.closeOthers(tab.id) },
    { label: t('tab.closeLeft'), run: () => tab && editor.closeLeft(tab.id) },
    { label: t('tab.closeRight'), run: () => tab && editor.closeRight(tab.id) },
    { label: t('tab.closeSaved'), run: () => editor.closeSaved() },
    { label: t('tab.closeAll'), run: () => commands.run('file.closeAll') },
    { sep: true },
    {
      label: t('tab.copyPath'),
      disabled: !path,
      run: async () => {
        if (!path) return
        await copyText(path)
        toast.ok(t('common.copied'))
      },
    },
    {
      label: t('tab.revealInExplorer'),
      disabled: !path,
      run: () => {
        if (path) void api.openPath(dirOf(path))
      },
    },
    { sep: true },
    {
      label: t('tab.copyName'),
      run: async () => {
        if (!tab) return
        await copyText(displayName(tab))
        toast.ok(t('common.copied'))
      },
    },
  ]
})

function onMenu(e: MouseEvent, tab: Tab) {
  e.preventDefault()
  menuTab.value = tab
  menu.value = { x: e.clientX, y: e.clientY }
}
</script>

<style scoped>
.nt-tabbar-spacer {
  flex: 1;
  min-width: var(--nt-sp-2);
}

.nt-tabbar-new {
  margin-right: var(--nt-sp-1);
  flex: none;
}
</style>

<style>
/* 右键菜单要能越过编辑器容器，所以放全局 */
.nt-ctx-menu {
  position: fixed;
  z-index: var(--nt-z-pop);
  min-width: 180px;
  padding: var(--nt-sp-1);
  background: var(--nt-surface);
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius);
  box-shadow: var(--nt-card-shadow);
  /* 从触发位置弹出来：缩放锚点在左上，符合「从那一行长出来」的直觉 */
  transform-origin: top left;
  animation: nt-menu-in var(--nt-dur-fast) var(--nt-ease);
}

@keyframes nt-menu-in {
  from {
    opacity: 0;
    transform: scale(0.97);
  }
}

.nt-ctx-item {
  display: block;
  width: 100%;
  height: calc(var(--nt-ctl-h) - 4px);
  padding: 0 var(--nt-sp-2);
  border-radius: var(--nt-radius-xs);
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink);
  text-align: left;
  transition:
    background-color var(--nt-dur-fast) var(--nt-ease),
    color var(--nt-dur-fast) var(--nt-ease);
}

.nt-ctx-item:hover {
  background: var(--nt-surface-selected);
  color: var(--nt-primary);
}

/* 菜单项是整行文字：按下压深一档，不缩放 */
.nt-ctx-item:active {
  background: var(--nt-primary-soft);
  color: var(--nt-primary-strong);
}

.nt-menu-sep {
  height: 1px;
  margin: var(--nt-sp-1) var(--nt-sp-2);
  background: var(--nt-border);
}
</style>
