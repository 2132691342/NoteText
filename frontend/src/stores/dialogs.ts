// 工具箱与浮窗的状态：当前打开的是哪个、传给它的初始文本。
//
// 为什么用 store 而不是各处 ref：工具栏、菜单、快捷键、右键菜单都要能开浮窗，
// 而浮窗本体挂在 EditorView 的最外层。中间隔了编辑器滚动容器，

import { defineStore } from 'pinia'
import { ref } from 'vue'

export type DialogKind =
  | 'json'
  | 'diff'
  | 'hash'
  | 'regex'
  | 'encoding'
  | 'lineOp'
  | 'find'
  | 'replace'
  | 'findInFiles'
  | 'gotoLine'
  | 'about'
  | 'clipboard'
  | 'hex'
  | 'batchFindReplace'
  | 'batchRename'
  | 'recentFiles'

export const useDialogStore = defineStore('dialogs', () => {
  const active = ref<DialogKind | null>(null)
  /** 打开时预填的文本：查找框默认带选中内容。 */
  const seedText = ref('')
  /** 附带参数（如对比的左右两侧）。 */
  const seedExtra = ref<Record<string, unknown>>({})

  function open(kind: DialogKind, text = '', extra: Record<string, unknown> = {}) {
    active.value = kind
    seedText.value = text
    seedExtra.value = extra
  }

  function close() {
    active.value = null
    seedText.value = ''
    seedExtra.value = {}
  }

  return { active, seedText, seedExtra, open, close }
})
