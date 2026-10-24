// 工具箱浮窗宿主：JSON / Diff / 哈希 / 正则 / 编码 / 行编辑。
// 统一挂在最外层（不塞进某个组件）：浮窗要能盖住编辑器，
<template>
  <component
    :is="currentComp"
    v-if="current"
    :title="currentTitle"
    @close="dialogs.close()"
  />
</template>

<script lang="ts" setup>
import { computed, defineAsyncComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDialogStore } from '@/stores/dialogs'

const { t } = useI18n()
const dialogs = useDialogStore()

const COMPONENTS = {
  json: () => import('@/components/dialogs/JsonToolDialog.vue'),
  diff: () => import('@/components/dialogs/DiffToolDialog.vue'),
  hash: () => import('@/components/dialogs/HashToolDialog.vue'),
  regex: () => import('@/components/dialogs/RegexToolDialog.vue'),
  encoding: () => import('@/components/dialogs/EncodingToolDialog.vue'),
  lineOp: () => import('@/components/dialogs/LineOpToolDialog.vue'),
  find: () => import('@/components/dialogs/FindDialog.vue'),
  replace: () => import('@/components/dialogs/ReplaceDialog.vue'),
  findInFiles: () => import('@/components/dialogs/FindInFilesDialog.vue'),
  gotoLine: () => import('@/components/dialogs/GotoLineDialog.vue'),
  about: () => import('@/components/dialogs/AboutDialog.vue'),
  clipboard: () => import('@/components/dialogs/ClipboardDialog.vue'),
  hex: () => import('@/components/dialogs/HexViewerDialog.vue'),
  batchFindReplace: () => import('@/components/dialogs/BatchFindReplaceDialog.vue'),
  batchRename: () => import('@/components/dialogs/BatchRenameDialog.vue'),
  recentFiles: () => import('@/components/dialogs/RecentFilesDialog.vue'),
  convert: () => import('@/components/dialogs/ConvertDialog.vue'),
  dirDiff: () => import('@/components/dialogs/DirDiffDialog.vue'),
  colorPicker: () => import('@/components/dialogs/ColorPickerDialog.vue'),
  logViewer: () => import('@/components/dialogs/LogViewerDialog.vue'),
  drafts: () => import('@/components/dialogs/DraftsDialog.vue'),
  createArchive: () => import('@/components/dialogs/CreateArchiveDialog.vue'),
  archiveBrowser: () => import('@/components/dialogs/ArchiveBrowserDialog.vue'),
  archiveTasks: () => import('@/components/dialogs/ArchiveTasksDialog.vue'),
  splitJoin: () => import('@/components/dialogs/SplitJoinDialog.vue'),
} as const

const current = computed(() => dialogs.active)

// 每个浮窗的 props 不同（Hex 要 path），但联合类型会让 defineAsyncComponent 推断失败；
// 统一按「只收 title + 发 close」的契约装配，个别浮窗自己从 store 取附加参数。
const currentComp = computed(() => {
  const key = dialogs.active
  if (!key) return null
  const load = COMPONENTS[key as keyof typeof COMPONENTS]
  return load ? defineAsyncComponent(load as () => Promise<{ default: unknown }>) : null
})

const currentTitle = computed(() => {
  const key = dialogs.active
  if (!key) return ''
  const map: Record<string, string> = {
    json: 'tool.json',
    diff: 'tool.diffText',
    hash: 'tool.hash',
    regex: 'tool.regex',
    encoding: 'tool.encoding',
    lineOp: 'tool.lineOp',
    find: 'dialog.find',
    replace: 'dialog.replace',
    findInFiles: 'dialog.findInFiles',
    gotoLine: 'dialog.gotoLine',
    about: 'menu.about',
    clipboard: 'tool.clipboard',
    hex: 'tool.hexViewer',
    batchFindReplace: 'tool.batchFindReplace',
    batchRename: 'tool.batchRename',
    recentFiles: 'menu.recentFiles',
    convert: 'convert.title',
    dirDiff: 'dirDiff.title',
    colorPicker: 'viewer.colorPicker',
    logViewer: 'viewer.logViewer',
    drafts: 'dialog.drafts',
    createArchive: 'archive.createTitle',
    archiveBrowser: 'archive.browserTitle',
    archiveTasks: 'archive.tasksTitle',
    splitJoin: 'archive.splitTitle',
  }
  return t(map[key] ?? key)
})
</script>
