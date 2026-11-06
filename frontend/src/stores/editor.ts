// 编辑器 store：标签页生命周期、活跃标签、脏标记、光标位置。
// **正文内容归编辑器实例（CodeMirror）**，这里只存标签元数据：每次按键都过 IPC

import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { api, normalizeError } from '@/api'
import type {
  EditorPosition,
  FileContentVO,
  FileMeta,
  ReadMode,
  TabDO,
  ViewType,
} from '@/types/api'
import { useToastStore } from './toast'
import { useSettingsStore } from './settings'
import { tr } from '@/i18n'
import { baseName, formatSize } from '@/utils'
import { isTextKind, isTableGrid, isWorkbookSheet, kindOf } from '@/utils/fileKind'
import { askPick } from '@/components/common/askPick'
import { stagePendingContent } from '@/composables/useEditorRegistry'
import { SHAPE_PROBE_MIN, isWide, probeShape } from '@/utils/textShape'

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
  /** 预览开关：与后端持久化的 view_type 正交，不进会话契约。 */
  showing_preview: boolean
  /** 内容被截断（大文件只读了前段）：此时只读，禁止保存。 */
  truncated: boolean
  /** 截断时实际读入的字节数：状态栏据此提示「已读前 X MB」。 */
  truncated_bytes: number
  /** 分屏时次视图是否打开。 */
  split: boolean
  /** 载入时的探测结果：low_confidence 时状态栏要提示「不确定」。 */
  encoding_uncertain: boolean
  /**
   * 分块只读模式（超大文件）：正文按块从后端取，不整篇进内存。
   * 运行时标记，不进会话契约。
   */
  chunked: boolean
}

let nextId = 1

/**
 * 全量读入的提示阈值，与后端 `service::file::MAX_FULL_READ` 对齐。
 * 超过它弹窗让用户选普通打开还是分块打开；两边必须同值。
 */
const FULL_READ_LIMIT = 64 * 1024 * 1024

/** 全量读阈值的 MB 数值：弹窗文案里不写死数字。 */
export const FULL_READ_LIMIT_MB = Math.round(FULL_READ_LIMIT / 1024 / 1024)

/**
 * 显式放行后的全量可编辑上限，与后端 `service::file::MAX_FORCE_FULL_READ`
 * 对齐（512MB）。再大只能分块（可逐块编辑）。
 */
const BIG_EDIT_LIMIT = 512 * 1024 * 1024

/** 全量读挂「打开中」提示的阈值：几 MB 以内读得比提示本身还快。 */
const LOADING_HINT_LIMIT = 16 * 1024 * 1024

