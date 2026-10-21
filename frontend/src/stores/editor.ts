// 编辑器 store：标签页生命周期、活跃标签、脏标记、光标位置。
//
// 归属（AGENTS.md §2.3.3）：**正文内容归编辑器实例（CodeMirror）**，
// 这里只存「标签的元数据 + 一个内容快照引用」。理由：每次按键都过 IPC
// 会把主线程与 WebView 之间的通道占满，而保存 / 恢复只需要路径与光标。
//
// 因此每个 tab 持有一个 `docRef`（CodeMirror 的 EditorState 引用由
// 组件层管理），store 不复制正文。需要正文的地方（保存、统计）由组件层
// 调 `getText(tabId)`，见 composables/useEditorRegistry.ts。
import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { api, normalizeError } from '@/api'
import type {
  EditorPosition,
  FileContentVO,
  ReadMode,
  TabDO,
  ViewType,
} from '@/types/api'
import { useToastStore } from './toast'

/** 一个标签页。`path` 为空 = 新建未保存的草稿标签。 */
export interface Tab {
  id: number
  path: string
  title: string
  language: string
  encoding: string
  line_ending: 'crlf' | 'lf' | 'cr'
  has_bom: boolean
  dirty: boolean
  read_only: boolean
  position: EditorPosition
  scroll_ratio: number
  view_type: ViewType
  /** 分屏时次视图是否打开。 */
  split: boolean
  /** 载入时的探测结果：low_confidence 时状态栏要提示「不确定」。 */
  encoding_uncertain: boolean
}

let nextId = 1

export const useEditorStore = defineStore('editor', () => {
  const tabs = ref<Tab[]>([])
  const activeId = ref<number | null>(null)
  /** 分屏比例（0–1）：副视图宽度占比。 */
  const splitRatio = ref(0.5)

  const active = computed(() => tabs.value.find((t) => t.id === activeId.value) ?? null)
  const dirtyCount = computed(() => tabs.value.filter((t) => t.dirty).length)

  /** 新建空标签。`draftId` 非空表示从草稿恢复。 */
  function newTab(opts?: { title?: string; draftId?: string }): Tab {
    const tab: Tab = {
      id: nextId++,
      path: opts?.draftId ? `draft:${opts.draftId}` : '',
      title: opts?.title ?? '',
      language: 'text',
      encoding: 'utf-8',
      line_ending: 'crlf',
      has_bom: false,
      dirty: false,
      read_only: false,
      position: { line: 1, col: 1, selection_len: 0 },
      scroll_ratio: 0,
      view_type: 'code',
      split: false,
      encoding_uncertain: false,
    }
    tabs.value.push(tab)
    activeId.value = tab.id
    return tab
  }

  /** 按路径找已打开的标签：双击文件树里已打开的文件应聚焦而不是开第二个。 */
  function findByPath(path: string): Tab | undefined {
    return tabs.value.find((t) => t.path === path)
  }

  function activate(id: number) {
    if (tabs.value.some((t) => t.id === id)) activeId.value = id
  }

  /** 关闭标签。返回 false = 被脏标记拦下（调用方应弹确认）。 */
  function close(id: number, force = false): boolean {
    const tab = tabs.value.find((t) => t.id === id)
    if (!tab) return true
    if (tab.dirty && !force) return false
    const i = tabs.value.findIndex((t) => t.id === id)
    tabs.value.splice(i, 1)
    if (activeId.value === id) {
      // 关的是活跃标签：激活左邻，没有则右邻。
      const next = tabs.value[Math.min(i, tabs.value.length - 1)]
      activeId.value = next?.id ?? null
    }
    return true
  }

  /** 强制关闭全部（退出确认后调）。 */
  function closeAll() {
    tabs.value = []
    activeId.value = null
  }

  function markDirty(id: number, dirty: boolean) {
    const tab = tabs.value.find((t) => t.id === id)
    if (tab) tab.dirty = dirty
  }

  function setPosition(id: number, pos: EditorPosition) {
    const tab = tabs.value.find((t) => t.id === id)
    if (tab) tab.position = pos
  }

  function setScroll(id: number, ratio: number) {
    const tab = tabs.value.find((t) => t.id === id)
    if (tab) tab.scroll_ratio = ratio
  }

  function setSplit(id: number, on: boolean) {
    const tab = tabs.value.find((t) => t.id === id)
    if (tab) tab.split = on
  }

  /**
   * 打开一个文件。
   *
   * 已打开则聚焦；否则读文件 → 建标签。读失败只报错不建标签——
   * 建一个空标签会让用户以为「打开了但内容是空的」。
   */
  async function openPath(path: string, mode?: ReadMode): Promise<Tab | null> {
    const existing = findByPath(path)
    if (existing) {
      activate(existing.id)
      // 已在别处打开时也刷一次外部变更提示：文件可能刚被改过。
      await reload(existing.id)
      return existing
    }
    try {
      const content = await api.readFile(path, mode)
      const tab = newTab()
      applyContent(tab, content)
      return tab
    } catch (e) {
      const err = normalizeError(e)
      useToastStore().bad(`打开失败：${err.message}`)
      return null
    }
  }

  /** 把读取结果贴到标签上（新建与重载共用）。 */
  function applyContent(tab: Tab, c: FileContentVO) {
    tab.path = c.path
    tab.title = fileNameOf(c.path)
    tab.language = languageOf(c.path)
    tab.encoding = c.encoding
    tab.line_ending = c.line_ending
    tab.has_bom = c.has_bom
    tab.read_only = false
    tab.encoding_uncertain = c.truncated && c.encoding !== 'utf-8'
    tab.view_type = viewTypeOf(c.path)
    tab.dirty = false
  }

  /** 重新载入（外部变更时）。保留光标与滚动。 */
  async function reload(id: number): Promise<void> {
    const tab = tabs.value.find((t) => t.id === id)
    if (!tab || !tab.path || tab.path.startsWith('draft:')) return
    try {
      const c = await api.readFile(tab.path)
      // 只更新元数据，正文由组件层按 fs:changed 事件决定是否替换
      // （用户正在编辑时不强制覆盖，见 composables/useExternalChange.ts）。
      tab.encoding = c.encoding
      tab.line_ending = c.line_ending
      tab.has_bom = c.has_bom
      tab.encoding_uncertain = c.truncated && c.encoding !== 'utf-8'
    } catch (e) {
      useToastStore().bad(`重新载入失败：${normalizeError(e).message}`)
    }
  }

  /** 关掉所有脏标签之外的全部标签。 */
  function closeOthers(id: number) {
    tabs.value = tabs.value.filter((t) => t.id === id || !t.dirty)
    if (!tabs.value.some((t) => t.id === activeId.value)) activeId.value = id
  }

  /** 关掉全部已保存标签。 */
  function closeSaved() {
    tabs.value = tabs.value.filter((t) => t.dirty)
    if (!tabs.value.some((t) => t.id === activeId.value)) {
      activeId.value = tabs.value[0]?.id ?? null
    }
  }

  /** 从后端会话恢复标签。正文由组件层按 path / draft 拉。 */
  function restore(dos: TabDO[]) {
    for (const d of dos) {
      const tab: Tab = {
        id: nextId++,
        path: d.path,
        title: d.title,
        language: d.language,
        encoding: d.encoding,
        line_ending: (d.line_ending as Tab['line_ending']) ?? 'crlf',
        has_bom: d.has_bom,
        // 恢复出来的标签一律按「未修改」处理：上次若没保存，
        // 用户下次启动看到的是磁盘上的版本，标脏会误导。
        dirty: false,
        read_only: false,
        position: {
          line: d.cursor_line,
          col: d.cursor_col,
          selection_len: 0,
        },
        scroll_ratio: d.scroll_ratio,
        view_type: (d.view_type as ViewType) ?? 'code',
        split: d.split,
        encoding_uncertain: false,
      }
      tabs.value.push(tab)
    }
  }

  /** 导出成后端 TabDO：保存会话时用。 */
  function toDOs(): TabDO[] {
    return tabs.value.map((t, i) => ({
      id: t.id,
      ord: i,
      path: t.path,
      title: t.title,
      language: t.language,
      encoding: t.encoding,
      line_ending: t.line_ending,
      has_bom: t.has_bom,
      cursor_line: t.position.line,
      cursor_col: t.position.col,
      scroll_ratio: t.scroll_ratio,
      dirty: t.dirty,
      split: t.split,
      view_type: t.view_type,
      updated_at: 0,
    }))
  }

  return {
    tabs,
    activeId,
    active,
    dirtyCount,
    splitRatio,
    newTab,
    findByPath,
    activate,
    close,
    closeAll,
    closeOthers,
    closeSaved,
    markDirty,
    setPosition,
    setScroll,
    setSplit,
    openPath,
    reload,
    restore,
    toDOs,
  }
})

