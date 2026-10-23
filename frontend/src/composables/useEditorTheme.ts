// 编辑器主题：CodeMirror 的扩展跟随 --nt-* 令牌。
// 为什么手写而不是引 @codemirror/theme-one-dark：EP 的主题包写死色值，

import { EditorView } from '@codemirror/view'
import type { Extension } from '@codemirror/state'
import { syntaxHighlighting, HighlightStyle } from '@codemirror/language'
import { tags as t } from '@lezer/highlight'
import { type Ref } from 'vue'
import { useSettingsStore } from '@/stores/settings'

/** 语法高亮配色：只给「区分度最高」的几类上色，其余用正文色。
 *  高亮上得太花，读代码时眼睛找不到重点。 */
const highlightStyle = HighlightStyle.define([
  { tag: t.comment, color: 'var(--nt-ink-muted)', fontStyle: 'italic' },
  { tag: t.keyword, color: 'var(--nt-primary)' },
  { tag: [t.name, t.deleted, t.character, t.macroName], color: 'var(--nt-ink)' },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: 'var(--nt-ch-1)' },
  { tag: [t.string, t.special(t.string)], color: 'var(--nt-success)' },
  { tag: [t.number, t.bool, t.null], color: 'var(--nt-warning)' },
  { tag: [t.typeName, t.className, t.tagName], color: 'var(--nt-ch-2)' },
  { tag: [t.propertyName], color: 'var(--nt-ink-2)' },
  { tag: [t.operator, t.punctuation], color: 'var(--nt-ink-2)' },
  { tag: t.link, color: 'var(--nt-primary)', textDecoration: 'underline' },
  { tag: t.heading, color: 'var(--nt-primary)', fontWeight: '600' },
])

export function useEditorTheme(host: Ref<HTMLElement | null>) {
  const settings = useSettingsStore()

  /** 主题扩展：引用 CSS 变量，不写死色值。 */
  function themeExtensions(): Extension[] {
    return [
      EditorView.theme(
        {
          '&': {
            backgroundColor: 'var(--nt-editor-bg)',
            color: 'var(--nt-ink)',
            fontSize: 'var(--nt-fs-md)',
          },
          '.cm-content': {
            caretColor: 'var(--nt-editor-cursor)',
            fontFamily: 'var(--nt-font-editor)',
            padding: 'var(--nt-sp-2) 0',
          },
          '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--nt-editor-cursor)' },
          '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection':
            { backgroundColor: 'var(--nt-editor-selection)' },
          '.cm-gutters': {
            backgroundColor: 'var(--nt-editor-gutter-bg)',
            color: 'var(--nt-editor-gutter-ink)',
            border: 'none',
          },
          '.cm-activeLine': { backgroundColor: 'var(--nt-editor-active-line)' },
          '.cm-activeLineGutter': {
            backgroundColor: 'var(--nt-editor-active-line)',
            color: 'var(--nt-ink-2)',
          },
          '.cm-foldPlaceholder': {
            backgroundColor: 'var(--nt-surface-2)',
            border: '1px solid var(--nt-border)',
            borderRadius: 'var(--nt-radius-xs)',
            color: 'var(--nt-ink-muted)',
            padding: '0 4px',
          },
          '.cm-panels': {
            backgroundColor: 'var(--nt-surface)',
            color: 'var(--nt-ink)',
            borderTop: '1px solid var(--nt-border)',
          },
          '.cm-panels input': {
            backgroundColor: 'var(--nt-control-bg)',
            color: 'var(--nt-ink)',
            border: '1px solid var(--nt-control-border)',
            borderRadius: 'var(--nt-radius-xs)',
          },
          '.cm-searchMatch': {
            backgroundColor: 'var(--nt-primary-soft)',
            outline: '1px solid var(--nt-primary)',
          },
          '.cm-searchMatch-selected': {
            backgroundColor: 'var(--nt-primary)',
            color: 'var(--nt-primary-contrast)',
          },
          '.cm-selectionMatch': { backgroundColor: 'var(--nt-primary-soft)' },
          '.cm-tooltip': {
            backgroundColor: 'var(--nt-surface)',
            border: '1px solid var(--nt-border)',
            borderRadius: 'var(--nt-radius-sm)',
            boxShadow: 'var(--nt-card-shadow)',
          },
          '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
            backgroundColor: 'var(--nt-surface-selected)',
            color: 'var(--nt-primary)',
          },
        },
        { dark: settings.resolvedTheme === 'dark' },
      ),
      syntaxHighlighting(highlightStyle),
    ]
  }

  /** 字号 / 字体变化：直接写宿主元素的样式，比 reconfigure 便宜。 */
  function applyFont() {
    const el = host.value
    if (!el) return
    el.style.fontFamily = 'var(--nt-font-editor)'
    el.style.fontSize = `calc(${settings.config.editor.font_size}px * var(--nt-fs-scale) * var(--nt-vw-scale))`
  }

  return { themeExtensions, applyFont }
}
