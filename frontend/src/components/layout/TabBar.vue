// 标签栏：横向滚动的标签 + 脏标记 + 关闭钮。
//
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
        <NtIcon name="x" />
      </button>
    </div>
    <div class="nt-tabbar-spacer" />
    <button class="nt-icon-btn nt-tabbar-new" :title="t('toolbar.new')" @click="commands.run('file.new')">
      <NtIcon name="plus" />
    </button>
  </div>

  <!-- 标签右键菜单 -->
  <Teleport to="body">
    <div
      v-if="menu"
      class="nt-ctx-menu"
      :style="{ left: `${menu.x}px`, top: `${menu.y}px` }"
      @click="menu = null"
    >
      <button class="nt-ctx-item" @click="commands.run('file.closeTab')">{{ t('tab.close') }}</button>
      <button class="nt-ctx-item" @click="onCloseOthers">{{ t('tab.closeOthers') }}</button>
      <button class="nt-ctx-item" @click="onCloseSaved">{{ t('tab.closeSaved') }}</button>
      <button class="nt-ctx-item" @click="commands.run('file.closeAll')">{{ t('tab.closeAll') }}</button>
      <div class="nt-menu-sep" />
      <button class="nt-ctx-item" @click="onCopyPath">{{ t('tab.copyPath') }}</button>
      <button
        v-if="menuTab?.path"
        class="nt-ctx-item"
        @click="api.openPath(dirOf(menuTab.path))"
      >
        {{ t('tab.revealInExplorer') }}
      </button>
    </div>
  </Teleport>
</template>

<script lang="ts" setup>
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { api } from '@/api'
import { copyText, dirName } from '@/utils'
import NtIcon from '@/components/common/NtIcon.vue'
import { useEditorStore, type Tab } from '@/stores/editor'
import { useCommandStore } from '@/stores/commands'
import { useToastStore } from '@/stores/toast'

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

function onClose(tab: Tab) {
  if (!editor.close(tab.id)) {
    // 脏标签：交给命令层弹确认（那里统一处理「保存 / 不保存 / 取消」）。
    commands.run('file.closeTab')
  }
}

function onCloseOthers() {
  if (menuTab.value) editor.closeOthers(menuTab.value.id)
  menu.value = null
}

function onCloseSaved() {
  editor.closeSaved()
  menu.value = null
}

async function onCopyPath() {
  if (!menuTab.value?.path) return
  await copyText(menuTab.value.path)
  toast.ok(t('common.copied'))
  menu.value = null
}

function onMenu(e: MouseEvent, tab: Tab) {
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
}

.nt-ctx-item:hover {
  background: var(--nt-surface-selected);
  color: var(--nt-primary);
}

.nt-menu-sep {
  height: 1px;
  margin: var(--nt-sp-1) var(--nt-sp-2);
  background: var(--nt-border);
}
</style>
