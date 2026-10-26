// 全局按键：把组合键映射到命令 id。
// 三入口收敛（AGENTS.md §2.10）：菜单、工具栏、按键都走命令表，
import { onMounted, onUnmounted } from 'vue'
import { DEFAULT_KEYBINDINGS, useCommandStore } from '@/stores/commands'
import { useSettingsStore, ZOOM_MAX, ZOOM_MIN } from '@/stores/settings'

/** 键盘事件 → Tauri accelerator 字符串（`Control+Shift+F` 形态）。 */
export function toAccelerator(e: KeyboardEvent): string {
  const parts: string[] = []
  if (e.ctrlKey) parts.push('Control')
  if (e.altKey) parts.push('Alt')
  if (e.shiftKey) parts.push('Shift')
  if (e.metaKey) parts.push('Super')

  // 用 e.code 而不是 e.key：同样按 `/`，不同键盘布局 e.key 可能不同，
  // 而 e.code 的 `Slash` 是物理位置，与默认键位表的定义一致。
  const codeMap: Record<string, string> = {
    Slash: '/',
    Equal: '=',
    Minus: '-',
    Backslash: '\\',
    Comma: ',',
    Period: '.',
    BracketLeft: '[',
    BracketRight: ']',
    Semicolon: ';',
    Quote: "'",
    Backquote: '`',
    Space: 'Space',
    Enter: 'Enter',
    Escape: 'Esc',
    Tab: 'Tab',
    ArrowUp: 'Up',
    ArrowDown: 'Down',
    ArrowLeft: 'Left',
    ArrowRight: 'Right',
    Delete: 'Delete',
    Backspace: 'Backspace',
    Home: 'Home',
    End: 'End',
    PageUp: 'PageUp',
    PageDown: 'PageDown',
    NumpadAdd: '+',
    NumpadSubtract: '-',
  }
  // 字母键取 e.code 的 `KeyX` → `X`；命令表的 `key` 按这个拼法写，两边必须同形。
  // 不用 e.key：按住 Shift 或输入法开时它会变成不可打印字符，匹配就断了。
  const fromCode = codeMap[e.code] ?? (/^Key[A-Z]$/.test(e.code) ? e.code.slice(3) : null)
  const key = fromCode ?? (e.key.length === 1 ? e.key.toUpperCase() : e.key)
  parts.push(key)
  return parts.join('+')
}

/** 焦点是否在原生输入控件里（EP 输入框 / textarea / select）。 */
function isNativeInput(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null
  if (!el) return false
  const tag = el.tagName
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || el.isContentEditable
}

/** 焦点是否在 CodeMirror 里。 */
function isInEditor(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null
  return !!el?.closest?.('.cm-editor')
}

/**
 * 焦点是否在只读文本面上（Markdown 预览 / 文档预览 / 十六进制）。
 *
 * 这些地方没有编辑器实例，用户选中的是页面里的真实文本。复制必须走浏览器
 * 原生路径：否则命令会去操作「当前激活的编辑器视图」，把用户选中的预览文本
 * 换成编辑器里的选区（通常是空的），Ctrl+C 就等于什么都没复制。
 */
function isReadonlySurface(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null
  return !!el?.closest?.('.nt-md-preview, .nt-md-doc, .nt-doc-host, .nt-hex-body')
}

/** 文本编辑组合：剪贴板 + 全选 + 撤销重做。 */
const TEXT_EDIT_RE = /^(Control|Super)\+[CVXAZY]$/