export function fileNameOf(path: string): string {
  if (!path) return ''
  const parts = path.replace(/\//g, '\\').split('\\')
  return parts[parts.length - 1] || path
}

export function extOf(path: string): string {
  const name = fileNameOf(path)
  const i = name.lastIndexOf('.')
  return i > 0 ? name.slice(i + 1).toLowerCase() : ''
}

/** 与后端 `service::editor::language_by_ext` 同构：不一致时恢复出的标签
 *  会用错高亮。后端清单见 commands/editor::get_languages。 */
const EXT_LANG: Record<string, string> = {
  txt: 'text',
  md: 'markdown',
  markdown: 'markdown',
  json: 'json',
  yml: 'yaml',
  yaml: 'yaml',
  toml: 'toml',
  ini: 'ini',
  cfg: 'ini',
  conf: 'ini',
  xml: 'xml',
  svg: 'xml',
  html: 'html',
  htm: 'html',
  css: 'css',
  scss: 'scss',
  sass: 'scss',
  js: 'javascript',
  mjs: 'javascript',
  cjs: 'javascript',
  jsx: 'jsx',
  ts: 'typescript',
  tsx: 'typescript',
  vue: 'vue',
  rs: 'rust',
  go: 'go',
  java: 'java',
  kt: 'kotlin',
  kts: 'kotlin',
  c: 'c',
  h: 'c',
  cpp: 'cpp',
  cc: 'cpp',
  cxx: 'cpp',
  hpp: 'cpp',
  cs: 'csharp',
  py: 'python',
  rb: 'ruby',
  php: 'php',
  sh: 'shell',
  bash: 'shell',
  ps1: 'powershell',
  bat: 'bat',
  cmd: 'bat',
  sql: 'sql',
  lua: 'lua',
  diff: 'diff',
  patch: 'diff',
  log: 'log',
  csv: 'csv',
  tsv: 'csv',
}

export function languageOf(path: string): string {
  const ext = extOf(path)
  if (!ext) return 'text'
  return EXT_LANG[ext] ?? 'text'
}

/** 与后端 `domain::editor::ViewType::from_ext` 同构。 */
export function viewTypeOf(path: string): ViewType {
  switch (extOf(path)) {
    case 'md':
    case 'markdown':
      return 'markdown'
    case 'docx':
    case 'doc':
    case 'rtf':
      return 'richtext'
    case 'png':
    case 'jpg':
    case 'jpeg':
    case 'gif':
    case 'bmp':
    case 'webp':
    case 'svg':
    case 'ico':
      return 'image'
    case 'log':
      return 'log'
    default:
      return 'code'
  }
}
