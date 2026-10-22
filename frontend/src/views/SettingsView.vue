// 设置视图：左侧分组 + 右侧表单。
//
// 每项改动即时生效 + 防抖回写。用路由 /settings/:tab 而不是弹窗：
// 刷新与前进后退都能停在原处。
<template>
  <div class="nt-settings">
    <header class="nt-settings-head">
      <h1>{{ t('settings.title') }}</h1>
      <div class="nt-settings-head-actions">
        <ElButton size="small" @click="onReset">{{ t('settings.reset') }}</ElButton>
        <ElButton size="small" :icon="Close" circle @click="router.back()" />
      </div>
    </header>
    <div class="nt-settings-body">
      <nav class="nt-settings-nav">
        <button
          v-for="tab in TABS"
          :key="tab.id"
          class="nt-row-item nt-settings-nav-item"
          :class="{ 'is-on': current === tab.id }"
          @click="router.replace(`/settings/${tab.id}`)"
        >
          {{ t(tab.labelKey) }}
        </button>
      </nav>
      <div class="nt-settings-panel">
        <AppearancePanel v-if="current === 'appearance'" />
        <EditorPanel v-else-if="current === 'editor'" />
        <FilePanel v-else-if="current === 'file'" />
        <KeybindingsPanel v-else-if="current === 'keybindings'" />
        <AboutPanel v-else-if="current === 'about'" />
        <GeneralPanel v-else />
      </div>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Close } from '@element-plus/icons-vue'
import { ElButton, ElMessage } from 'element-plus'
import { useSettingsStore } from '@/stores/settings'
import AppearancePanel from '@/components/settings/AppearancePanel.vue'
import EditorPanel from '@/components/settings/EditorPanel.vue'
import FilePanel from '@/components/settings/FilePanel.vue'
import KeybindingsPanel from '@/components/settings/KeybindingsPanel.vue'
import AboutPanel from '@/components/settings/AboutPanel.vue'
import GeneralPanel from '@/components/settings/GeneralPanel.vue'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const settings = useSettingsStore()

const TABS = [
  { id: 'general', labelKey: 'settings.general' },
  { id: 'appearance', labelKey: 'settings.appearance' },
  { id: 'editor', labelKey: 'settings.editor' },
  { id: 'file', labelKey: 'settings.file' },
  { id: 'keybindings', labelKey: 'settings.keybindings' },
  { id: 'about', labelKey: 'settings.about' },
] as const

const current = computed(() => (route.params.tab as string) || 'general')

async function onReset() {
  await settings.reset()
  ElMessage.success(t('settings.resetDone'))
}
</script>

<style scoped>
.nt-settings {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--nt-bg);
}

.nt-settings-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: var(--nt-h-menu);
  padding: 0 var(--nt-sp-3);
  border-bottom: 1px solid var(--nt-border);
  -webkit-app-region: drag;
  flex: none;
}

.nt-settings-head h1 {
  font-size: var(--nt-fs-md);
  font-weight: 600;
}

.nt-settings-head-actions {
  display: flex;
  gap: var(--nt-sp-2);
  -webkit-app-region: no-drag;
}

.nt-settings-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.nt-settings-nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
  width: 160px;
  padding: var(--nt-sp-3) var(--nt-sp-2);
  border-right: 1px solid var(--nt-border);
  flex: none;
  overflow-y: auto;
}

.nt-settings-nav-item {
  height: var(--nt-ctl-h);
  font-size: var(--nt-fs-sm);
  text-align: left;
  color: var(--nt-ink-2);
}

.nt-settings-panel {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  padding: var(--nt-sp-5) var(--nt-sp-6);
}
</style>
