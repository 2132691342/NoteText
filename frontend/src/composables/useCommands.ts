// 命令注册：把「菜单 / 工具栏 / 快捷键」三个入口接到同一份命令表。
//
// 铁律：新功能先在这里注册，再在菜单或工具栏加入口。
// 反过来做（先在菜单写死一个动作）就会出现「菜单能用的功能按键按不了」。
import { useCommandStore, type Command } from '@/stores/commands'
import { useEditorStore } from '@/stores/editor'
import { useFileTreeStore } from '@/stores/fileTree'
import { useSettingsStore } from '@/stores/settings'
import { useDialogStore } from '@/stores/dialogs'
import { useToastStore } from '@/stores/toast'
import { useI18n } from 'vue-i18n'
import { api, normalizeError } from '@/api'
import { getText, getSelection, focusedTabId, get } from '@/composables/useEditorRegistry'
import { baseName, nextUntitledIndex, readClipboard } from '@/utils'
import { toggleFullscreen } from '@/utils/window'
import {
  recording,
  steps,
  startRecording,
  stopRecording,
  playSteps,
} from '@/composables/useEditorMacro'
import { columnMode, setColumnMode, insertAtColumn, selectLinesAsColumns } from '@/composables/useEditorColumnMode'
import { useFindResultsStore } from '@/stores/findResults'

