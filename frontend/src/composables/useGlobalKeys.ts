// 全局按键：把组合键映射到命令 id。
// 三入口收敛（AGENTS.md §2.10）：菜单、工具栏、按键都走命令表，
import { onMounted, onUnmounted } from 'vue'
import { DEFAULT_KEYBINDINGS, useCommandStore } from '@/stores/commands'
import { useSettingsStore } from '@/stores/settings'

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

function editorOwns(accel: string): boolean {
  // 单键（无修饰）：全部交给编辑器
  if (!accel.includes('+')) {
    // F1~F12 例外：编辑器不给功能键绑动作，而 F3 / Shift+F3 是最常按的，
    // 不特例就会被这条规则整块吃掉。
    if (/^F\d{1,2}$/.test(accel) || /^Shift\+F\d{1,2}$/.test(accel)) return false
    return true
  }
  // 纯 Shift 组合也是编辑器的（Shift+方向选词）
  if (/^Shift\+/.test(accel)) return true
  // Alt+上下 是「移动当前行」（编辑器自己的动作），但 Alt+左右 是
  // 「上一个 / 下一个位置」（光标跳转历史）。左右留给命令表分发。
  if (/^Alt\+Arrow(Up|Down)$/.test(accel)) return true
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

    // 原生输入框里只放行「到不了文本」的组合（Ctrl+S 这类）；
    // 单键与非 Ctrl 组合留在输入框里（否则侧栏搜索框打不了字）。
    if (isNativeInput(e.target)) {
      if (!e.ctrlKey && !e.metaKey) return
    }

    // 阻止默认：Ctrl+S 会被 WebView 当成「保存网页」，必须拦。
    e.preventDefault()
    commands.run(id)
  }

  onMounted(() => window.addEventListener('keydown', onKeyDown, { capture: true }))
  onUnmounted(() => window.removeEventListener('keydown', onKeyDown, { capture: true }))
}
