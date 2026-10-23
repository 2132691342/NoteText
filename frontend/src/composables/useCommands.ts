// 命令注册：把「菜单 / 工具栏 / 快捷键 / 右键菜单」接到同一份命令表。
// 铁律：新功能先在这里注册，再在菜单或工具栏加入口。
import { useCommandStore, type Command } from '@/stores/commands'
import { useEditorStore } from '@/stores/editor'
import { useFileTreeStore } from '@/stores/fileTree'
import { useSettingsStore } from '@/stores/settings'
import { useDialogStore } from '@/stores/dialogs'
import { useToastStore } from '@/stores/toast'
import { useI18n } from 'vue-i18n'
import { setLocale } from '@/i18n'
import { api, normalizeError } from '@/api'
import { getText, getSelection, focusedTabId, commitBaseline } from '@/composables/useEditorRegistry'
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
  doGotoBracket,
  doCaseTransform,
  doSortLines,
  doDedupLines,
  doTrimTrailing,
  doReverseLines,
  doConvertIndent,
  doReplaceDoc,
} from '@/composables/useEditorActions'
import { baseName, nextUntitledIndex, readClipboard, dirName, joinPath, extName } from '@/utils'
import { askText, confirmText } from '@/components/common/askText'
import { askPick } from '@/components/common/askPick'
import { goBack, goForward } from '@/composables/useNavHistory'
import { toggleFullscreen } from '@/utils/window'
import { encodingLabel } from '@/utils/encoding'
import {
  recording,
  steps,
  startRecording,
  stopRecording,
  playSteps,
} from '@/composables/useEditorMacro'
import {
  quickJsonFormat,
  quickJsonMinify,
  quickEscape,
  quickUnescape,
  quickConvert,
} from '@/composables/useQuickActions'
import {
  columnMode,
  setColumnMode,
  insertAtColumn,
  selectLinesAsColumns,
} from '@/composables/useEditorColumnMode'
import { get } from '@/composables/useEditorRegistry'
import { useFindResultsStore } from '@/stores/findResults'
import { stepLastQuery } from '@/composables/useLastQuery'
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

  /** 列表选择：语言 / 编码这类「从后端清单里挑一个」的场景。 */
  const pickFromList = (opts: {
    title: string
    items: Array<{ value: string; label: string; hint?: string }>
    current?: string
  }) => askPick(opts)

  /**
   * 按指定编码重新读一个标签。
   *
   * 与 reload 的区别：reload 走后端的自动探测，这里强制用用户选的编码。
   * 用于「文件是 GBK 但被猜成 UTF-8，打开一片乱码」这个场景。
   */
  async function reopenWithEncoding(tabId: number, encoding: string) {
    const tab = editor.tabs.find((t) => t.id === tabId)
    if (!tab?.path) return
    try {
      const c = await api.readFile(tab.path, encoding)
      const v = get(tabId)
      if (v) doReplaceDoc(tabId, c.text)
      tab.encoding = c.encoding
      tab.line_ending = c.line_ending
      tab.has_bom = c.has_bom
      commitBaseline(tabId)
      editor.markDirty(tabId, false)
      toast.ok(t('toast.encodingChanged', { encoding: c.encoding }))
    } catch (e) {
      toast.bad(t('toast.openFailed', { reason: normalizeError(e).message }))
    }
  }

  /** 命令的作用对象：用户正在编辑的那个标签，退化为活跃标签。 */
  const targetId = () => focusedTabId() ?? editor.activeId

  /**
   * 浮窗的选区种子：预填「用户选中的文本」。
   *
   * 从注册表视图直接读选区而不是看 DOM 焦点——右键菜单的点击会把焦点
   * 抢到菜单按钮上，focusedTabId() 随即变 null，选中词就丢了
   * （表现为「选中→右键→查找」预填为空）。
   */
  const selectionSeed = () => getSelection(targetId() ?? -1)

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
    {
      id: 'file.closeOthers',
      group: 'file',
      labelKey: 'menu.closeOthers',
      enabled: () => editor.tabs.length > 1,
      run: () => {
        const id = targetId()
        if (id == null) return
        editor.closeOthers(id)
      },
    },
    {
      id: 'file.closeLeft',
      group: 'file',
      labelKey: 'menu.closeLeft',
      enabled: () => {
        const id = targetId()
        return id != null && editor.tabs.findIndex((t) => t.id === id) > 0
      },
      run: () => {
        const id = targetId()
        if (id != null) editor.closeLeft(id)
      },
    },
    {
      id: 'file.closeRight',
      group: 'file',
      labelKey: 'menu.closeRight',
      enabled: () => {
        const id = targetId()
        const i = id == null ? -1 : editor.tabs.findIndex((t) => t.id === id)
        return i >= 0 && i < editor.tabs.length - 1
      },
      run: () => {
        const id = targetId()
        if (id != null) editor.closeRight(id)
      },
    },
    {
      id: 'file.closeSaved',
      group: 'file',
      labelKey: 'menu.closeSaved',
      enabled: () => editor.tabs.some((t) => !t.dirty),
      run: () => editor.closeSaved(),
    },
    {
      id: 'file.rename',
      group: 'file',
      labelKey: 'menu.rename',
      enabled: () => hasFile(),
      run: async () => {
        const tab = editor.active
        if (!tab?.path) return
        const oldName = baseName(tab.path)
        const name = await askText({
          title: t('menu.rename'),
          label: t('dialog.newName'),
          value: oldName,
        })
        if (!name || name === oldName) return
        if (/[\\/:*?"<>|]/.test(name)) {
          toast.bad(t('toast.badFileName'))
          return
        }
        const target = joinPath(dirName(tab.path), name)
        try {
          const res = await api.fileOps.rename(tab.path, target)
          const first = res.results?.[0]
          if (first && !first.ok) {
            toast.bad(first.message || t('toast.renameFailed'))
            return
          }
          // 已打开标签的路径跟着改：不改的话保存会写回旧路径，
          // 用户看到的是「另存了一份，原文件还在旧名字下面」。
          editor.renamePath(tab.path, target)
          tree.markDirty(tab.path, false)
          if (tree.root) await tree.refresh(tree.root)
          toast.ok(t('toast.renamed', { name }))
        } catch (e) {
          toast.bad(t('toast.renameFailed', { reason: normalizeError(e).message }))
        }
      },
    },
    {
      id: 'file.reload',
      group: 'file',
      labelKey: 'menu.reload',
      enabled: () => hasFile(),
      run: async () => {
        const id = targetId()
        if (id == null) return
        // 有未保存修改先问：静默重载会把用户刚写的東西丢掉。
        const tab = editor.tabs.find((t) => t.id === id)
        if (tab?.dirty) {
          const ok = await confirmText({
            title: t('menu.reload'),
            message: t('dialog.reloadBody', { name: tab.title || baseName(tab.path) }),
          })
          if (!ok) return
        }
        await editor.reload(id)
        // 通知编辑器组件替换正文：store 的 reload 只更新元数据。
        document.dispatchEvent(
          new CustomEvent('nt-reload-doc', { detail: { tabId: id } }),
        )
      },
    },
    {
      id: 'file.drafts',
      group: 'file',
      labelKey: 'menu.drafts',
      run: () => dialogs.open('drafts'),
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
      id: 'edit.gotoBracket',
      group: 'edit',
      labelKey: 'menu.gotoBracket',
      // Ctrl+Shift+\ ：Ctrl+] 已被「增加缩进」占用（那也是 CodeMirror 的
      // 默认键位），两者不能重合，否则菜单显示一个、按了跑另一个。
      key: 'Control+Shift+\\',
      enabled: () => hasTab(),
      run: () => {
        // 光标不在括号上要给提示：静默什么都不做，用户会以为菜单坏了。
        if (!doGotoBracket(targetId())) toast.warn(t('toast.noBracketAtCursor'))
      },
    },
    {
      id: 'edit.prevPosition',
      group: 'edit',
      labelKey: 'menu.prevPosition',
      key: 'Alt+Left',
      enabled: () => hasTab(),
      run: () => {
        const id = targetId()
        if (id != null && !goBack(id)) toast.warn(t('toast.noPrevPosition'))
      },
    },
    {
      id: 'edit.nextPosition',
      group: 'edit',
      labelKey: 'menu.nextPosition',
      key: 'Alt+Right',
      enabled: () => hasTab(),
      run: () => {
        const id = targetId()
        if (id != null && !goForward(id)) toast.warn(t('toast.noNextPosition'))
      },
    },
    {
      id: 'bookmark.clearAll',
      group: 'edit',
      labelKey: 'menu.clearBookmarks',
      enabled: () => hasFile(),
      run: async () => {
        const tab = editor.active
        if (!tab?.path) return
        const ok = await confirmText({
          title: t('menu.clearBookmarks'),
          message: t('dialog.clearBookmarksBody'),
        })
        if (!ok) return
        const n = await api.clearBookmarks()
        document.dispatchEvent(new CustomEvent('bookmarks-changed'))
        toast.ok(t('toast.bookmarksCleared', { n }))
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
      group: 'search',
      labelKey: 'menu.find',
      key: 'Control+F',
      run: () => dialogs.open('find', selectionSeed()),
    },
    {
      id: 'edit.replace',
      group: 'search',
      labelKey: 'menu.replace',
      key: 'Control+H',
      run: () => dialogs.open('replace', selectionSeed()),
    },
    // F3 / Shift+F3：沿用上次的查询词直接跳下一个 / 上一个命中。
    // 关掉查找浮窗后照样能用——这是主流编辑器里最常按的两个键。
    {
      id: 'edit.findNext',
      group: 'search',
      labelKey: 'menu.findNext',
      key: 'F3',
      enabled: () => hasTab(),
      run: () => stepLastQuery(1),
    },
    {
      id: 'edit.findPrev',
      group: 'search',
      labelKey: 'menu.findPrev',
      key: 'Shift+F3',
      enabled: () => hasTab(),
      run: () => stepLastQuery(-1),
    },
    {
      id: 'edit.findInFiles',
      group: 'search',
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
      // 状态栏的语言读数：两档循环切换。与 view.theme 同构——
      // 没有这个 id 时状态栏的语言按钮是死的（点了没反应）。
      id: 'view.language',
      group: 'view',
      labelKey: 'menu.uiLanguage',
      run: () => {
        const next = settings.config.language === 'zh-CN' ? 'en' : 'zh-CN'
        settings.update({ language: next })
        setLocale(next)
        // 注意新语言下的文案：语言切换后 t() 已经按新语言取值。
        toast.ok(t(next === 'zh-CN' ? 'menu.langZh' : 'menu.langEn'))
      },
    },
    {
      // 编辑器语法语言：切当前标签的高亮与注释符。
      // 与 view.language（界面语言）是两件事，id 与分组都分开。
      id: 'lang.setSyntax',
      group: 'language',
      labelKey: 'menu.syntaxLanguage',
      enabled: () => hasTab(),
      run: async () => {
        const tab = editor.active
        if (!tab) return
        const list = await api.languages()
        const picked = await pickFromList({
          title: t('menu.syntaxLanguage'),
          items: list.map((l) => ({ value: l.id, label: l.label })),
          current: tab.language || 'text',
        })
        if (!picked || picked === tab.language) return
        tab.language = picked
        // 把显示名而不是内部 id 报到 toast：用户不认识「jsonc」这种内部键。
        const name = list.find((l) => l.id === picked)?.label ?? picked
        toast.ok(t('toast.languageSet', { name }))
      },
    },
    {
      // 按扩展名重新猜一遍语法语言：从别的编辑器复制来的文件常带错扩展名。
      id: 'lang.detect',
      group: 'language',
      labelKey: 'menu.detectLanguage',
      enabled: () => hasFile(),
      run: async () => {
        const tab = editor.active
        if (!tab?.path) return
        const list = await api.languages()
        const hit = list.find((l) => l.exts.includes(extName(tab.path)))
        if (!hit) {
          toast.warn(t('toast.languageUnknown'))
          return
        }
        tab.language = hit.id
        toast.ok(t('toast.languageSet', { name: hit.label }))
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
      group: 'encoding',
      labelKey: 'menu.encoding',
      enabled: () => hasTab(),
      run: () => dialogs.open('encoding'),
    },
    {
      id: 'view.lineEnding',
      group: 'encoding',
      labelKey: 'menu.lineEnding',
      enabled: () => hasTab(),
      run: () => dialogs.open('encoding'),
    },
    {
      id: 'tools.encoding',
      group: 'encoding',
      labelKey: 'tool.encoding',
      key: 'Control+Shift+E',
      run: () => dialogs.open('encoding'),
    },
    // 一键转码：走浮窗要点四下，这里直接按新编码写盘。
    // 编辑器里始终是解码后的 Unicode，所以不必重建文档，只改 tab 的写盘参数。
    {
      id: 'encoding.toUtf8',
      group: 'encoding',
      labelKey: 'encoding.toUtf8',
      enabled: () => hasFile(),
      run: async () => {
        const tab = editor.active
        if (!tab?.path) return
        try {
          await api.writeFile(tab.path, getText(tab.id), 'utf-8', tab.line_ending, false)
          tab.encoding = 'utf-8'
          tab.has_bom = false
          commitBaseline(tab.id)
          editor.markDirty(tab.id, false)
          toast.ok(t('toast.encodingChanged', { encoding: 'UTF-8' }))
        } catch (e) {
          toast.bad(t('error.encodeFailed', { reason: normalizeError(e).message }))
        }
      },
    },
    {
      id: 'encoding.toUtf8Bom',
      group: 'encoding',
      labelKey: 'encoding.toUtf8Bom',
      enabled: () => hasFile(),
      run: async () => {
        const tab = editor.active
        if (!tab?.path) return
        try {
          await api.writeFile(tab.path, getText(tab.id), 'utf-8', tab.line_ending, true)
          tab.encoding = 'utf-8'
          tab.has_bom = true
          commitBaseline(tab.id)
          editor.markDirty(tab.id, false)
          toast.ok(t('toast.encodingChanged', { encoding: 'UTF-8 BOM' }))
        } catch (e) {
          toast.bad(t('error.encodeFailed', { reason: normalizeError(e).message }))
        }
      },
    },
    {
      id: 'encoding.reopen',
      group: 'encoding',
      labelKey: 'encoding.reopenWith',
      enabled: () => hasFile(),
      run: async () => {
        const tab = editor.active
        if (!tab?.path) return
        const list = await api.encodings()
        const picked = await pickFromList({
          title: t('encoding.reopenWith'),
          items: list.map((e) => ({ value: e.id, label: encodingLabel(e.id) })),
          current: tab.encoding,
        })
        if (!picked || picked === tab.encoding) return
        await reopenWithEncoding(tab.id, picked)
      },
    },

    // ---- 工具 ----
    {
      id: 'tools.json',
      group: 'tools',
      labelKey: 'tool.json',
      run: () => dialogs.open('json', selectionSeed()),
    },
    // 一键快操作：选中即用，结果原地替换，不经浮窗。
    // Ctrl+Shift+J 给最高频的「格式化」，浮窗（JSONPath / 转结构体）从菜单进。
    {
      id: 'quick.jsonFormat',
      group: 'tools',
      labelKey: 'quick.jsonFormat',
      key: 'Control+Shift+J',
      enabled: () => hasTab(),
      run: () => void quickJsonFormat(targetId()),
    },
    {
      id: 'quick.jsonMinify',
      group: 'tools',
      labelKey: 'quick.jsonMinify',
      key: 'Control+Alt+J',
      enabled: () => hasTab(),
      run: () => void quickJsonMinify(targetId()),
    },
    {
      id: 'quick.escape',
      group: 'tools',
      labelKey: 'quick.escape',
      key: 'Control+Alt+E',
      enabled: () => hasTab(),
      run: () => quickEscape(targetId()),
    },
    {
      id: 'quick.unescape',
      group: 'tools',
      labelKey: 'quick.unescape',
      key: 'Control+Alt+U',
      enabled: () => hasTab(),
      run: () => quickUnescape(targetId()),
    },
    // 转换矩阵不占默认键位：右键菜单与选区浮条是一等入口。
    {
      id: 'quick.jsonToYaml',
      group: 'tools',
      labelKey: 'quick.jsonToYaml',
      enabled: () => hasTab(),
      run: () => void quickConvert(targetId(), 'json', 'yaml'),
    },
    {
      id: 'quick.jsonToToml',
      group: 'tools',
      labelKey: 'quick.jsonToToml',
      enabled: () => hasTab(),
      run: () => void quickConvert(targetId(), 'json', 'toml'),
    },
    {
      id: 'quick.jsonToXml',
      group: 'tools',
      labelKey: 'quick.jsonToXml',
      enabled: () => hasTab(),
      run: () => void quickConvert(targetId(), 'json', 'xml'),
    },
    {
      id: 'quick.toConvert',
      group: 'tools',
      labelKey: 'quick.toConvert',
      enabled: () => hasTab(),
      run: () => dialogs.open('convert', selectionSeed()),
    },
    {
      id: 'tools.diff',
      group: 'compare',
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
      group: 'compare',
      labelKey: 'dirDiff.title',
      run: () => dialogs.open('dirDiff'),
    },
    {
      // 与剪贴板对比：从别处复制了一段代码，想知道它和本地差在哪。
      // 走文本对比浮窗，左侧预填当前正文、右侧预填剪贴板。
      id: 'compare.clipboard',
      group: 'compare',
      labelKey: 'compare.clipboard',
      enabled: () => hasTab(),
      run: async () => {
        const clip = await readClipboard()
        if (!clip) {
          toast.warn(t('toast.clipboardEmpty'))
          return
        }
        const id = targetId()
        if (id == null) return
        dialogs.open('diff', getText(id), { right: clip })
      },
    },
    {
      // 在资源管理器里定位当前文件：改完配置想去看目录里还有什么。
      id: 'file.reveal',
      group: 'file',
      labelKey: 'menu.reveal',
      enabled: () => hasFile(),
      run: () => {
        const tab = editor.active
        if (tab?.path) void api.openPath(dirName(tab.path))
      },
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
        const id = targetId()
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
        const id = targetId()
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
          toast.warn(t('sidebar.noBookmarksInFile'))
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
