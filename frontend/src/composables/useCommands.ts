// 命令注册：把「菜单 / 工具栏 / 快捷键 / 右键菜单」接到同一份命令表。
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
import { getText, getSelection, focusedTabId } from '@/composables/useEditorRegistry'
import {
  doUndo,
  doRedo,
  doSelectAll,
  doDeleteLine,
  doMoveLine,
  doDuplicateLine,
  doInsert,
  doToggleComment,
  doIndent,
  doGotoLine,
  doCaseTransform,
  doSortLines,
  doDedupLines,
  doTrimTrailing,
  doReverseLines,
  doConvertIndent,
} from '@/composables/useEditorActions'
import { baseName, nextUntitledIndex, readClipboard } from '@/utils'
import { toggleFullscreen } from '@/utils/window'
import {
  recording,
  steps,
  startRecording,
  stopRecording,
  playSteps,
} from '@/composables/useEditorMacro'
import {
  columnMode,
  setColumnMode,
  insertAtColumn,
  selectLinesAsColumns,
} from '@/composables/useEditorColumnMode'
import { get } from '@/composables/useEditorRegistry'
import { useFindResultsStore } from '@/stores/findResults'
import { saveTabById, saveTabAsById, closeTabById } from '@/composables/useCloseTab'

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
  /** 命令的作用对象：用户正在编辑的那个标签，退化为活跃标签。 */
  const targetId = () => focusedTabId() ?? editor.activeId

  /** 命令层用的薄封装：真正的实现在 useCloseTab（标签栏共用同一条链路）。 */
  const saveTab = saveTabById
  const saveTabAs = saveTabAsById
  const closeTab = closeTabById

  /** 打开一个路径并记入最近文件。 */
  async function openPath(path: string): Promise<void> {
    if (settings.config.file.remember_last_folder) {
      settings.updateFile({ last_folder: path.replace(/\\[^\\]*$/, '') })
    }
    const tab = await editor.openPath(path)
    if (tab) {
      void api.touchRecent(path)
      toast.ok(t('toast.opened', { name: baseName(path) }))
    }
  }

  const list: Command[] = [
    // ---- 文件 ----
    {
      id: 'file.new',
      group: 'file',
      labelKey: 'menu.new',
      key: 'Control+N',
      run: () => {
        const taken = editor.tabs
          .map((x) => new RegExp(`^(?:${t('tab.untitled')}|Untitled)-(\d+)$`).exec(x.title))
          .filter(Boolean)
          .map((m) => Number(m![1]))
        editor.newTab({
          title: `${t('tab.untitled')}-${nextUntitledIndex(taken)}`,
        })
        // 新标签的编辑器挂载后自动聚焦：用户按完 Ctrl+N 就期待能直接打字。
        requestAnimationFrame(() => {
          const id = editor.activeId
          if (id != null) get(id)?.focus()
        })
      },
    },
    {
      id: 'file.open',
      group: 'file',
      labelKey: 'menu.open',
      key: 'Control+O',
      run: async () => {
        const p = await api.openFileDialog(
          settings.config.file.remember_last_folder
            ? settings.config.file.last_folder
            : undefined,
        )
        if (!p) return
        await openPath(p)
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
        settings.updateUI({ show_sidebar: true })
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
        const id = targetId()
        if (id != null) await saveTab(id)
      },
    },
    {
      id: 'file.saveAs',
      group: 'file',
      labelKey: 'menu.saveAs',
      key: 'Control+Shift+S',
      enabled: () => hasTab(),
      run: async () => {
        const id = targetId()
        if (id != null) await saveTabAs(id)
      },
    },
    {
      id: 'file.saveAll',
      group: 'file',
      labelKey: 'menu.saveAll',
      key: 'Control+Alt+S',
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
        // 关「当前聚焦的」而不是「store 里的 active」：分屏时两者会不一致。
        const id = targetId()
        if (id != null) await closeTab(id)
      },
    },
    {
      id: 'file.closeAll',
      group: 'file',
      labelKey: 'menu.closeAll',
      enabled: () => hasTab(),
      run: async () => {
        // 逐个走确认：一次问一个，用户能在中途停下。
        for (const tab of [...editor.tabs]) {
          if (!(await closeTab(tab.id))) return
        }
      },
    },

    // ---- 编辑 ----
    {
      id: 'edit.undo',
      group: 'edit',
      labelKey: 'menu.undo',
      key: 'Control+Z',
      enabled: () => hasTab(),
      run: () => {
        doUndo(targetId())
      },
    },
    {
      id: 'edit.redo',
      group: 'edit',
      labelKey: 'menu.redo',
      key: 'Control+Y',
      enabled: () => hasTab(),
      run: () => {
        doRedo(targetId())
      },
    },
    {
      id: 'edit.selectAll',
      group: 'edit',
      labelKey: 'menu.selectAll',
      key: 'Control+A',
      enabled: () => hasTab(),
      run: () => {
        doSelectAll(targetId())
      },
    },
    // 剪贴板三件套：WebView 自带 Ctrl+X/C/V；菜单点下来得能落地。
    {
      id: 'edit.cut',
      group: 'edit',
      labelKey: 'menu.cut',
      key: 'Control+X',
      enabled: () => hasTab(),
      run: () => {
        const id = targetId()
        const sel = getSelection(id ?? -1)
        if (!sel) return
        void import('@/utils').then(async ({ copyText }) => {
          if (await copyText(sel)) doInsert(id, '')
        })
      },
    },
    {
      id: 'edit.copy',
      group: 'edit',
      labelKey: 'menu.copy',
      key: 'Control+C',
      enabled: () => hasTab(),
      run: () => {
        const sel = getSelection(targetId() ?? -1)
        if (!sel) return
        void import('@/utils').then(async ({ copyText }) => {
          if (await copyText(sel)) toast.ok(t('common.copied'))
        })
      },
    },
    {
      id: 'edit.paste',
      group: 'edit',
      labelKey: 'menu.paste',
      key: 'Control+V',
      enabled: () => hasTab(),
      run: async () => {
        const text = await readClipboard()
        if (!text) {
          toast.warn(t('toast.clipboardEmpty'))
          return
        }
        doInsert(targetId(), text)
      },
    },
    {
      id: 'edit.moveLineUp',
      group: 'edit',
      labelKey: 'menu.moveLineUp',
      key: 'Alt+Up',
      enabled: () => hasTab(),
      run: () => {
        doMoveLine(targetId(), 'up')
      },
    },
    {
      id: 'edit.moveLineDown',
      group: 'edit',
      labelKey: 'menu.moveLineDown',
      key: 'Alt+Down',
      enabled: () => hasTab(),
      run: () => {
        doMoveLine(targetId(), 'down')
      },
    },
    {
      id: 'edit.indent',
      group: 'edit',
      labelKey: 'menu.indent',
      key: 'Control+]',
      enabled: () => hasTab(),
      run: () => {
        doIndent(targetId(), false)
      },
    },
    {
      id: 'edit.outdent',
      group: 'edit',
      labelKey: 'menu.outdent',
      key: 'Control+[',
      enabled: () => hasTab(),
      run: () => {
        doIndent(targetId(), true)
      },
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
      key: 'Control+/',
      enabled: () => hasTab(),
      run: async () => {
        const id = targetId()
        const tab = editor.tabs.find((x) => x.id === id)
        if (!tab || id == null) return
        await doToggleComment(id, async () => {
          const sel = getSelection(id) || getText(id)
          return api.toggleLineComment(tab.language, sel)
        })
      },
    },
    {
      id: 'edit.duplicateLine',
      group: 'edit',
      labelKey: 'menu.duplicateLine',
      key: 'Control+D',
      enabled: () => hasTab(),
      run: () => {
        doDuplicateLine(targetId())
      },
    },
    {
      id: 'edit.deleteLine',
      group: 'edit',
      labelKey: 'menu.deleteLine',
      key: 'Control+Shift+K',
      enabled: () => hasTab(),
      run: () => {
        doDeleteLine(targetId())
      },
    },
    {
      id: 'edit.caseUpper',
      group: 'edit',
      labelKey: 'menu.caseUpper',
      key: 'Control+Shift+U',
      enabled: () => hasTab(),
      run: () => doCaseTransform(targetId(), 'upper'),
    },
    {
      id: 'edit.caseLower',
      group: 'edit',
      labelKey: 'menu.caseLower',
      // Ctrl+Shift+G：Ctrl+Shift+L 已被「行编辑」占用，而 Ctrl+Shift+K
      // 是「删除当前行」——大小写在编辑菜单里与它们同级，键位要错开。
      key: 'Control+Shift+G',
      enabled: () => hasTab(),
      run: () => doCaseTransform(targetId(), 'lower'),
    },
    {
      id: 'edit.caseTitle',
      group: 'edit',
      labelKey: 'menu.caseTitle',
      enabled: () => hasTab(),
      run: () => doCaseTransform(targetId(), 'title'),
    },
    {
      id: 'edit.sortAsc',
      group: 'edit',
      labelKey: 'menu.sortAsc',
      enabled: () => hasTab(),
      run: () => doSortLines(targetId(), 'asc'),
    },
    {
      id: 'edit.sortDesc',
      group: 'edit',
      labelKey: 'menu.sortDesc',
      enabled: () => hasTab(),
      run: () => doSortLines(targetId(), 'desc'),
    },
    {
      id: 'edit.dedupLines',
      group: 'edit',
      labelKey: 'menu.dedupLines',
      enabled: () => hasTab(),
      run: () => doDedupLines(targetId()),
    },
    {
      id: 'edit.reverseLines',
      group: 'edit',
      labelKey: 'menu.reverseLines',
      enabled: () => hasTab(),
      run: () => doReverseLines(targetId()),
    },
    {
      id: 'edit.trimTrailing',
      group: 'edit',
      labelKey: 'menu.trimTrailing',
      enabled: () => hasTab(),
      run: () => doTrimTrailing(targetId()),
    },
    {
      id: 'edit.tabToSpaces',
      group: 'edit',
      labelKey: 'menu.tabToSpaces',
      enabled: () => hasTab(),
      run: () =>
        doConvertIndent(targetId(), 'tab-to-space', settings.config.editor.tab_size, true),
    },
    {
      id: 'edit.spacesToTabs',
      group: 'edit',
      labelKey: 'menu.spacesToTabs',
      enabled: () => hasTab(),
      run: () =>
        doConvertIndent(targetId(), 'space-to-tab', settings.config.editor.tab_size, true),
    },

    // ---- 视图 ----
    {
      id: 'view.zoomIn',
      group: 'view',
      labelKey: 'menu.zoomIn',
      key: 'Control+=',
      run: () => settings.updateUI({ zoom_level: Math.min(200, settings.config.ui.zoom_level + 10) }),
    },
    {
      id: 'view.zoomOut',
      group: 'view',
      labelKey: 'menu.zoomOut',
      key: 'Control+-',
      run: () => settings.updateUI({ zoom_level: Math.max(50, settings.config.ui.zoom_level - 10) }),
    },
    {
      id: 'view.resetZoom',
      group: 'view',
      labelKey: 'menu.resetZoom',
      key: 'Control+0',
      run: () => settings.updateUI({ zoom_level: 100 }),
    },
    {
      id: 'view.toggleSidebar',
      group: 'view',
      labelKey: 'menu.toggleSidebar',
      key: 'Control+B',
      run: () => settings.updateUI({ show_sidebar: !settings.config.ui.show_sidebar }),
    },
    {
      id: 'view.toggleStatusbar',
      group: 'view',
      labelKey: 'menu.toggleStatusbar',
      key: 'Control+Alt+B',
      run: () => settings.updateUI({ show_status_bar: !settings.config.ui.show_status_bar }),
    },
    {
      id: 'view.toggleWordWrap',
      group: 'view',
      labelKey: 'menu.toggleWordWrap',
      key: 'Alt+Z',
      run: () => settings.updateEditor({ word_wrap: !settings.config.editor.word_wrap }),
    },
    {
      id: 'view.toggleMinimap',
      group: 'view',
      labelKey: 'menu.toggleMinimap',
      run: () => settings.updateEditor({ minimap: !settings.config.editor.minimap }),
    },
    {
      id: 'view.splitView',
      group: 'view',
      labelKey: 'menu.splitView',
      key: 'Control+\\',
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
        // 三档循环：浅 → 深 → 跟随系统 → 浅
        const cur = settings.config.theme
        const next = cur === 'light' ? 'dark' : cur === 'dark' ? 'system' : 'light'
        settings.update({ theme: next })
        const label =
          next === 'light'
            ? t('menu.themeLight')
            : next === 'dark'
              ? t('menu.themeDark')
              : t('menu.themeSystem')
        toast.ok(label)
      },
    },
    {
      id: 'view.fullscreen',
      group: 'view',
      labelKey: 'menu.fullscreen',
      key: 'F11',
      run: () => void toggleFullscreen(),
    },
    {
      id: 'file.print',
      group: 'file',
      labelKey: 'menu.print',
      key: 'Control+P',
      enabled: () => hasTab(),
      run: () => {
        // 打印走 WebView 自带对话框：打印样式表已把 chrome 隐藏（print.css），
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
      run: () => dialogs.open('encoding'),
    },

    // ---- 工具 ----
    {
      id: 'tools.json',
      group: 'tools',
      labelKey: 'tool.json',
      key: 'Control+Shift+J',
      run: () => dialogs.open('json', getSelection(focusedTabId() ?? -1)),
    },
    {
      id: 'tools.diff',
      group: 'tools',
      labelKey: 'tool.diffText',
      key: 'Control+Shift+D',
      run: () => dialogs.open('diff'),
    },
    {
      id: 'tools.hash',
      group: 'tools',
      labelKey: 'tool.hash',
      key: 'Control+Shift+H',
      run: () => dialogs.open('hash'),
    },
    {
      id: 'tools.regex',
      group: 'tools',
      labelKey: 'tool.regex',
      key: 'Control+Shift+R',
      run: () => dialogs.open('regex'),
    },
    {
      id: 'tools.encoding',
      group: 'tools',
      labelKey: 'tool.encoding',
      key: 'Control+Shift+E',
      run: () => dialogs.open('encoding'),
    },
    {
      id: 'tools.lineOp',
      group: 'tools',
      labelKey: 'tool.lineOp',
      key: 'Control+Shift+L',
      enabled: () => hasTab(),
      run: () => dialogs.open('lineOp'),
    },
    {
      id: 'tools.convert',
      group: 'tools',
      labelKey: 'convert.title',
      key: 'Control+Shift+C',
      run: () => dialogs.open('convert'),
    },
    {
      id: 'tools.dirDiff',
      group: 'tools',
      labelKey: 'dirDiff.title',
      run: () => dialogs.open('dirDiff'),
    },
    {
      id: 'tools.colorPicker',
      group: 'tools',
      labelKey: 'viewer.colorPicker',
      run: () => dialogs.open('colorPicker'),
    },
    {
      id: 'tools.logViewer',
      group: 'tools',
      labelKey: 'viewer.logViewer',
      run: () => dialogs.open('logViewer'),
    },

    // ---- 列编辑 ----
    {
      id: 'edit.columnMode',
      group: 'edit',
      labelKey: 'menu.columnMode',
      key: 'Alt+C',
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
      key: 'Control+Shift+M',
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
      key: 'Control+Shift+P',
      enabled: () => hasTab() && steps.value.length > 0,
      run: async () => {
        const id = focusedTabId() ?? editor.activeId
        const v = id != null ? get(id) : undefined
        if (!v) return
        await playSteps(v, steps.value, (cmdId) => commands.run(cmdId))
      },
    },

    // ---- 剪贴板 / 预览 ----
    {
      id: 'tools.clipboard',
      group: 'tools',
      labelKey: 'tool.clipboard',
      key: 'Control+Shift+V',
      run: () => dialogs.open('clipboard'),
    },
    {
      id: 'tools.hex',
      group: 'tools',
      labelKey: 'tool.hexViewer',
      key: 'Control+Alt+H',
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
      key: 'Control+Shift+B',
      run: () => dialogs.open('batchFindReplace'),
    },
    {
      id: 'tools.batchRename',
      group: 'tools',
      labelKey: 'tool.batchRename',
      key: 'Control+Alt+R',
      run: () => dialogs.open('batchRename'),
    },

    // ---- 书签 / 帮助 ----
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
      key: 'Shift+F2',
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
        const next =
          list.map((b) => b.line).sort((a, b) => a - b).find((n) => n > cur) ??
          list[0].line
        doGotoLine(tab.id, next)
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
    {
      id: 'help.shortcuts',
      group: 'help',
      labelKey: 'menu.shortcuts',
      run: () => {
        window.location.hash = '#/help/shortcuts'
      },
    },
    {
      id: 'view.settings',
      group: 'view',
      labelKey: 'menu.settings',
      key: 'Control+,',
      run: () => {
        window.location.hash = '#/settings'
      },
    },
  ]

  commands.register(list)

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

  return { pushFindResults, saveTab, saveTabAs, closeTab, openPath }
}
