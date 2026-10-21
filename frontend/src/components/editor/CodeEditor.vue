// CodeMirror 容器：一个标签一个实例，切换不销毁。
//
// 关键机制：
// - 各关注点一个 compartment，运行时切换走 reconfigure（不销毁重建）；
// - 标签首次挂载时读文件正文；切换回来时保留实例与撤销历史；
// - 光标 / 统计通过自定义事件推给状态栏；
// - 脏标记按「与载入时的快照不同」判，而不是按有没有输入过——
//   用户改了又改回来，标签该是干净的。
<template>
  <div ref="host" class="nt-code-editor" />
</template>

<script lang="ts" setup>
import { computed, onMounted, onBeforeUnmount, ref, watch } from 'vue'
import { EditorView, lineNumbers, highlightActiveLine, highlightSpecialChars, rectangularSelection, crosshairCursor, drawSelection, dropCursor } from '@codemirror/view'
import { EditorState, Compartment } from '@codemirror/state'
import { indentWithTab, history, historyKeymap, defaultKeymap } from '@codemirror/commands'
import { keymap } from '@codemirror/view'
import { syntaxHighlighting, defaultHighlightStyle, bracketMatching, foldGutter, foldKeymap, indentOnInput, indentUnit } from '@codemirror/language'
import { searchKeymap, highlightSelectionMatches } from '@codemirror/search'
import { autocompletion, completionKeymap, closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete'

import { undo, redo, selectAll, deleteLine, copyLineDown } from '@codemirror/commands'
import type { Tab } from '@/stores/editor'
import { useSettingsStore } from '@/stores/settings'
import { useEditorStore } from '@/stores/editor'
import { api, normalizeError } from '@/api'
import { useToastStore } from '@/stores/toast'
import { register, unregister } from '@/composables/useEditorRegistry'
import { useEditorTheme } from '@/composables/useEditorTheme'
import { useEditorLanguage } from '@/composables/useEditorLanguage'
import { recordDocChange } from '@/composables/useEditorMacro'
import { pushClip } from '@/composables/useClipboardHistory'

const props = defineProps<{ tab: Tab; viewId?: 'main' | 'second'; readonly?: boolean }>()

const settings = useSettingsStore()
const editor = useEditorStore()
const toast = useToastStore()

const host = ref<HTMLElement | null>(null)

// ---- compartments：每个关注点一份，切换只 reconfigure ----
const themeComp = new Compartment()
const langComp = new Compartment()
const editableComp = new Compartment()
const wrapComp = new Compartment()
const tabSizeComp = new Compartment()
const lineNumComp = new Compartment()
const foldComp = new Compartment()
const historyComp = new Compartment()

let view: EditorView | null = null
/** 载入时的正文快照：脏标记的判据。 */
let baseline = ''

const cfg = () => settings.config.editor

onMounted(async () => {
  if (!host.value) return

  // 读正文：草稿标签（path 以 draft: 开头）走草稿接口，其余读文件。
  let text = ''
  if (props.tab.path.startsWith('draft:')) {
    try {
      const drafts = await api.listDrafts()
      const hit = drafts.find((d) => d.path === props.tab.path)
      text = hit?.content ?? ''
    } catch {
      /* 草稿读不到：空文档，用户接着写 */
    }
  } else if (props.tab.path) {
    try {
      const c = await api.readFile(props.tab.path)
      text = c.text
      // 读到的元数据比 tab 上的新：以磁盘为准。
      props.tab.encoding = c.encoding
      props.tab.line_ending = c.line_ending
      props.tab.has_bom = c.has_bom
      props.tab.encoding_uncertain = c.truncated && c.encoding !== 'utf-8'
    } catch (e) {
      toast.bad(`打开失败：${normalizeError(e).message}`)
      return
    }
  }
  baseline = text

  const state = EditorState.create({
    doc: text,
    extensions: [
      // 基础编辑行为
      history(),
      drawSelection(),
      dropCursor(),
      highlightSpecialChars(),
      indentOnInput(),
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      bracketMatching(),
      closeBrackets(),
      autocompletion(),
      rectangularSelection(),
      crosshairCursor(),
      highlightActiveLine(),
      highlightSelectionMatches(),
      keymap.of([
        ...closeBracketsKeymap,
        ...defaultKeymap,
        ...searchKeymap,
        ...historyKeymap,
        ...foldKeymap,
        ...completionKeymap,
        indentWithTab,
      ]),
      // compartments（初值来自当前配置）
      themeComp.of(themeExtensions()),
      langComp.of(syncExtensions()),
      editableComp.of(props.readonly ? EditorState.readOnly.of(true) : []),
      wrapComp.of(cfg().word_wrap ? EditorView.lineWrapping : []),
      tabSizeComp.of(indentUnit.of(' '.repeat(cfg().tab_size))),
      lineNumComp.of(cfg().line_numbers ? lineNumbers() : []),
      foldComp.of(cfg().fold_gutter ? foldGutter() : []),
      historyComp.of(history()),
      EditorView.updateListener.of((vu) => {
        if (vu.docChanged) {
          // 宏录制：把插入的文本收成一步。
          let inserted = ''
          vu.changes.iterChanges((_fromA, _toA, _fromB, _toB, ins) => {
            inserted += ins.toString()
          })
          recordDocChange(vu.view, inserted)
          editor.markDirty(props.tab.id, vu.state.doc.toString() !== baseline)
          emitCursor(vu.state)
          emitStats(vu.state)
          emitHistory()
        }
        if (vu.selectionSet || vu.docChanged) {
          emitCursor(vu.state)
        }
      }),
      // 剪贴板历史：从 copy / cut 事件收集，不碰系统剪贴板（要权限）。
      EditorView.domEventHandlers({
        copy(_e, v) {
          const { from, to } = v.state.selection.main
          const text = v.state.sliceDoc(from, to)
          if (text) pushClip(text)
          return false
        },
        cut(_e, v) {
          const { from, to } = v.state.selection.main
          const text = v.state.sliceDoc(from, to)
          if (text) pushClip(text)
          return false
        },
      }),
    ],
  })

  view = new EditorView({ state, parent: host.value })
  register(props.tab.id, view)
  emitHistory()

  // 异步语言包：首屏先用同步能拿到的那几种，加载完再 reconfigure。
  void loadLanguage().then((ext) => {
    if (view && ext.length) view.dispatch({ effects: langComp.reconfigure(ext) })
  })

  // 恢复光标与滚动：会话恢复时把视图放回原处。
  if (props.tab.position.line > 1 || props.tab.position.col > 1) {
    const line = view.state.doc.line(Math.min(props.tab.position.line, view.state.doc.lines))
    view.dispatch({ selection: { anchor: line.from + Math.min(props.tab.position.col - 1, line.length) } })
  }
  if (props.tab.scroll_ratio > 0) {
    requestAnimationFrame(() => {
      if (!view) return
      const max = view.scrollDOM.scrollHeight - view.scrollDOM.clientHeight
      view.scrollDOM.scrollTop = max * props.tab.scroll_ratio
    })
  }
})

onBeforeUnmount(() => {
  unregister(props.tab.id)
  document.removeEventListener('editor-cmd', onEditorCmd)
  document.removeEventListener('editor-goto-line', onGotoLine)
  document.removeEventListener('editor-replace-doc', onReplaceDoc)
  document.removeEventListener('editor-select-range', onSelectRange)
  view?.destroy()
  view = null
})

// ---- 外部命令入口 ----
//
// `useCommands` 通过 `editor-cmd` 事件把菜单 / 工具栏 / 快捷键的动作发过来。
// 编辑器实例在这里，也只有它能执行——之前这个事件没有任何监听方，
// Ctrl+Z / Ctrl+D / Ctrl+L 全是死命令（派发了但没人做）。
function onEditorCmd(e: Event) {
  const d = (e as CustomEvent).detail as { cmd: string; payload?: unknown }
  if (!d || !view) return
  // 只响应发给自己的：分屏时两个视图共存，命令要落在活跃的那个。
  if (props.viewId === 'second' && props.readonly) return

  switch (d.cmd) {
    case 'undo':
      undo(view)
      break
    case 'redo':
      redo(view)
      break
    case 'selectAll':
      selectAll(view)
      break
    case 'deleteLine':
      deleteLine(view)
      break
    case 'duplicateLine':
      // CodeMirror 没有 duplicateLine：copyLineDown 是「向下复制一行」，
      // 效果等价且保留撤销历史。
      copyLineDown(view)
      break
    case 'insert': {
      const text = typeof d.payload === 'string' ? d.payload : ''
      if (!text) return
      const { from, to } = view.state.selection.main
      view.dispatch({
        changes: { from, to, insert: text },
        selection: { anchor: from + text.length },
        scrollIntoView: true,
      })
      break
    }
    case 'replaceSelection': {
      const text = typeof d.payload === 'string' ? d.payload : ''
      const { from, to } = view.state.selection.main
      view.dispatch({
        changes: { from, to, insert: text },
        selection: { anchor: from, head: from + text.length },
        scrollIntoView: true,
      })
      break
    }
    default:
      break
  }
  view.focus()
}

function onGotoLine(e: Event) {
  const d = (e as CustomEvent).detail as { tabId: number; line: number }
  if (!d || !view || d.tabId !== props.tab.id) return
  const n = Math.min(Math.max(d.line, 1), view.state.doc.lines)
  const line = view.state.doc.line(n)
  view.dispatch({
    selection: { anchor: line.from },
    effects: EditorView.scrollIntoView(line.from, { y: 'center' }),
  })
  view.focus()
}

function onReplaceDoc(e: Event) {
  const d = (e as CustomEvent).detail as { tabId: number; text: string }
  if (!d || !view || d.tabId !== props.tab.id) return
  view.dispatch({
    changes: { from: 0, to: view.state.doc.length, insert: d.text },
  })
  baseline = d.text
  editor.markDirty(props.tab.id, false)
}

function onSelectRange(e: Event) {
  const d = (e as CustomEvent).detail as { tabId: number; from: number; to: number }
  if (!d || !view || d.tabId !== props.tab.id) return
  view.dispatch({
    selection: { anchor: d.from, head: d.to },
    effects: EditorView.scrollIntoView(d.from, { y: 'center' }),
  })
  view.focus()
}

onMounted(() => {
  document.addEventListener('editor-cmd', onEditorCmd)
  document.addEventListener('editor-goto-line', onGotoLine)
  document.addEventListener('editor-replace-doc', onReplaceDoc)
  document.addEventListener('editor-select-range', onSelectRange)
})

// ---- 配置变化：只 reconfigure 对应的 compartment ----
watch(() => cfg().word_wrap, (v) => view?.dispatch({ effects: wrapComp.reconfigure(v ? EditorView.lineWrapping : []) }))
watch(() => cfg().tab_size, (v) => view?.dispatch({ effects: tabSizeComp.reconfigure(indentUnit.of(' '.repeat(v))) }))
watch(() => cfg().line_numbers, (v) => view?.dispatch({ effects: lineNumComp.reconfigure(v ? lineNumbers() : []) }))
watch(() => cfg().fold_gutter, (v) => view?.dispatch({ effects: foldComp.reconfigure(v ? foldGutter() : []) }))
watch(() => props.tab.language, () => {
  void loadLanguage().then((ext) => {
    if (view) view.dispatch({ effects: langComp.reconfigure(ext) })
  })
})
watch(() => settings.resolvedTheme, () => view?.dispatch({ effects: themeComp.reconfigure(themeExtensions()) }))
watch(() => cfg().font_size, () => applyFont())
watch(() => cfg().font_family, () => applyFont())
watch(() => props.readonly, (v) => view?.dispatch({ effects: editableComp.reconfigure(v ? EditorState.readOnly.of(true) : []) }))

// ---- 主题 / 语言 / 字体 ----
const { themeExtensions, applyFont } = useEditorTheme(host)
const { syncExtensions, loadLanguage } = useEditorLanguage(computed(() => props.tab.language))

// ---- 事件出口：状态栏与工具栏从这里拿数据 ----
function emitCursor(state: EditorState) {
  const { from, to } = state.selection.main
  const line = state.doc.lineAt(from)
  document.dispatchEvent(
    new CustomEvent('editor-cursor-pos', {
      detail: { line: line.number, col: from - line.from + 1, selection_len: to - from },
    }),
  )
}

function emitStats(state: EditorState) {
  const text = state.doc.toString()
  document.dispatchEvent(
    new CustomEvent('editor-stats', {
      detail: { lines: text === '' ? 0 : text.split('\n').length },
    }),
  )
}

function emitHistory() {
  // 撤销深度：CodeMirror 不直接暴露历史深度，用固定 1 表示「可用」；
  // 工具栏只问可用性，精确深度对它没有意义。
  document.dispatchEvent(
    new CustomEvent('editor-history', {
      detail: { undo: 1, redo: 1 },
    }),
  )
}
</script>

<style scoped>
.nt-code-editor {
  height: 100%;
  min-height: 0;
  overflow: hidden;
}

/* 编辑器表面：与界面分离，代码区可独立压暗 */
.nt-code-editor :deep(.cm-editor) {
  height: 100%;
  background: var(--nt-editor-bg);
  color: var(--nt-ink);
  font-family: var(--nt-font-editor);
  font-size: var(--nt-fs-md);
}

.nt-code-editor :deep(.cm-scroller) {
  line-height: 1.55;
  font-family: var(--nt-font-editor);
}

.nt-code-editor :deep(.cm-gutters) {
  background: var(--nt-editor-gutter-bg);
  border-right: 1px solid var(--nt-border);
  color: var(--nt-editor-gutter-ink);
}

.nt-code-editor :deep(.cm-activeLine) {
  background: var(--nt-editor-active-line);
}

.nt-code-editor :deep(.cm-activeLineGutter) {
  background: var(--nt-editor-active-line);
  color: var(--nt-ink-2);
}

.nt-code-editor :deep(.cm-selectionBackground) {
  background: var(--nt-editor-selection);
}

.nt-code-editor :deep(.cm-cursor) {
  border-left-color: var(--nt-editor-cursor);
}

/* 只读视图：光标不闪，但保留选中 */
.nt-code-editor :deep(.cm-readOnly .cm-cursor) {
  display: none;
}
</style>