export const useEditorStore = defineStore('editor', () => {
  const tabs = ref<Tab[]>([])
  const activeId = ref<number | null>(null)
  /** 分屏比例（0–1）：副视图宽度占比。 */
  const splitRatio = ref(0.5)
  /** 本次运行是否已恢复过会话：路由离开编辑器视图再回来时不能重复灌标签。 */
  const sessionRestored = ref(false)

  const active = computed(() => tabs.value.find((t) => t.id === activeId.value) ?? null)
  const dirtyCount = computed(() => tabs.value.filter((t) => t.dirty).length)
  /** 正在全量读的文件路径：编辑区据此挂「打开中」提示，大文件读入不再像假死。 */
  const opening = ref<string | null>(null)

  /**
   * 新建空标签。`draftId` 非空表示从草稿恢复。
   * 编码与行尾取文件配置里的默认值，否则那三项设置永远是「改了也不生效」。
   */
  function newTab(opts?: { title?: string; draftId?: string }): Tab {
    const file = useSettingsStore().config.file
    const ending: Tab['line_ending'] =
      file.new_line_ending === 'system'
        ? navigator.userAgent.includes('Windows')
          ? 'crlf'
          : 'lf'
        : (file.new_line_ending as Tab['line_ending'])
    const tab: Tab = {
      id: nextId++,
      path: opts?.draftId ? `draft:${opts.draftId}` : '',
      title: opts?.title ?? '',
      language: 'text',
      encoding: file.default_encoding === 'auto' ? 'utf-8' : file.default_encoding,
      line_ending: ending,
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
      chunked: false,
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

  /**
   * 把标签从 `from` 挪到 `to`（两处都是当前数组下标）。
   * `toDOs` 按数组顺序写 `ord`，所以这里只动数组就够。
   */
  function moveTab(from: number, to: number) {
    if (from === to || from < 0 || to < 0) return
    if (from >= tabs.value.length || to >= tabs.value.length) return
    const [moved] = tabs.value.splice(from, 1)
    tabs.value.splice(to, 0, moved)
  }

  function setSplit(id: number, on: boolean) {
    const tab = tabs.value.find((t) => t.id === id)
    if (tab) tab.split = on
  }

  /** 节点在磁盘上换了路径；`path` 与 `title` 必须一起换。空串表示节点被删除，关掉标签。 */
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
   * 打开一个文件：已打开则聚焦；否则 stat → 按形态读内容或只建标签 → 建标签。
   * 非文本形态只建标签，正文由预览器按字节读。
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
      let meta: FileMeta | null = null
      try {
        meta = await api.statFile(path)
      } catch {
        /* stat 失败就按全量读试一次，真正的错误由 readFile 报 */
      }

      const kind = kindOf(path, meta?.is_binary ?? false)
      // CSV / TSV / JSONL 走可编辑表格网格：大文件也进（后端按记录边界建索引 + 分页取数）。
      // 旧的「≤64MB 才可编辑」限制会让百万行的表只能看前 2000 行。
      const editableSheet = kind === 'sheet' && isTableGrid(path)
      if (editableSheet) {
        // 表格网格自己按页取数，不走全量读：正文不进内存。
        // 编码给 'auto'：后端按文件头探测，预设 'utf-8' 会让 GB18030 的文件整表乱码。
        const tab = newTab()
        tab.path = path
        tab.title = fileNameOf(path)
        tab.language = languageOf(path)
        tab.encoding = 'auto'
        tab.view_type = 'code'
        tab.showing_preview = false
        tab.read_only = false
        return tab
      }
      if (!isTextKind(kind)) {
        const tab = newTab()
        tab.path = path
        tab.title = fileNameOf(path)
        tab.language = languageOf(path)
        // 二进制进 Hex，能预览的进预览器（DocPreview 自己按扩展名分流）
        tab.view_type = kind === 'binary' ? 'hex' : viewTypeOf(path)
        // XLSX / XLSM 默认进可编辑网格（单元格值与公式可改，样式原地保留）；
        // .xls / .ods 走只读预览（后端不具备它们的写回能力）。
        const editableWorkbook = kind === 'sheet' && isWorkbookSheet(path)
        tab.showing_preview = kind !== 'binary' && !editableWorkbook
        tab.read_only = !editableWorkbook
        return tab
      }

      let effective = mode
      let forceBig = false
      // 宽行探测：CodeMirror 对几 MB 的单行会冻住主线程，开之前先看一眼头部。
      // 用户显式给了读取模式就不再探测——那是一次明确的意图。
      let wide = false
      if (!effective && (meta === null || meta.size > SHAPE_PROBE_MIN)) {
        wide = isWide(await probeShape(path))
      }
      if (!effective && meta && meta.size > FULL_READ_LIMIT) {
        if (meta.size > BIG_EDIT_LIMIT) {
          // 超过显式放行上限：不再询问，直接进分块（秒开，可逐块编辑）。
          return await openChunked(path)
        }
        // 64MB ~ 512MB：弹窗让用户选——普通打开可编辑但慢，分块秒开。
        const choice = await askPick({
          title: tr('dialog.bigFile.title', {
            name: fileNameOf(path),
            size: formatSize(meta.size),
          }),
          items: [
            {
              value: 'normal',
              label: tr('dialog.bigFile.normal'),
              hint: tr(wide ? 'dialog.bigFile.normalWideHint' : 'dialog.bigFile.normalHint'),
            },
            {
              value: 'chunked',
              label: tr('dialog.bigFile.chunked'),
              hint: tr('dialog.bigFile.chunkedHint'),
            },
          ],
        })
        if (choice === 'chunked') return await openChunked(path)
        if (choice !== 'normal') return null // 用户取消：不开标签
        effective = { mode: 'full' }
        forceBig = true
      } else if (!effective && wide) {
        // 不足全量读阈值但含超宽行：同样给一次选择，普通打开可能卡顿。
        const choice = await askPick({
          title: tr('dialog.wideLine.title', { name: fileNameOf(path) }),
          items: [
            {
              value: 'chunked',
              label: tr('dialog.bigFile.chunked'),
              hint: tr('dialog.wideLine.chunkedHint'),
            },
            {
              value: 'normal',
              label: tr('dialog.bigFile.normal'),
              hint: tr('dialog.wideLine.normalHint'),
            },
          ],
        })
        if (choice === 'chunked') return await openChunked(path)
        if (choice !== 'normal') return null
      }
      // 大文件全量读有秒级耗时：挂「打开中」提示，别让用户以为界面死了。
      const hintLoading = (meta?.size ?? 0) > LOADING_HINT_LIMIT
      if (hintLoading) opening.value = path
      let content: FileContentVO
      try {
        content = await api.readFile(path, undefined, effective, forceBig)
      } catch (e) {
        const err = normalizeError(e)
        if (err.code === '1004') {
          // stat 失败或建标签竞态导致没走到分流的：这里兜底转分块，不弹错误。
          return await openChunked(path)
        }
        throw e
      } finally {
        if (hintLoading) opening.value = null
      }
      const tab = newTab()
      // 正文暂存：编辑器挂载时直接消费，不再重读一遍（截断标签尤其重要，
      // 挂载时不带 mode 的重读会被后端拒绝，用户看到错误面）。
      stagePendingContent(tab.id, content.text)
      applyContent(tab, content)
      // 编码置信度低也置只读以外不做处理：真正的截断只有显式 head 模式才有。
      if (content.truncated) {
        tab.read_only = true
        tab.truncated = true
        tab.truncated_bytes = content.read_bytes
      }
      return tab
    } catch (e) {
      const err = normalizeError(e)
      useToastStore().bad(tr('toast.openFailed', { reason: err.message }))
      return null
    }
  }

  /** 以分块模式打开超大文件（索引建好后可逐块编辑）。 */
  async function openChunked(path: string): Promise<Tab | null> {
    const tab = newTab()
    tab.path = path
    tab.title = fileNameOf(path)
    tab.language = languageOf(path)
    tab.encoding = 'utf-8'
    tab.read_only = true
    tab.chunked = true
    tab.view_type = viewTypeOf(path)
    return tab
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
    // 分块只读标签不做全量重读：读块时后端按修改时间报 stale，够用。
    if (tab.chunked) return
    try {
      // 截断标签按原样读前段：全量读会在上限之外的文件上直接报错，重载永远失败。
      const c = await api.readFile(tab.path, undefined, tab.truncated ? { mode: 'head' } : undefined)
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

  /** 从后端会话整块替换标签并恢复 activeIndex；正文由组件层按 path / draft 拉。 */
  function restore(dos: TabDO[], activeIndex = 0) {
    tabs.value = []
    activeId.value = null
    sessionRestored.value = true
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
        chunked: false,
      }
      tabs.value.push(tab)
    }
    if (tabs.value.length > 0) {
      const i = Math.min(Math.max(activeIndex, 0), tabs.value.length - 1)
      activeId.value = tabs.value[i].id
    }
    // 恢复出来的大标签先判一次大小：几十 MB 的文件在会话恢复时被整篇读进
    // 内存再报「太大」，用户看到的是启动后长时间无响应。这里直接转分块。
    void markOversizedAsChunked()
  }

  /**
   * 会话恢复后把超大或含超宽行的标签改成分块模式。
   * 只对文本形态的标签做：二进制与表格各有自己的打开路径。
   */
  async function markOversizedAsChunked() {
    const candidates = tabs.value.filter(
      (t) =>
        t.path &&
        !t.path.startsWith('draft:') &&
        !t.chunked &&
        isTextKind(kindOf(t.path)),
    )
    if (candidates.length === 0) return
    await Promise.all(
      candidates.map(async (tab) => {
        try {
          const meta = await api.statFile(tab.path)
          if (meta.size > FULL_READ_LIMIT) {
            tab.chunked = true
            tab.read_only = true
            tab.truncated = false
            return
          }
          // 尺寸不大但含超宽行的文件同样进分块：全量读进 CodeMirror 会冻结。
          if (meta.size > SHAPE_PROBE_MIN && isWide(await probeShape(tab.path))) {
            tab.chunked = true
            tab.read_only = true
          }
        } catch {
          /* stat 失败：文件可能已不在，保持原样让组件层给错误面 */
        }
      }),
    )
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
    opening,
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
    moveTab,
    renamePath,
    openPath,
    reload,
    applyContent,
    restore,
    sessionRestored,
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
