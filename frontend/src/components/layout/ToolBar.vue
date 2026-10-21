// 工具栏：高频操作的图标钮一排。
//
// 只放「每天都用」的几个（新建 / 打开 / 保存 / 撤销 / 查找 / 替换 / 换行 / 缩放），
// 其余进菜单。工具栏塞满会让每个都难找——那是菜单的职责。
<template>
  <div class="nt-toolbar">
    <button class="nt-icon-btn" :title="t('toolbar.new')" @click="commands.run('file.new')">
      <NtIcon name="plus" />
    </button>
    <button class="nt-icon-btn" :title="t('toolbar.open')" @click="commands.run('file.open')">
      <NtIcon name="folder-open" />
    </button>
    <button
      class="nt-icon-btn"
      :title="t('toolbar.save')"
      :disabled="!editor.active?.dirty"
      @click="commands.run('file.save')"
    >
      <NtIcon name="save" />
    </button>
    <span class="nt-toolbar-sep" />
    <button
      class="nt-icon-btn"
      :title="t('toolbar.undo')"
      :disabled="!canUndo"
      @click="commands.run('edit.undo')"
    >
      <NtIcon name="undo" />
    </button>
    <button
      class="nt-icon-btn"
      :title="t('toolbar.redo')"
      :disabled="!canRedo"
      @click="commands.run('edit.redo')"
    >
      <NtIcon name="redo" />
    </button>
    <span class="nt-toolbar-sep" />
    <button class="nt-icon-btn" :title="t('toolbar.find')" @click="commands.run('edit.find')">
      <NtIcon name="search" />
    </button>
    <button class="nt-icon-btn" :title="t('toolbar.replace')" @click="commands.run('edit.replace')">
      <NtIcon name="replace" />
    </button>
    <span class="nt-toolbar-sep" />
    <button
      class="nt-icon-btn"
      :class="{ 'is-on': editorCfg.word_wrap }"
      :title="t('toolbar.wordWrap')"
      @click="toggleWrap"
    >
      <NtIcon name="wrap" />
    </button>
    <span class="nt-toolbar-sep" />
    <button class="nt-icon-btn" :title="t('toolbar.zoomOut')" @click="zoom(-10)">
      <NtIcon name="zoom-out" />
    </button>
    <span class="nt-readout nt-toolbar-zoom">{{ ui.zoom_level }}%</span>
    <button class="nt-icon-btn" :title="t('toolbar.zoomIn')" @click="zoom(10)">
      <NtIcon name="zoom-in" />
    </button>
    <span class="nt-toolbar-sep" />
    <button
      class="nt-icon-btn"
      :title="t('menu.splitView')"
      :class="{ 'is-on': editor.active?.split }"
      @click="commands.run('view.splitView')"
    >
      <NtIcon name="split" />
    </button>
    <div class="nt-toolbar-spacer" />
    <!-- 工具入口：不塞满工具栏，收进「更多」下拉 -->
    <ElDropdown trigger="click" @command="onTool">
      <button class="nt-icon-btn" :title="t('toolbar.more')">
        <NtIcon name="braces" />
      </button>
      <template #dropdown>
        <ElDropdownMenu>
          <ElDropdownItem command="json">{{ t('tool.json') }}</ElDropdownItem>
          <ElDropdownItem command="diff">{{ t('tool.diffText') }}</ElDropdownItem>
          <ElDropdownItem command="hash">{{ t('tool.hash') }}</ElDropdownItem>
          <ElDropdownItem command="regex">{{ t('tool.regex') }}</ElDropdownItem>
          <ElDropdownItem command="encoding">{{ t('tool.encoding') }}</ElDropdownItem>
          <ElDropdownItem command="lineOp">{{ t('tool.lineOp') }}</ElDropdownItem>
          <ElDropdownItem command="clipboard" divided>{{ t('tool.clipboard') }}</ElDropdownItem>
          <ElDropdownItem command="hex">{{ t('tool.hexViewer') }}</ElDropdownItem>
          <ElDropdownItem command="batchFindReplace">
            {{ t('tool.batchFindReplace') }}
          </ElDropdownItem>
          <ElDropdownItem command="batchRename">{{ t('tool.batchRename') }}</ElDropdownItem>
        </ElDropdownMenu>
      </template>
    </ElDropdown>
  </div>
</template>

<script lang="ts" setup>
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElDropdown, ElDropdownItem, ElDropdownMenu } from 'element-plus'
import NtIcon from '@/components/common/NtIcon.vue'
import { useEditorStore } from '@/stores/editor'
import { useSettingsStore } from '@/stores/settings'
import { useCommandStore } from '@/stores/commands'

const { t } = useI18n()
const editor = useEditorStore()
const settings = useSettingsStore()
const commands = useCommandStore()

const ui = computed(() => settings.config.ui)
const editorCfg = computed(() => settings.config.editor)

// 撤销 / 重做可用性：CodeMirror 的历史栈只在组件层，这里通过事件同步。
const canUndo = computed(() => undoDepth.value > 0)
const canRedo = computed(() => redoDepth.value > 0)
const undoDepth = ref(0)
const redoDepth = ref(0)
import { onMounted, onUnmounted, ref } from 'vue'

function onHistory(e: Event) {
  const d = (e as CustomEvent).detail
  undoDepth.value = d?.undo ?? 0
  redoDepth.value = d?.redo ?? 0
}
onMounted(() => document.addEventListener('editor-history', onHistory))
onUnmounted(() => document.removeEventListener('editor-history', onHistory))

function toggleWrap() {
  settings.updateEditor({ word_wrap: !editorCfg.value.word_wrap })
}

function zoom(delta: number) {
  const next = Math.min(200, Math.max(50, ui.value.zoom_level + delta))
  settings.updateUI({ zoom_level: next })
}

function onTool(cmd: string) {
  commands.run(`tools.${cmd}`)
}
</script>

<style scoped>
.nt-toolbar-spacer {
  flex: 1;
}

.nt-toolbar-zoom {
  font-size: var(--nt-fs-hint);
  min-width: 3.2em;
  text-align: center;
  flex: none;
}
</style>
