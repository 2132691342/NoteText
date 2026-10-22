// 自绘标题栏：品牌 + 当前文件名 + 窗控。
//
// 无边框窗口（tauri.conf.json decorations=false）后，最小化 / 最大化 /
// 关闭都不再是系统装饰，全走这里。拖拽区用 data-tauri-drag-region 声明。
<template>
  <div class="nt-titlebar" data-tauri-drag-region>
    <div class="nt-titlebar-brand">
      <span class="nt-titlebar-logo">N</span>
      <span class="nt-titlebar-name">NoteText</span>
    </div>
    <div class="nt-titlebar-center" data-tauri-drag-region>
      {{ activeTitle }}
      <span v-if="editor.active?.dirty" class="nt-titlebar-dirty">●</span>
    </div>
    <div class="nt-titlebar-controls">
      <button class="nt-icon-btn" :title="t('toolbar.new')" @click="commands.run('file.new')">
        <NtIcon name="plus" />
      </button>
      <button
        class="nt-icon-btn"
        :title="t('menu.toggleSidebar')"
        :class="{ 'is-on': ui.show_sidebar }"
        @click="toggleSidebar"
      >
        <NtIcon name="panel-left" />
      </button>
      <span class="nt-titlebar-sep" />
      <button class="nt-icon-btn" :title="t('menu.settings')" @click="router.push('/settings')">
        <NtIcon name="settings" />
      </button>
      <span class="nt-titlebar-sep" />
      <button class="nt-icon-btn" :title="t('common.minimize')" @click="api.windowMinimize()">
        <NtIcon name="minus" />
      </button>
      <button class="nt-icon-btn" :title="t('common.maximize')" @click="api.windowToggleMaximize()">
        <NtIcon :name="maximized ? 'square' : 'square-dashed'" />
      </button>
      <button class="nt-icon-btn is-close" :title="t('common.close')" @click="api.windowClose()">
        <NtIcon name="x" />
      </button>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { api } from '@/api'
import { useEditorStore } from '@/stores/editor'
import { useSettingsStore } from '@/stores/settings'
import { useCommandStore } from '@/stores/commands'
import NtIcon from '@/components/common/NtIcon.vue'

const { t } = useI18n()
const router = useRouter()
const editor = useEditorStore()
const settings = useSettingsStore()
const commands = useCommandStore()

const ui = computed(() => settings.config.ui)
const maximized = ref(false)

const activeTitle = computed(() => {
  const tab = editor.active
  if (!tab) return 'NoteText'
  const name = tab.title || t('tab.untitled')
  return tab.dirty ? `${name} *` : name
})

function toggleSidebar() {
  settings.updateUI({ show_sidebar: !ui.value.show_sidebar })
}

onMounted(() => {
  void api.windowIsMaximized().then((v) => (maximized.value = v))
  // 窗口尺寸变化时最大化态可能变：监听 resize 比轮询省。
  window.addEventListener('resize', () => {
    void api.windowIsMaximized().then((v) => (maximized.value = v))
  })
})
</script>

<style scoped>
.nt-titlebar {
  display: flex;
  align-items: center;
  height: var(--nt-h-menu);
  padding: 0 var(--nt-sp-1) 0 var(--nt-sp-3);
  background: var(--nt-chrome);
  border-bottom: 1px solid var(--nt-border);
  flex: none;
  user-select: none;
}

.nt-titlebar-brand {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  flex: none;
}

.nt-titlebar-logo {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: var(--nt-sp-5);
  height: var(--nt-sp-5);
  border-radius: var(--nt-radius-xs);
  background: var(--nt-primary);
  color: var(--nt-primary-contrast);
  font-family: var(--font-display);
  font-size: var(--nt-fs-xs);
  font-weight: 700;
}

.nt-titlebar-name {
  font-size: var(--nt-fs-label);
  font-weight: 600;
  color: var(--nt-ink);
}

.nt-titlebar-center {
  flex: 1;
  min-width: 0;
  text-align: center;
  font-size: var(--nt-fs-label);
  color: var(--nt-ink-2);
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  padding: 0 var(--nt-sp-4);
}

.nt-titlebar-dirty {
  color: var(--nt-primary);
  font-size: var(--nt-fs-xs);
  vertical-align: middle;
  margin-left: 2px;
}

.nt-titlebar-controls {
  display: flex;
  align-items: center;
  gap: 2px;
  flex: none;
}

.nt-titlebar-sep {
  width: 1px;
  height: var(--nt-sp-4);
  background: var(--nt-border);
  margin: 0 2px;
}

.nt-icon-btn.is-close:hover {
  background: var(--nt-danger-bg);
  color: var(--nt-danger);
}
</style>