function editorOwns(accel: string): boolean {
  // 单键（无修饰）：全部交给编辑器，功能键（F1~F12）除外——
  // F3 / F2 / F1 这类最常按的键归命令表，不特例就会被这条规则整块吃掉。
  if (!accel.includes('+')) {
    if (/^F\d{1,2}$/.test(accel)) return false
    return true
  }
  // Shift+方向 / Shift+字母是编辑器的（选词选行）；Shift+F3 是「上一个命中」，归命令表。
  if (/^Shift\+/.test(accel)) return !/^Shift\+F\d{1,2}$/.test(accel)
  // 剪贴板与全选放行给 WebView 原生行为：paste/copy 事件自带数据，
  // 不经过需要权限的 clipboard.readText，被命令表拦下反而贴不进来。
  if (TEXT_EDIT_RE.test(accel)) return true
  // Alt+上下 是「移动当前行」（编辑器自己的动作，CodeMirror 默认键位同键），
  // 但 Alt+左右 是「上一个 / 下一个位置」（光标跳转历史），左右留给命令表分发。
  if (/^Alt\+(Up|Down)$/.test(accel)) return true
  if (/^(Alt\+)?Tab$/.test(accel)) return true
  if (/^(Control|Super)\+\[$/.test(accel) || /^(Control|Super)\+\]$/.test(accel)) return true
  if (/^(Control|Super)\+Alt\+\\$/.test(accel)) return true
  return false
}

export function useGlobalKeys() {
  const commands = useCommandStore()
  const settings = useSettingsStore()

  function resolveId(accel: string): string | undefined {
    // 1) 用户自定义优先。空对象兜底：配置里缺这个字段时 Object.keys(undefined)
    //    会抛 TypeError，整个按键分发随之失灵——表现为「所有快捷键都没反应」。
    const overrides = settings.config.keybindings ?? {}
    const custom = Object.keys(overrides).find((k) => overrides[k] === accel)
    if (custom) return custom

    // 2) 命令自己声明的 key：以它为准，DEFAULT_KEYBINDINGS 只做兼容兜底。
    // 两份表一旦漂移，按键就变成「菜单里写着、按了没反应」。
    for (const c of commands.registry.values()) {
      if (c.key && c.key === accel) return c.id
    }

    // 3) 默认键位表（历史兼容：有些命令没在自己身上写 key）。
    return Object.entries(DEFAULT_KEYBINDINGS).find(([, v]) => v === accel)?.[0]
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') return

    const accel = toAccelerator(e)
    const id = resolveId(accel)
    if (!id) return

    // 在编辑器里：编辑器自己绑过的组合优先给它。
    if (isInEditor(e.target) && editorOwns(accel)) return

    // 只读预览里：C / X / A / V 交给浏览器。preventDefault 一旦执行，
    // 原生复制就被掐掉，而命令只会去操作看不见的编辑器。
    if (isReadonlySurface(e.target) && /^(Control|Super)\+[CXAV]$/.test(accel)) return

    // 原生输入框：单键、非 Ctrl 组合与文本编辑组合留在输入框；CodeMirror 的正文
    // 不算输入框——F3 / F2 / Alt+Z 这些命令表的键必须能在编辑器里用。
    if (isNativeInput(e.target) && !isInEditor(e.target)) {
      if (!e.ctrlKey && !e.metaKey) return
      if (TEXT_EDIT_RE.test(accel)) return
    }

    // 阻止默认：Ctrl+S 会被 WebView 当成「保存网页」，必须拦。
    e.preventDefault()
    commands.run(id)
  }

  /**
   * Ctrl + 滚轮缩放字号：鼠标在手时最顺手的一档，与工具栏的 ± 同一套刻度。
   * 必须 passive:false——浏览器默认把 Ctrl+滚轮当整页缩放，不拦就会两套缩放打架。
   */
  function onWheel(e: WheelEvent) {
    if (!e.ctrlKey || e.deltaY === 0) return
    e.preventDefault()
    const cur = settings.config.ui.zoom_level
    const next = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, cur + (e.deltaY < 0 ? 10 : -10)))
    if (next !== cur) settings.updateUI({ zoom_level: next })
  }

  onMounted(() => {
    window.addEventListener('keydown', onKeyDown, { capture: true })
    window.addEventListener('wheel', onWheel, { capture: true, passive: false })
  })
  onUnmounted(() => {
    window.removeEventListener('keydown', onKeyDown, { capture: true })
    window.removeEventListener('wheel', onWheel, { capture: true })
  })
}
