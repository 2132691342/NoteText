// 全局按键：把组合键映射到命令 id。
//
// 三入口收敛（AGENTS.md §2.10）：菜单、工具栏、按键都走命令表，
// 于是「菜单能用的功能按键一定能按」。优先级：输入框内不抢按键
// （除了 Esc 与功能键），否则用户在查找框里打字会触发编辑器命令。
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
  }
  const key = codeMap[e.code] ?? (e.key.length === 1 ? e.key.toUpperCase() : e.key)
  parts.push(key)
  return parts.join('+')
}

/** 是否正在输入：输入控件内不触发编辑器快捷键。 */
function isTyping(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null
  if (!el) return false
  const tag = el.tagName
  return (
    tag === 'INPUT' ||
    tag === 'TEXTAREA' ||
    tag === 'SELECT' ||
    el.isContentEditable ||
    // CodeMirror 的编辑区：它自己处理按键，全局只兜菜单类命令。
    !!el.closest('.cm-editor')
  )
}

export function useGlobalKeys() {
  const commands = useCommandStore()
  const settings = useSettingsStore()

  function onKeyDown(e: KeyboardEvent) {
    // Esc 永远放行：关闭浮窗不该被输入状态挡住。
    if (e.key === 'Escape') return
    if (isTyping(e.target)) return

    const accel = toAccelerator(e)
    // 覆盖优先于默认：用户在设置里改过键位。
    const overrides = settings.config.keybindings
    const id = Object.keys(overrides).find((k) => overrides[k] === accel)
      ?? Object.entries(DEFAULT_KEYBINDINGS).find(([, v]) => v === accel)?.[0]
    if (!id) return

    // 阻止默认：Ctrl+S 会被 WebView 当成「保存网页」，必须拦。
    e.preventDefault()
    commands.run(id)
  }

  onMounted(() => window.addEventListener('keydown', onKeyDown, { capture: true }))
  onUnmounted(() => window.removeEventListener('keydown', onKeyDown, { capture: true }))
}

