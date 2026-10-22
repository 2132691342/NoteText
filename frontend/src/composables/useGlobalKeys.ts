// 全局按键：把组合键映射到命令 id。
//
// 三入口收敛（AGENTS.md §2.10）：菜单、工具栏、按键都走命令表，
// 于是「菜单能用的功能按键一定能按」。
//
// **编辑器内也要放行**：Ctrl+S / Ctrl+F / Ctrl+H / Ctrl+/ 这类组合在
// CodeMirror 里没有任何绑定，早先整块 return 会让「保存」在编辑器里失效——
// 而这正是用户最常按的几个键。判据改成「这条组合是否有命令认领」：
// 有认领就执行并 preventDefault，没有就交给 CodeMirror。
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
  // 字母键一律取 e.code 的 `KeyX` → `X`。
  // 注意：命令表里的 `key` 就是按这里的拼法写的（`Control+,` / `Control+/`），
  // 两边必须同形，否则「菜单显示有快捷键、按了没反应」。
  // 不能用 e.key：按住 Shift 时 e.key 是 'F' 而键位表写的是 'F'（大写）还好，
  // 但 Ctrl+F 在某些输入法状态下 e.key 会变成不可打印字符，匹配就断了。
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

function editorOwns(accel: string): boolean {
  // 单键（无修饰）：全部交给编辑器
  if (!accel.includes('+')) return true
  // 纯 Shift 组合也是编辑器的（Shift+方向选词）
  if (/^Shift\+/.test(accel)) return true
  if (/^Alt\+Arrow(Up|Down|Left|Right)$/.test(accel)) return true
  if (/^(Alt\+)?Tab$/.test(accel)) return true
  if (/^(Control|Super)\+\[$/.test(accel) || /^(Control|Super)\+\]$/.test(accel)) return true
  if (/^(Control|Super)\+Alt\+\\$/.test(accel)) return true
  return false
}

export function useGlobalKeys() {
  const commands = useCommandStore()
  const settings = useSettingsStore()

  function resolveId(accel: string): string | undefined {
    // 1) 用户自定义优先。
    const overrides = settings.config.keybindings
    const custom = Object.keys(overrides).find((k) => overrides[k] === accel)
    if (custom) return custom

    // 2) 命令自己声明的 key。
    //
    // 这一条以前缺着：命令表里写了 `key: 'Control+,'` 只用于菜单右侧的提示，
    // 真正分发靠另一份 DEFAULT_KEYBINDINGS。两份表一旦漂移，按键就变成
    // 「菜单里写着、按了没反应」。现在以命令声明为准，默认表只做兼容兜底。
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
