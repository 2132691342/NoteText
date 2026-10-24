// 命令 store：快捷键 / 菜单 / 工具栏的统一分发表。
// 为什么收敛到一处：三个入口（菜单、工具栏、按键）最终都该走到同一个
import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { LineOp } from '@/types/api'

/** 一个命令：id 全局唯一，菜单 / 工具栏 / 键位表都引用它。 */
export interface Command {
  id: string
  /** 菜单分组：同组的在菜单里用分隔线隔开。 */
  group: MenuGroup
  /** i18n key（menu.* / tool.*）。 */
  labelKey: string
  /** 默认组合键（Tauri 的 accelerator 格式）。空 = 无快捷键。 */
  key?: string
  run: () => void | Promise<void>
  /** 是否可执行（置灰判据）。 */
  enabled?: () => boolean
}

/**
 * 菜单分组的全集与顺序。
 *
 * 十个顶级菜单与主流编辑器对齐：文件 / 编辑 / 查找 / 视图 / 编码 /
 * 语言 / 设置 / 工具 / 对比 / 帮助。顺序即渲染顺序，改这里就是改菜单条。
 */
export const MENU_GROUPS = [
  'file',
  'edit',
  'search',
  'view',
  'encoding',
  'language',
  'settings',
  'tools',
  'compare',
  'help',
] as const

export type MenuGroup = (typeof MENU_GROUPS)[number]

/** 分组 → 顶级菜单的 i18n key 前缀。菜单条按这个取标题。
 *
 * 四个新菜单用 `*Menu` 后缀：`menu.encoding` / `menu.language` 已被
 * 「视图 → 编码」「视图 → 语言」两条命令占用，同名会撞。 */
export const MENU_LABEL_KEYS: Record<MenuGroup, string> = {
  file: 'menu.file',
  edit: 'menu.edit',
  search: 'menu.search',
  view: 'menu.view',
  encoding: 'menu.encodingMenu',
  language: 'menu.languageMenu',
  settings: 'menu.settings',
  tools: 'menu.tools',
  compare: 'menu.compareMenu',
  help: 'menu.help',
}

export const useCommandStore = defineStore('commands', () => {
  const registry = ref(new Map<string, Command>())

  function register(cmds: Command[]) {
    for (const c of cmds) registry.value.set(c.id, c)
    // 触发响应性：Map 的 set 不触发，替换成新 Map 才行。
    registry.value = new Map(registry.value)
  }

  function get(id: string): Command | undefined {
    return registry.value.get(id)
  }

  function run(id: string) {
    const c = registry.value.get(id)
    if (!c) return
    if (c.enabled && !c.enabled()) return
    void c.run()
  }

  /** 菜单渲染用的分组清单。 */
  function byGroup(group: Command['group']): Command[] {
    return [...registry.value.values()].filter((c) => c.group === group)
  }

  return { registry, register, get, run, byGroup }
})

/**
 * 默认键位表：命令表里声明的 `key` 的副本，供「设置 → 快捷键」展示与冲突检测。
 *
 * **真相源是命令自己声明的 `key`**（useCommands 注册时写的那一份）。
 * 这里保留一份是为了：① 设置页列出全部默认键位；② 用户改键后仍能显示
 * 「默认是什么」。两者必须一致——分发优先看命令的 key，所以这张表落后
 * 只会让设置页显示旧的默认值，不会让按键失灵。
 */
export const DEFAULT_KEYBINDINGS: Record<string, string> = {
  'file.new': 'Control+N',
  'file.open': 'Control+O',
  'file.openRecent': 'Control+Shift+O',
  'file.openFolder': 'Control+K',
  'file.save': 'Control+S',
  'file.saveAs': 'Control+Shift+S',
  'file.saveAll': 'Control+Alt+S',
  'file.closeTab': 'Control+W',
  'edit.undo': 'Control+Z',
  'edit.redo': 'Control+Y',
  'edit.cut': 'Control+X',
  'edit.copy': 'Control+C',
  'edit.paste': 'Control+V',
  'edit.selectAll': 'Control+A',
  'edit.moveLineUp': 'Alt+Up',
  'edit.moveLineDown': 'Alt+Down',
  'edit.indent': 'Control+]',
  'edit.outdent': 'Control+[',
  'edit.find': 'Control+F',
  'edit.replace': 'Control+H',
  'edit.findNext': 'F3',
  'edit.findPrev': 'Shift+F3',
  'edit.gotoBracket': 'Control+Shift+\\',
  'edit.prevPosition': 'Alt+Left',
  'edit.nextPosition': 'Alt+Right',
  'edit.findInFiles': 'Control+Shift+F',
  'edit.gotoLine': 'Control+G',
  'edit.toggleComment': 'Control+/',
  'edit.duplicateLine': 'Control+D',
  'edit.deleteLine': 'Control+Shift+K',
  'edit.caseUpper': 'Control+Shift+U',
  'edit.caseLower': 'Control+Shift+G',
  'edit.columnMode': 'Alt+C',
  'view.zoomIn': 'Control+=',
  'view.zoomOut': 'Control+-',
  'view.resetZoom': 'Control+0',
  'view.fullscreen': 'F11',
  'view.toggleSidebar': 'Control+B',
  'view.toggleStatusbar': 'Control+Alt+B',
  'view.toggleWordWrap': 'Alt+Z',
  'view.splitView': 'Control+\\',
  'view.settings': 'Control+,',
  'file.print': 'Control+P',
  'quick.jsonFormat': 'Control+Shift+J',
  'quick.jsonMinify': 'Control+Alt+J',
  'quick.escape': 'Control+Alt+E',
  'quick.unescape': 'Control+Alt+U',
  'tools.diff': 'Control+Shift+D',
  'tools.hash': 'Control+Shift+H',
  'tools.regex': 'Control+Shift+R',
  'tools.encoding': 'Control+Shift+E',
  'tools.lineOp': 'Control+Shift+L',
  'tools.convert': 'Control+Shift+C',
  'tools.clipboard': 'Control+Shift+V',
  'tools.hex': 'Control+Alt+H',
  'tools.batchFindReplace': 'Control+Shift+B',
  'tools.batchRename': 'Control+Alt+R',
  'tools.archive': 'Control+Alt+A',
  'tools.archiveTasks': 'Control+Alt+T',
  'bookmark.toggle': 'F2',
  'bookmark.next': 'Shift+F2',
  'macro.record': 'Control+Shift+M',
  'macro.play': 'Control+Shift+P',
  'help.manual': 'F1',
}

/** 行编辑操作在工具箱里的顺序：与 settings.lineOp 面板一致。 */
export const LINE_OPS: Array<{ op: LineOp; labelKey: string }> = [
  { op: 'remove_blank', labelKey: 'tool.removeBlank' },
  { op: 'trim_trailing_ws', labelKey: 'tool.trimTrailing' },
  { op: 'to_upper', labelKey: 'tool.toUpper' },
  { op: 'to_lower', labelKey: 'tool.toLower' },
  { op: 'to_title', labelKey: 'tool.toTitle' },
  { op: 'invert_case', labelKey: 'tool.invertCase' },
  { op: 'squeeze_blank', labelKey: 'tool.squeezeBlank' },
  { op: 'reverse', labelKey: 'tool.reverseLines' },
  { op: 'dedup', labelKey: 'tool.dedupLines' },
  { op: 'sort_asc', labelKey: 'tool.sortAsc' },
  { op: 'sort_desc', labelKey: 'tool.sortDesc' },
]