export function registerCommands() {
  const { t } = useI18n()
  const commands = useCommandStore()
  const editor = useEditorStore()
  const tree = useFileTreeStore()
  const settings = useSettingsStore()
  const dialogs = useDialogStore()
  const toast = useToastStore()
  const findResults = useFindResultsStore()

  const hasTab = () => editor.activeId != null
  const hasFile = () => !!editor.active?.path && !editor.active?.path.startsWith('draft:')
  const hasDirty = () => editor.dirtyCount > 0

  const list: Command[] = [
    // ---- 文件 ----
    {
      id: 'file.new',
      group: 'file',
      labelKey: 'menu.new',
      key: 'Control+N',
      run: () => {
        const taken = editor.tabs
          .map((t) => /^未命名-(\d+)$/.exec(t.title))
          .filter(Boolean)
          .map((m) => Number(m![1]))
        editor.newTab({ title: `未命名-${nextUntitledIndex(taken)}` })
      },
    },
    {
      id: 'file.open',
      group: 'file',
      labelKey: 'menu.open',
      key: 'Control+O',
      run: async () => {
        const p = await api.openFileDialog(
          settings.config.file.remember_last_folder ? settings.config.file.last_folder : undefined,
        )
        if (!p) return
        if (settings.config.file.remember_last_folder) {
          const dir = p.replace(/\\[^\\]*$/, '')
          settings.updateFile({ last_folder: dir })
        }
        const tab = await editor.openPath(p)
        if (tab) {
          void api.touchRecent(p)
          toast.ok(t('toast.opened', { name: baseName(p) }))
        }
      },
    },
    {
      id: 'file.openRecent',
      group: 'file',
      labelKey: 'menu.recentFiles',
      key: 'Control+Shift+O',
      run: () => dialogs.open('recentFiles'),
    },
    {
      id: 'file.openFolder',
      group: 'file',
      labelKey: 'menu.openFolder',
      key: 'Control+K',
      run: async () => {
        const p = await api.openFolderDialog(tree.root || undefined)
        if (!p) return
        await tree.open(p)
        if (settings.config.file.remember_last_folder) {
          settings.updateFile({ last_folder: p })
        }
      },
    },
    {
      id: 'file.save',
      group: 'file',
      labelKey: 'menu.save',
      key: 'Control+S',
      enabled: () => hasTab(),
      run: async () => {
        const tab = editor.active
        if (!tab) return
        if (tab.path.startsWith('draft:')) {
          await commands.run('file.saveAs')
          return
        }
        await saveTab(tab.id)
      },
    },
    {
      id: 'file.saveAs',
      group: 'file',
      labelKey: 'menu.saveAs',
      key: 'Control+Shift+S',
      enabled: () => hasTab(),
      run: async () => {
        const tab = editor.active
        if (!tab) return
        const text = getText(tab.id)
        const suggested = tab.title || '未命名.txt'
        const res = await api.saveFileAs(text, suggested, tab.encoding, tab.line_ending, tab.has_bom)
        if (!res) return
        tab.path = res.path
        tab.title = baseName(res.path)
        tab.dirty = false
        if (tab.path.startsWith('draft:')) void api.removeDraft(tab.path.slice(6))
        void api.touchRecent(res.path)
        toast.ok(t('toast.saved', { name: tab.title }))
      },
    },
    {
      id: 'file.saveAll',
      group: 'file',
      labelKey: 'menu.saveAll',
      enabled: () => hasDirty(),
      run: async () => {
        for (const tab of editor.tabs.filter((x) => x.dirty)) {
          await saveTab(tab.id)
        }
      },
    },
    {
      id: 'file.closeTab',
      group: 'file',
      labelKey: 'menu.closeTab',
      key: 'Control+W',
      enabled: () => hasTab(),
      run: async () => {
        const tab = editor.active
        if (!tab) return
        if (!tab.dirty) {
          editor.close(tab.id)
          return
        }
        // 脏标签：弹原生确认（保存 / 不保存 / 取消三选一）。
        const ok = await api.showConfirm(
          t('app.quitConfirmTitle'),
          t('app.quitConfirmBody', { count: 1 }),
        )
        if (ok) {
          await saveTab(tab.id)
          editor.close(tab.id, true)
        } else {
          editor.close(tab.id, true)
        }
      },
    },
    {
      id: 'file.closeAll',
      group: 'file',
      labelKey: 'menu.closeAll',
      enabled: () => hasTab(),
      run: async () => {
        if (!hasDirty()) {
          editor.closeAll()
          return
        }
        const ok = await api.showConfirm(
          t('app.quitConfirmTitle'),
          t('app.quitConfirmBody', { count: editor.dirtyCount }),
        )
        if (!ok) return
        for (const tab of editor.tabs.filter((x) => x.dirty)) {
          await saveTab(tab.id)
        }
        editor.closeAll()
      },
    },

    // ---- 编辑 ----
    {
      id: 'edit.undo',
      group: 'edit',
      labelKey: 'menu.undo',
      run: () => emitEditorCmd('undo'),
    },
    {
      id: 'edit.redo',
      group: 'edit',
      labelKey: 'menu.redo',
      run: () => emitEditorCmd('redo'),
    },
    {
      id: 'edit.selectAll',
      group: 'edit',
      labelKey: 'menu.selectAll',
      run: () => emitEditorCmd('selectAll'),
    },
    // 剪贴板三件套：默认键位表里一直有 Ctrl+X/C/V，但没有任何命令用这三个 id，
    // 按下完全无响应。不拦默认：CodeMirror 自己的 keymap 先拿到，
    // 这里只是让它们能注册、能被键位表与菜单引用。
    {
      id: 'edit.cut',
      group: 'edit',
      labelKey: 'menu.cut',
      enabled: () => hasTab(),
      run: () => document.execCommand('cut'),
    },
    {
      id: 'edit.copy',
      group: 'edit',
      labelKey: 'menu.copy',
      enabled: () => hasTab(),
      run: () => document.execCommand('copy'),
    },
    {
      id: 'edit.paste',
      group: 'edit',
      labelKey: 'menu.paste',
      enabled: () => hasTab(),
      run: async () => {
        const text = await readClipboard()
        if (!text) return
        emitEditorCmd('insert', text)
      },
    },
    // 行移动：CodeMirror 自带这两个命令，只是之前没接到命令表。
    {
      id: 'edit.moveLineUp',
      group: 'edit',
      labelKey: 'menu.moveLineUp',
      enabled: () => hasTab(),
      run: () => emitEditorCmd('moveLineUp'),
    },
    {
      id: 'edit.moveLineDown',
      group: 'edit',
      labelKey: 'menu.moveLineDown',
      enabled: () => hasTab(),
      run: () => emitEditorCmd('moveLineDown'),
    },
    {
      id: 'edit.find',
      group: 'edit',
      labelKey: 'menu.find',
      key: 'Control+F',
      run: () => dialogs.open('find', getSelection(focusedTabId() ?? -1)),
    },
    {
      id: 'edit.replace',
      group: 'edit',
      labelKey: 'menu.replace',
      key: 'Control+H',
      run: () => dialogs.open('replace', getSelection(focusedTabId() ?? -1)),
    },
    {
      id: 'edit.findInFiles',
      group: 'edit',
      labelKey: 'menu.findInFiles',
      key: 'Control+Shift+F',
      run: () => dialogs.open('findInFiles'),
    },
    {
      id: 'edit.gotoLine',
      group: 'edit',
      labelKey: 'menu.gotoLine',
      key: 'Control+G',
      enabled: () => hasTab(),
      run: () => dialogs.open('gotoLine'),
    },
    {
      id: 'edit.toggleComment',
      group: 'edit',
      labelKey: 'menu.toggleComment',
      enabled: () => hasTab(),
      run: async () => {
        const tab = editor.active
        if (!tab) return
        const out = await api.toggleLineComment(tab.language, getSelection(tab.id) || getText(tab.id))
        emitEditorCmd('replaceSelection', out)
      },
    },
    {
      id: 'edit.duplicateLine',
      group: 'edit',
      labelKey: 'menu.copy',
      run: () => emitEditorCmd('duplicateLine'),
    },
    {
      id: 'edit.deleteLine',
      group: 'edit',
      labelKey: 'menu.cut',
      run: () => emitEditorCmd('deleteLine'),
    },

    // ---- 视图 ----
    {
      id: 'view.zoomIn',
      group: 'view',
      labelKey: 'menu.zoomIn',
      run: () => settings.updateUI({ zoom_level: Math.min(200, settings.config.ui.zoom_level + 10) }),
    },
    {
      id: 'view.zoomOut',
      group: 'view',
      labelKey: 'menu.zoomOut',
      run: () => settings.updateUI({ zoom_level: Math.max(50, settings.config.ui.zoom_level - 10) }),
    },
    {
      id: 'view.resetZoom',
      group: 'view',
      labelKey: 'menu.resetZoom',
      run: () => settings.updateUI({ zoom_level: 100 }),
    },
    {
      id: 'view.toggleSidebar',
      group: 'view',
      labelKey: 'menu.toggleSidebar',
      run: () => settings.updateUI({ show_sidebar: !settings.config.ui.show_sidebar }),
    },
    {
      id: 'view.toggleStatusbar',
      group: 'view',
      labelKey: 'menu.toggleStatusbar',
      run: () => settings.updateUI({ show_status_bar: !settings.config.ui.show_status_bar }),
    },
    {
      id: 'view.splitView',
      group: 'view',
      labelKey: 'menu.splitView',
      enabled: () => hasTab(),
      run: () => {
        const tab = editor.active
        if (tab) editor.setSplit(tab.id, !tab.split)
      },
    },
    {
      id: 'view.theme',
      group: 'view',
      labelKey: 'menu.theme',
      run: () => {
        const next = settings.resolvedTheme === 'dark' ? 'light' : 'dark'
        settings.update({ theme: next })
      },
    },
    {
      id: 'view.fullscreen',
      group: 'view',
      labelKey: 'menu.fullscreen',
      key: 'F11',
      // 全屏是窗口态，不进配置（那是后端持久化的契约，
      // 存一个重启后为假的字段只会把两边弄不一致）。
      run: () => void toggleFullscreen(),
    },
    {
      id: 'file.print',
      group: 'file',
      labelKey: 'menu.print',
      key: 'Control+P',
      enabled: () => hasTab(),
      run: () => {
        // 打印走 WebView 自带对话框：打印样式表已把chrome隐藏（print.css），
        // 打印出来只有正文。不引入 PDF 库——系统打印对话框自带「另存为 PDF」。
        window.print()
      },
    },
    {
      id: 'view.encoding',
      group: 'view',
      labelKey: 'menu.encoding',
      enabled: () => hasTab(),
      run: () => dialogs.open('encoding'),
    },
    {
      id: 'view.lineEnding',
      group: 'view',
      labelKey: 'menu.lineEnding',
      enabled: () => hasTab(),
      run: async () => {
        const tab = editor.active
        if (!tab) return
        const next = tab.line_ending === 'crlf' ? 'lf' : tab.line_ending === 'lf' ? 'cr' : 'crlf'
        const out = await api.convertLineEnding(tab.path, getText(tab.id), next)
        emitEditorCmd('replaceSelection', out)
        tab.line_ending = next
      },
    },
    {
      id: 'view.language',
      group: 'view',
      labelKey: 'menu.tools',
      enabled: () => hasTab(),
      run: () => dialogs.open('encoding'),
    },

    // ---- 工具 ----
    {
      id: 'tools.json',
      group: 'tools',
      labelKey: 'tool.json',
      run: () => dialogs.open('json', getSelection(focusedTabId() ?? -1)),
    },
    {
      id: 'tools.diff',
      group: 'tools',
      labelKey: 'tool.diffText',
      run: () => dialogs.open('diff'),
    },
    {
      id: 'tools.hash',
      group: 'tools',
      labelKey: 'tool.hash',
      run: () => dialogs.open('hash'),
    },
    {
      id: 'tools.regex',
      group: 'tools',
      labelKey: 'tool.regex',
      run: () => dialogs.open('regex'),
    },
    {
      id: 'tools.encoding',
      group: 'tools',
      labelKey: 'tool.encoding',
      run: () => dialogs.open('encoding'),
    },
    {
      id: 'tools.lineOp',
      group: 'tools',
      labelKey: 'tool.lineOp',
      enabled: () => hasTab(),
      run: () => dialogs.open('lineOp'),
    },

    // ---- 列编辑 ----
    {
      id: 'edit.columnMode',
      group: 'edit',
      labelKey: 'menu.columnMode',
      enabled: () => hasTab(),
      run: () => {
        const next = !columnMode.value
        setColumnMode(next)
        const id = focusedTabId()
        const v = id != null ? get(id) : undefined
        // 开列模式时把当前选区转成逐行选区，后续输入才会落在每行同列。
        if (next && v) selectLinesAsColumns(v)
        toast.ok(next ? t('toast.columnOn') : t('toast.columnOff'))
      },
    },
    {
      id: 'edit.columnInsert',
      group: 'edit',
      labelKey: 'menu.columnInsert',
      enabled: () => hasTab() && columnMode.value,
      run: () => {
        const id = focusedTabId()
        const v = id != null ? get(id) : undefined
        if (!v) return
        const text = getSelection(id ?? -1)
        if (!insertAtColumn(v, text || ' ')) toast.bad(t('toast.columnNoSelection'))
      },
    },

    // ---- 宏 ----
    {
      id: 'macro.record',
      group: 'tools',
      labelKey: 'menu.macroRecord',
      run: () => {
        if (recording.value) {
          const got = stopRecording()
          toast.ok(t('toast.macroRecordStop', { count: got.length }))
          // 录完直接弹保存：让用户给这串动作起个名字，否则下次找不到。
          if (got.length) void saveMacro(got)
          return
        }
        startRecording()
        toast.ok(t('toast.macroRecordStart'))
      },
    },
    {
      id: 'macro.play',
      group: 'tools',
      labelKey: 'menu.macroPlay',
      enabled: () => hasTab() && steps.value.length > 0,
      run: async () => {
        const id = focusedTabId() ?? editor.activeId
        const v = id != null ? get(id) : undefined
        if (!v) return
        await playSteps(v, steps.value, (cmdId) => commands.run(cmdId))
      },
    },
    {
      id: 'macro.list',
      group: 'tools',
      labelKey: 'menu.macroList',
      run: () => {
        settings.updateUI({ show_sidebar: true })
        document.dispatchEvent(new CustomEvent('sidebar-tab', { detail: 'macros' }))
      },
    },

    // ---- 剪贴板 / 预览 ----
    {
      id: 'tools.clipboard',
      group: 'tools',
      labelKey: 'tool.clipboard',
      run: () => dialogs.open('clipboard'),
    },
    {
      id: 'tools.hex',
      group: 'tools',
      labelKey: 'tool.hexViewer',
      enabled: () => hasFile(),
      run: () => dialogs.open('hex', '', { path: editor.active!.path }),
    },
    {
      id: 'tools.preview',
      group: 'tools',
      labelKey: 'tool.preview',
      enabled: () => hasFile(),
      run: () => {
        const tab = editor.active
        if (!tab) return
        // 预览与编辑两种视图来回切：按扩展名决定能不能预览，
        // 不支持的格式由 DocPreview 自己给空态提示。
        tab.showing_preview = !tab.showing_preview
      },
    },

    // ---- 批量操作 ----
    {
      id: 'tools.batchFindReplace',
      group: 'tools',
      labelKey: 'tool.batchFindReplace',
      run: () => dialogs.open('batchFindReplace'),
    },
    {
      id: 'tools.batchRename',
      group: 'tools',
      labelKey: 'tool.batchRename',
      run: () => dialogs.open('batchRename'),
    },

    // ---- 书签 / 宏 / 帮助 ----
    {
      id: 'bookmark.toggle',
      group: 'tools',
      labelKey: 'sidebar.bookmarks',
      key: 'F2',
      enabled: () => hasFile(),
      run: async () => {
        const tab = editor.active
        if (!tab?.path) return
        const added = await api.toggleBookmark({ path: tab.path, line: tab.position.line })
        toast.ok(added ? t('toast.bookmarkAdded') : t('toast.bookmarkRemoved'))
        document.dispatchEvent(new CustomEvent('bookmarks-changed'))
      },
    },
    {
      id: 'bookmark.next',
      group: 'tools',
      labelKey: 'menu.bookmarkNext',
      enabled: () => hasFile(),
      run: async () => {
        const tab = editor.active
        if (!tab?.path) return
        const list = await api.listFileBookmarks(tab.path)
        if (list.length === 0) {
          toast.warn(t('toast.noBookmarksInFile'))
          return
        }
        // 下一个：按行号升序找第一个大于当前光标行的，没有则回到第一个。
        const cur = tab.position.line
        const next = list.map((b) => b.line).sort((a, b) => a - b).find((n) => n > cur) ?? list[0].line
        document.dispatchEvent(
          new CustomEvent('editor-goto-line', { detail: { tabId: tab.id, line: next } }),
        )
      },
    },
    {
      id: 'help.manual',
      group: 'help',
      labelKey: 'menu.userManual',
      key: 'F1',
      run: () => {
        window.location.hash = '#/help'
      },
    },
  ]

  commands.register(list)

  /** 保存一个标签：正文从编辑器取，编码 / 行尾 / BOM 按 tab 上的值写回。 */
  async function saveTab(id: number) {
    const tab = editor.tabs.find((x) => x.id === id)
    if (!tab?.path || tab.path.startsWith('draft:')) return
    // 截断的标签只有部分内容：写回就是把剩余部分删掉，直接拒绝。
    if (tab.truncated) {
      toast.warn(t('toast.saveBlockedTruncated'))
      return
    }
    const text = getText(id)
    try {
      await api.writeFile(tab.path, text, tab.encoding, tab.line_ending, tab.has_bom)
      editor.markDirty(id, false)
      tree.markDirty(tab.path, false)
      void api.touchRecent(tab.path)
      toast.ok(t('toast.saved', { name: tab.title }))
    } catch (e) {
      const err = normalizeError(e)
      toast.bad(`${t('error.writeFailed', { reason: err.message })}`)
    }
  }

  /** 给编辑器组件发一个命令：编辑器自己知道怎么执行。 */
  function emitEditorCmd(cmd: string, payload?: unknown) {
    document.dispatchEvent(
      new CustomEvent('editor-cmd', { detail: { cmd, payload } }),
    )
  }

  /** 把录到的步骤存成宏。名字用「宏 N」：用户可以在侧栏改名。 */
  async function saveMacro(list: typeof steps.value) {
    try {
      const existing = await api.listMacros()
      await api.saveMacro({
        name: `${t('menu.macro')} ${existing.length + 1}`,
        steps: list,
      })
      document.dispatchEvent(new CustomEvent('macros-changed'))
    } catch (e) {
      toast.bad(t('error.macroFailed', { reason: normalizeError(e).message }))
    }
  }

  /**
   * 目录级查找的结果写进侧栏面板。
   *
   * 浮窗负责收集条件，结果落在面板——命中几十个文件时用户要反复点开，
   * 浮窗一关结果就没了。
   */
  function pushFindResults(hits: import('@/types/api').BatchFileHits[], count: number) {
    findResults.set(hits, count)
    settings.updateUI({ show_sidebar: true })
  }

  return { pushFindResults }
}
