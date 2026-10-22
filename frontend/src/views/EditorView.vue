// 编辑器主视图：唯一的常驻视图。
//
// 装配顺序：布局（TitleBar → MenuBar → ToolBar → 主体 → StatusBar）
// → 会话恢复 → 首个标签。
<template>
  <div class="nt-root">
    <MenuBar v-if="ui.show_menu_bar" />
    <ToolBar v-if="ui.show_toolbar" />
    <div class="nt-body">
      <SideBar v-if="ui.show_sidebar" />
      <main class="nt-main">
        <TabBar />
        <EditorArea />
      </main>
    </div>
    <StatusBar v-if="ui.show_status_bar" />
    <!-- 工具箱与浮窗：统一挂在最外层，避免被编辑器滚动容器裁掉 -->
    <DialogHost />
  </div>
</template>

<script lang="ts" setup>
import { computed, onMounted } from 'vue'
import MenuBar from '@/components/layout/MenuBar.vue'
import ToolBar from '@/components/layout/ToolBar.vue'
import SideBar from '@/components/layout/SideBar.vue'
import StatusBar from '@/components/layout/StatusBar.vue'
import TabBar from '@/components/layout/TabBar.vue'
import EditorArea from '@/components/editor/EditorArea.vue'
import DialogHost from '@/components/dialogs/DialogHost.vue'
import { useSettingsStore } from '@/stores/settings'
import { useEditorStore } from '@/stores/editor'
import { useFileTreeStore } from '@/stores/fileTree'
import { registerCommands } from '@/composables/useCommands'
import { useViewportScale } from '@/composables/useViewportScale'
import { useGlobalKeys } from '@/composables/useGlobalKeys'
import { useExternalChange } from '@/composables/useExternalChange'
import { useShellIntent } from '@/composables/useShellIntent'
import { useQuitGuard } from '@/composables/useQuitGuard'
import { useFileDrop } from '@/composables/useFileDrop'
import { useAutoSave } from '@/composables/useAutoSave'

const settings = useSettingsStore()
const editor = useEditorStore()
const tree = useFileTreeStore()

const ui = computed(() => settings.config.ui)

useViewportScale()
useGlobalKeys()
registerCommands()
useExternalChange()
useShellIntent()
useQuitGuard()
useFileDrop()
useAutoSave()

onMounted(async () => {
  // 会话恢复：失败不阻塞启动（用户看到的是欢迎页）。
  if (settings.config.ui.restore_session) {
    try {
      const { api } = await import('@/api')
      const restored = await api.restoreSession()
      if (restored.session) {
        editor.splitRatio = restored.session.split_ratio
      }
      editor.restore(restored.tabs, restored.session?.active_index ?? 0)
      if (restored.missing.length > 0) {
        const { useToastStore } = await import('@/stores/toast')
        useToastStore().warn(
          `${restored.missing.length} 个上次打开的文件已不存在`,
        )
      }
    } catch {
      /* 恢复失败：欢迎页即可 */
    }
  }

  // 记住上次打开的文件夹：启动即进入用户熟悉的工作区。
  if (settings.config.file.remember_last_folder && settings.config.file.last_folder) {
    await tree.open(settings.config.file.last_folder)
  }

  if (editor.tabs.length === 0) {
    editor.newTab()
  }
})
</script>

<style scoped>
.nt-root {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--nt-bg);
}

.nt-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.nt-main {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
  min-height: 0;
}
</style>
