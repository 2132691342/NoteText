// 编辑器 store：标签页生命周期、活跃标签、脏标记、光标位置。
//
// **正文内容归编辑器实例（CodeMirror）**，这里只存标签元数据：每次按键都过 IPC
// 会占满主线程与 WebView 之间的通道，而保存 / 恢复只需要路径与光标。

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
import { tr } from '@/i18n'
import { baseName } from '@/utils'

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
  /**
   * 预览开关：与 view_type 正交。
   *
   * 为什么不给 ViewType 加一档 `preview`：view_type 是**后端持久化的契约**
   * （`domain::editor::ViewType`，按扩展名推断、写进会话表）。预览只是
   * 「同一个文件换个看法」，不改变它的类型，也不必存进会话。
   */
  showing_preview: boolean
  /** 内容被截断（大文件只读了前段）：此时只读，禁止保存。 */
  truncated: boolean
  /** 截断时实际读入的字节数：状态栏据此提示「已读前 X MB」。 */
  truncated_bytes: number
  /** 分屏时次视图是否打开。 */
  split: boolean
  /** 载入时的探测结果：low_confidence 时状态栏要提示「不确定」。 */
  encoding_uncertain: boolean
}

let nextId = 1

/**
 * 全量读入的大小上限，与后端 `service::file::MAX_FULL_READ` 对齐（32MB）。
 * 超过它改读前段并只读——把几百 MB 的日志整篇塞进 CodeMirror 会直接卡死界面。
 * 两边必须同值：只改一边会出现「前端以为能全读、后端拒绕」。
 */
const FULL_READ_LIMIT = 32 * 1024 * 1024

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
      showing_preview: false,
      truncated: false,
      truncated_bytes: 0,
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
   * 节点在磁盘上换了路径（重命名 / 移动 / 删除）。
   *
   * `path` 与 `title` 必须一起换：只换 path 的后果是标签还显示旧名字，
   * 用户以为改的是另一个文件。`newPath` 为空串表示节点被删除：关掉标签。
   * 文件树的脏标记由调用方（树面板）自己同步，store 之间不互相引用。
   */
  function renamePath(oldPath: string, newPath: string) {
    if (!oldPath) return
    const tab = findByPath(oldPath)
    if (!tab) return
    if (!newPath) {
      close(tab.id, true)
      return
    }
    tab.path = newPath
    tab.title = baseName(newPath)
  }

  /**
   * 打开一个文件。
   *
   * 已打开则聚焦；否则读文件 → 建标签。读失败只报错不建标签——
   * 建一个空标签会让用户以为「打开了但内容是空的」。
   *
   * 大文件自动降级：先 stat 拿大小，超过后端全量读上限就改读前段，
   * 并标记只读。之前没有任何调用方传 mode，超 32MB 的文件直接报错打不开。
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
      let effective = mode
      let truncatedBySize = false
      if (!effective) {
        try {
          const meta = await api.statFile(path)
          if (meta.size > FULL_READ_LIMIT) {
            effective = { mode: 'head' }
            truncatedBySize = true
          }
        } catch {
          /* stat 失败就按全量读试一次，真正的错误由 readFile 报 */
        }
      }
      const content = await api.readFile(path, effective)
      const tab = newTab()
      applyContent(tab, content)
      // 截断的标签一律只读：保存会把截断后的内容写回磁盘，那是数据丢失。
      if (content.truncated) {
        tab.read_only = true
        tab.truncated = true
        if (truncatedBySize) tab.truncated_bytes = content.read_bytes
      }
      return tab
    } catch (e) {
      const err = normalizeError(e)
      useToastStore().bad(tr('toast.openFailed', { reason: err.message }))
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
      useToastStore().bad(tr('toast.reloadFailed', { reason: normalizeError(e).message }))
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

  /** 关掉 id 左侧的标签。脏标签跳过——不替用户丢未保存的修改。 */
  function closeLeft(id: number) {
    const i = tabs.value.findIndex((t) => t.id === id)
    if (i < 0) return
    tabs.value = tabs.value.filter((t, idx) => idx >= i || t.dirty)
    if (!tabs.value.some((t) => t.id === activeId.value)) activeId.value = id
  }

  /** 关掉 id 右侧的标签。脏标签跳过。 */
  function closeRight(id: number) {
    const i = tabs.value.findIndex((t) => t.id === id)
    if (i < 0) return
    tabs.value = tabs.value.filter((t, idx) => idx <= i || t.dirty)
    if (!tabs.value.some((t) => t.id === activeId.value)) activeId.value = id
  }

  /**
   * 从后端会话恢复标签。正文由组件层按 path / draft 拉。
   *
   * `activeIndex` 必须用上：只恢复标签不恢复活跃项，启动后一个标签都不显示，
   * 用户看到的是欢迎页——「上次的文件全没了」这种报告就是这么来的。
   */
  function restore(dos: TabDO[], activeIndex = 0) {
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
        showing_preview: false,
        truncated: false,
        truncated_bytes: 0,
        split: d.split,
        encoding_uncertain: false,
      }
      tabs.value.push(tab)
    }
    if (tabs.value.length > 0) {
      const i = Math.min(Math.max(activeIndex, 0), tabs.value.length - 1)
      activeId.value = tabs.value[i].id
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
    closeLeft,
    closeRight,
    markDirty,
    setPosition,
    setScroll,
    setSplit,
    renamePath,
    openPath,
    reload,
    applyContent,
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
