// 侧栏：五个面板的容器（文件树 / 书签 / 片段 / 函数 / 监控）。
// 宽度可拖拽：上限与下限在 CSS 变量里，拖拽只改内联宽度。
<template>
  <div class="nt-sidebar" :style="{ width: `${ui.sidebar_width}px` }">
    <div class="nt-sidebar-header">
      <span>{{ t(`sidebar.${tab}`) }}</span>
      <div class="nt-sidebar-header-actions">
        <button
          v-if="tab === 'files'"
          class="nt-icon-btn"
          :title="t('menu.openFolder')"
          @click="commands.run('file.openFolder')"
        >
          <NtIcon name="folder-open" />
        </button>
        <button class="nt-icon-btn" @click="settings.updateUI({ show_sidebar: false })">
          <NtIcon name="panel-left" />
        </button>
      </div>
    </div>

    <nav class="nt-sidebar-tabs">
      <button
        v-for="id in TABS"
        :key="id"
        class="nt-sidebar-tab"
        :class="{ 'is-on': tab === id }"
        :title="t(`sidebar.${id}`)"
        @click="switchTab(id)"
      >
        <NtIcon :name="iconOf(id)" />
      </button>
    </nav>

    <div class="nt-sidebar-body">
      <FileTreePanel v-if="tab === 'files'" />
      <BookmarkPanel v-else-if="tab === 'bookmarks'" />
      <SnippetPanel v-else-if="tab === 'snippets'" />
      <FunctionPanel v-else-if="tab === 'functions'" />
      <FindResultsPanel v-else-if="tab === 'findResults'" />
      <MonitorPanel v-else-if="tab === 'monitor'" />
    </div>

    <div
      class="nt-sidebar-resize"
      @mousedown="startResize"
      @dblclick="resetWidth"
    />
  </div>
</template>

<script lang="ts" setup>
import { computed, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import NtIcon from '@/components/common/NtIcon.vue'
import FileTreePanel from '@/components/panels/FileTreePanel.vue'
import BookmarkPanel from '@/components/panels/BookmarkPanel.vue'
import SnippetPanel from '@/components/panels/SnippetPanel.vue'
import FunctionPanel from '@/components/panels/FunctionPanel.vue'
import FindResultsPanel from '@/components/panels/FindResultsPanel.vue'
import MonitorPanel from '@/components/panels/MonitorPanel.vue'
import { useSettingsStore } from '@/stores/settings'
import { useCommandStore } from '@/stores/commands'

const { t } = useI18n()
const settings = useSettingsStore()
const commands = useCommandStore()

const TABS = ['files', 'bookmarks', 'snippets', 'functions', 'findResults', 'monitor'] as const
type TabId = (typeof TABS)[number]

/** 当前页签：跟随会话里存的 sidebar_tab，启动即回到上次的位置。 */
const tab = computed<TabId>(() =>
  (TABS as readonly string[]).includes(ui.value.sidebar_tab)
    ? (ui.value.sidebar_tab as TabId)
    : 'files',
)

const ui = computed(() => settings.config.ui)

function switchTab(id: TabId) {
  settings.updateUI({ sidebar_tab: id })
}

function iconOf(id: TabId): string {
  switch (id) {
    case 'files':
      return 'folder'
    case 'bookmarks':
      return 'bookmark'
    case 'snippets':
      return 'list'
    case 'functions':
      return 'braces'
    default:
      return 'activity'
  }
}

// ---- 宽度拖拽 ----
let dragging = false
function startResize(e: MouseEvent) {
  dragging = true
  const startX = e.clientX
  const startW = ui.value.sidebar_width
  const min = 180
  const max = 480

  const onMove = (ev: MouseEvent) => {
    if (!dragging) return
    const next = Math.min(max, Math.max(min, startW + ev.clientX - startX))
    settings.updateUI({ sidebar_width: next })
  }
  const onUp = () => {
    dragging = false
    document.removeEventListener('mousemove', onMove)
    document.removeEventListener('mouseup', onUp)
    document.body.style.cursor = ''
  }
  document.body.style.cursor = 'col-resize'
  document.addEventListener('mousemove', onMove)
  document.addEventListener('mouseup', onUp)
}

/** 双击复位到默认宽度。 */
function resetWidth() {
  settings.updateUI({ sidebar_width: 260 })
}

onUnmounted(() => {
  dragging = false
})
</script>

<style scoped>
.nt-sidebar {
  position: relative;
  display: flex;
  flex-direction: column;
  background: var(--nt-chrome);
  border-right: 1px solid var(--nt-border);
  flex: none;
  min-width: 0;
}

.nt-sidebar-header-actions {
  display: flex;
  gap: 2px;
}

.nt-sidebar-tabs {
  display: flex;
  gap: 2px;
  padding: var(--nt-sp-1);
  border-bottom: 1px solid var(--nt-border);
  flex: none;
  /* 6 个页签在 180px 宽的侧栏里放不下：允许横向滚动，
     而不是换行把侧栏头挤成两行。滚动条隐藏，靠滚轮 / 拖拽。 */
  overflow-x: auto;
  scrollbar-width: none;
}

.nt-sidebar-tabs::-webkit-scrollbar {
  display: none;
}

.nt-sidebar-tab {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  min-width: var(--nt-ctl-h-sm);
  height: var(--nt-ctl-h-sm);
  border-radius: var(--nt-radius-xs);
  color: var(--nt-ink-2);
  font-size: var(--nt-fs-sm);
  transition:
    background-color var(--nt-dur-fast) var(--nt-ease),
    color var(--nt-dur-fast) var(--nt-ease),
    transform var(--nt-dur-fast) var(--nt-ease),
    box-shadow var(--nt-dur-fast) var(--nt-ease);
}

.nt-sidebar-tab:hover:not(.is-on) {
  background: var(--nt-surface-hover);
  color: var(--nt-ink);
  transform: scale(1.06);
}

.nt-sidebar-tab:active {
  transform: scale(0.92);
}

.nt-sidebar-tab.is-on {
  background: var(--nt-surface);
  color: var(--nt-primary);
  box-shadow: var(--nt-card-shadow);
}

.nt-sidebar-body {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

.nt-sidebar-resize {
  position: absolute;
  top: 0;
  right: -2px;
  width: var(--nt-sp-1);
  height: 100%;
  cursor: col-resize;
  z-index: 1;
  transition: background-color var(--nt-dur-fast) var(--nt-ease);
}

.nt-sidebar-resize:hover {
  background: var(--nt-primary);
}
</style>
