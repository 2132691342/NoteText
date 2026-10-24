// 编辑器主视图：唯一的常驻视图。
// 装配顺序：布局（TitleBar → MenuBar → ToolBar → 主体 → StatusBar）
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
    <AskTextDialog />
    <AskPickDialog />
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
import AskTextDialog from '@/components/dialogs/AskTextDialog.vue'
import AskPickDialog from '@/components/dialogs/AskPickDialog.vue'
import { useSettingsStore } from '@/stores/settings'
import { useEditorStore } from '@/stores/editor'
import { useFileTreeStore } from '@/stores/fileTree'
import { useArchiveStore } from '@/stores/archive'
import { registerCommands } from '@/composables/useCommands'
import { useViewportScale } from '@/composables/useViewportScale'
import { useExternalChange } from '@/composables/useExternalChange'
import { useFileDrop } from '@/composables/useFileDrop'
import { useAutoSave } from '@/composables/useAutoSave'
import { tr } from '@/i18n'

const settings = useSettingsStore()
const editor = useEditorStore()
const tree = useFileTreeStore()
const archive = useArchiveStore()

const ui = computed(() => settings.config.ui)

useViewportScale()
// 压缩任务事件接线（specs/18）：挂根视图，设置页路由来回切也只 init 一次。
void archive.init()
// 退出守卫与全局键在 App 根层注册：路由切到设置页时编辑器视图已卸载，
// 挂在这里会随视图一起销毁（设置页点 X 没反应就是这么来的）。
registerCommands()
useExternalChange()
useFileDrop()
useAutoSave()

onMounted(async () => {
  // 会话恢复：失败不阻塞启动（用户看到的是欢迎页）。只恢复一次——
  // 设置页 / 帮助页是独立路由，回来时本视图重新挂载，再灌一遍就是重复标签。
  if (settings.config.ui.restore_session && !editor.sessionRestored) {
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
          tr('toast.missingOnRestore', { n: restored.missing.length }),
        )
      }
    } catch {
      /* 恢复失败：欢迎页即可 */
    }
  }

  // 记住上次打开的文件夹：启动即进入用户熟悉的工作区。已在工作区就别重开。
  if (
    settings.config.file.remember_last_folder &&
    settings.config.file.last_folder &&
    !tree.root
  ) {
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
