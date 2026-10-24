// 设置视图：左侧分组 + 右侧表单。
// 每项改动即时生效 + 防抖回写。用路由 /settings/:tab 而不是弹窗：
<template>
  <div class="nt-settings">
    <!-- 标题与窗控已在根标题栏：这里只放页面级动作，不再重复一遍标题 -->
    <header class="nt-settings-head">
      <span class="nt-settings-hint">{{ t('settings.resetHint') }}</span>
      <ElButton size="small" @click="onReset">{{ t('settings.reset') }}</ElButton>
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
        <SystemPanel v-else-if="current === 'system'" />
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
import { ElButton, ElMessage } from 'element-plus'
import { useSettingsStore } from '@/stores/settings'
import AppearancePanel from '@/components/settings/AppearancePanel.vue'
import EditorPanel from '@/components/settings/EditorPanel.vue'
import FilePanel from '@/components/settings/FilePanel.vue'
import SystemPanel from '@/components/settings/SystemPanel.vue'
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
  { id: 'system', labelKey: 'settings.system' },
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
  flex: none;
}

.nt-settings-hint {
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-muted);
}

.nt-settings-head-actions {
  display: flex;
  gap: var(--nt-sp-2);
}

/* 返回按钮：右上角 X 是唯一的退出口，给它一个可读的悬停反馈 */
.nt-settings-head-actions :deep(.el-button) {
  color: var(--nt-ink-2);
}

.nt-settings-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

/* 左栏宽度按最长标签定：中英切换后 "Files & encoding" / "快捷键" 都要能整行显示，
   固定 160px 在英文下会折行甚至裁掉半个字。 */
.nt-settings-nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
  width: 168px;
  padding: var(--nt-sp-3) var(--nt-sp-2);
  border-right: 1px solid var(--nt-border);
  flex: none;
  overflow-y: auto;
}

.nt-settings-nav-item {
  height: var(--nt-ctl-h);
  padding: 0 var(--nt-sp-2);
  border-radius: var(--nt-radius-sm);
  font-size: var(--nt-fs-sm);
  text-align: left;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  color: var(--nt-ink-2);
}

.nt-settings-panel {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  padding: var(--nt-sp-5) var(--nt-sp-6);
}
</style>
