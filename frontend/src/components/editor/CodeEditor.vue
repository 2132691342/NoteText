// CodeMirror 容器：一个标签一个实例，切换标签不销毁。
// 各关注点一个 compartment，运行时切换走 reconfigure；脏标记按
<template>
  <div ref="host" class="nt-code-editor" @contextmenu="onContextMenu">
    <div v-if="mountError" class="nt-mount-error">
      <span class="nt-mount-error-title">{{ tr('editor.openFailedTitle') }}</span>
      <span class="nt-mount-error-detail">{{ mountError }}</span>
    </div>
  </div>
  <!-- 选区浮条：fixed 定位贴选区起点上方（coordsAtPos 给的是视口坐标） -->
  <div
    v-if="selBar"
    class="nt-sel-bar"
    :style="{ left: `${Math.max(selBar.x, 8)}px`, top: `${Math.max(selBar.y, 8)}px` }"
    @mousedown.prevent
  >
    <button v-for="it in selBarItems" :key="it.label" class="nt-sel-bar-btn" @click="it.run()">
      {{ it.label }}
    </button>
  </div>
  <ContextMenu
    v-if="menu"
    :x="menu.x"
    :y="menu.y"
    :items="menuItems"
    :min-width="200"
    @close="menu = null"
  />
</template>

<script lang="ts" setup>
import { computed, onMounted, onBeforeUnmount, ref, watch, nextTick } from 'vue'
import {
  Decoration,
  type DecorationSet,
  ViewPlugin,
  type PluginValue,
  EditorView,
  lineNumbers,
  gutter,
  GutterMarker,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  rectangularSelection,
  crosshairCursor,
  drawSelection,
  dropCursor,
  keymap,
  highlightWhitespace,
  placeholder,
  scrollPastEnd,
  type ViewUpdate,
} from '@codemirror/view'
import { EditorState, Compartment, RangeSetBuilder, type Extension } from '@codemirror/state'
import {
  indentWithTab,
  history,
  historyKeymap,
  defaultKeymap,
  undo,
  redo,
} from '@codemirror/commands'
import {
  syntaxHighlighting,
  defaultHighlightStyle,
  bracketMatching,
  foldGutter,
  foldKeymap,
  indentOnInput,
  indentUnit,
} from '@codemirror/language'
import { searchKeymap, highlightSelectionMatches } from '@codemirror/search'
import {
  autocompletion,
  completionKeymap,
  closeBrackets,
  closeBracketsKeymap,
} from '@codemirror/autocomplete'
import type { Tab } from '@/stores/editor'
import { useSettingsStore } from '@/stores/settings'
import { useEditorStore } from '@/stores/editor'
import { api } from '@/api'
import { useToastStore } from '@/stores/toast'
import { register, unregister, isDirty, getSelection, bumpDocVersion } from '@/composables/useEditorRegistry'
import ContextMenu from '@/components/common/ContextMenu.vue'
import type { MenuItem } from '@/components/common/contextMenu'
import {
  doUndo,
  doRedo,
  doSelectAll,
  doDeleteLine,
  doDuplicateLine,
  doMoveLine,
  doSelectLine,
  doDeleteSelection,
  doCaseTransform,
  doSortLines,
  doDedupLines,
  doTrimTrailing,
  doReverseLines,
  doConvertIndent,
  doBookmarkLineOp,
  doPasteBookmarkLines,
  doMarkSelection,
  doClearMarks,
  bookmarkClip,
  setMarksRenderer,
  renderMarks,
  allMarkWords,
  type MarkColor,
  viewOf,
} from '@/composables/useEditorActions'
import { useCommandStore } from '@/stores/commands'
import { useDialogStore } from '@/stores/dialogs'
import { copyText, readClipboard } from '@/utils'
import { i18n } from '@/i18n'
import { useEditorTheme } from '@/composables/useEditorTheme'
import { useEditorLanguage } from '@/composables/useEditorLanguage'
import { guideExtensions } from '@/composables/useEditorGuides'
import { recordJump } from '@/composables/useNavHistory'
import { recordDocChange } from '@/composables/useEditorMacro'
import { pushClip } from '@/composables/useClipboardHistory'
import {
  quickJsonFormat,
  quickJsonMinify,
  quickEscape,
  quickUnescape,
  quickConvert,
  quickCase,
} from '@/composables/useQuickActions'

const props = defineProps<{ tab: Tab; viewId?: 'main' | 'second'; readonly?: boolean }>()

/**
 * 5 色标记的装饰插件。
 *
 * 词集合由动作层持有（`allMarkWords`），这里只负责把它画出来。
 * 分屏 / 多标签每个视图各挂一份：同一组标记在所有视图上都看得见，
 * 与「标记是这份文件的看法」一致。词 → 颜色档固定，
 * 用户不会因为点了两次就换个颜色。
 */
function markClassOf(word: string): number {
  let h = 0
  for (let i = 0; i < word.length; i += 1) h = (h * 31 + word.charCodeAt(i)) % 5
  return h + 1
}

class MarkPluginValue implements PluginValue {
  decorations: DecorationSet

  constructor(
    private view: EditorView,
    private words: Set<string>,
  ) {
    this.decorations = this.build()
  }

  update(u: ViewUpdate) {
    if (u.docChanged || u.viewportChanged) this.decorations = this.build()
  }

  private build(): DecorationSet {
    if (this.words.size === 0) return Decoration.none
    const builder = new RangeSetBuilder<Decoration>()
    const doc = this.view.state.doc
    // 只扫可见区间：给几万行的文件每个词都算一遍位置会卡住输入。
    for (const { from, to } of this.view.visibleRanges) {
      const text = doc.sliceString(from, to)
      for (const word of this.words) {
        if (!word) continue
        let idx = text.indexOf(word)
        while (idx >= 0) {
          const start = from + idx
          builder.add(
            start,
            start + word.length,
            Decoration.mark({ class: `nt-mark nt-mark-${markClassOf(word)}` }),
          )
          idx = text.indexOf(word, idx + word.length)
        }
      }
    }
    return builder.finish()
  }
}

/** 建一个标记插件扩展（每个视图一份，词集合快照进来）。 */
function markExtension(words: Set<string>): Extension {
  return ViewPlugin.fromClass(
    class extends MarkPluginValue {
      constructor(view: EditorView) {
        super(view, words)
      }
    },
  )
}

const settings = useSettingsStore()
const editor = useEditorStore()
const toast = useToastStore()
const commands = useCommandStore()
const dialogs = useDialogStore()
const tr = (k: string, n: Record<string, unknown> = {}) => i18n.global.t(k, n)

/** 右键菜单位置：null = 不显示。 */
const menu = ref<{ x: number; y: number } | null>(null)
/** 打开菜单那一刻的条目快照：选区不是响应式状态，computed 会缓存
 *  过期的 hasSel（表现为「选中了却不出快操作段」），必须现算现用。 */
const menuItems = ref<MenuItem[]>([])

const host = ref<HTMLElement | null>(null)

// ---- compartments：每个关注点一份，切换只 reconfigure ----
const themeComp = new Compartment()
const langComp = new Compartment()
const editableComp = new Compartment()
const wrapComp = new Compartment()
const tabSizeComp = new Compartment()
const lineNumComp = new Compartment()
const foldComp = new Compartment()
const activeLineComp = new Compartment()
const bracketComp = new Compartment()
const whitespaceComp = new Compartment()
// 视觉辅助三件：缩进参考线 / 括号配对着色 / 行尾标记。
// 三者都是「看法」，合成一个 compartment 整体 reconfigure。
const guidesComp = new Compartment()
// 书签 gutter：行号左侧一列点。状态存在视图里（不进 store：书签是
// 「这个文件在这个位置有个标记」，与标签元数据无关）。
const bookmarkComp = new Compartment()
// 5 色标记装饰：词集合整体换，所以也是 reconfigure 而不是重建视图。
const marksComp = new Compartment()

let view: EditorView | null = null

/** 挂载失败的原因（文件读不了 / 会话恢复了已被删除的文件）。
 *  非空时这个标签没有编辑器实例——查找、保存等命令会全部静默失效，
 *  必须把「为什么是空的」画在界面上，而不是留一张空白标签。 */
const mountError = ref('')

/** 选区浮条：选中即出现，常用操作一步执行。 */
const selBar = ref<{ x: number; y: number } | null>(null)

/** 按当前选区更新浮条位置；空选区收起。 */
function refreshSelBar(v: EditorView) {
  const { from, to } = v.state.selection.main
  if (from >= to || props.readonly) {
    selBar.value = null
    return
  }
  // 长选区贴起点上方：终点可能在视口外，贴终点会「浮条满屏跳」。
  const head = v.coordsAtPos(from)
  if (!head) {
    selBar.value = null
    return
  }
  selBar.value = { x: head.left, y: head.top - 34 }
}

/** 浮条按钮组：只放一步就能完成的动作，多步的走「更多」浮窗。 */
const selBarItems = computed(() => [
  { label: tr('quick.jsonFormat'), run: () => void quickJsonFormat(props.tab.id) },
  { label: tr('quick.jsonMinify'), run: () => void quickJsonMinify(props.tab.id) },
  { label: tr('quick.jsonToYaml'), run: () => void quickConvert(props.tab.id, 'json', 'yaml') },
  { label: tr('menu.caseUpper'), run: () => quickCase(props.tab.id, 'upper') },
  { label: tr('menu.caseLower'), run: () => quickCase(props.tab.id, 'lower') },
  { label: tr('menu.find'), run: () => commands.run('edit.find') },
])

const cfg = () => settings.config.editor

/** 编辑器视图的扩展集合（不含语言与主题，那些是 compartment）。 */
function baseExtensions(editable: boolean) {
  return [
    // history 只注册一次：注册两次会让每一步操作进两个栈，
    // 撤销一次只退回半步，用户按两下才看到效果。
    history(),
    drawSelection(),
    dropCursor(),
    highlightSpecialChars(),
    indentOnInput(),
    syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
    closeBrackets(),
    autocompletion(),
    rectangularSelection(),
    crosshairCursor(),
    scrollPastEnd(),
    // 选中即高亮同词：日志/代码里找「这个词还出现在哪」非常高频。
    cfg().highlight_selection ? highlightSelectionMatches() : [],
    keymap.of([
      ...closeBracketsKeymap,
      ...defaultKeymap,
      ...searchKeymap,
      ...historyKeymap,
      ...foldKeymap,
      ...completionKeymap,
      indentWithTab,
    ]),
    editable ? EditorState.readOnly.of(false) : EditorState.readOnly.of(true),
    EditorView.updateListener.of(onUpdate),
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
  ]
}

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
      // 读不了（文件被删 / 被独占）：画出错误面，不建编辑器。
      // 静默留一张空白标签的后果是查找、保存全部「按了没反应」。
      mountError.value = normalizeErr(e)
      return
    }
  }

  const state = EditorState.create({
    doc: text,
    extensions: [
      ...baseExtensions(!props.readonly),
      // compartments（初值来自当前配置）
      themeComp.of(themeExtensions()),
      langComp.of(syncExtensions()),
      editableComp.of(props.readonly ? EditorState.readOnly.of(true) : []),
      wrapComp.of(cfg().word_wrap ? EditorView.lineWrapping : []),
      tabSizeComp.of(indentUnit.of(indentText())),
      lineNumComp.of(cfg().line_numbers ? lineNumbers() : []),
      activeLineComp.of(cfg().highlight_active_line ? [highlightActiveLine(), highlightActiveLineGutter()] : []),
      bracketComp.of(cfg().bracket_matching ? bracketMatching() : []),
      foldComp.of(cfg().fold_gutter ? foldGutter() : []),
      bookmarkComp.of(bookmarkGutter()),
      marksComp.of(markExtension(allMarkWords())),
      guidesComp.of(guideExtensions(cfg())),
      whitespaceComp.of(
        cfg().show_whitespace ? highlightWhitespace() : placeholder(''),
      ),
    ],
  })

  view = new EditorView({ state, parent: host.value })
  register(props.tab.id, view, view.state.doc)
  document.dispatchEvent(new CustomEvent('nt-editor-ready', { detail: { tabId: props.tab.id } }))

  // 异步语言包：首屏先用同步能拿到的那几种，加载完再 reconfigure。
  void loadLanguage().then((ext) => {
    if (view && ext.length) view.dispatch({ effects: langComp.reconfigure(ext) })
  })

  // 恢复光标与滚动：会话恢复时把视图放回原处。
  if (props.tab.position.line > 1 || props.tab.position.col > 1) {
    const line = view.state.doc.line(
      Math.min(props.tab.position.line, view.state.doc.lines),
    )
    view.dispatch({
      selection: { anchor: line.from + Math.min(props.tab.position.col - 1, line.length) },
    })
  }
  if (props.tab.scroll_ratio > 0) {
    requestAnimationFrame(() => {
      if (!view) return
      const max = view.scrollDOM.scrollHeight - view.scrollDOM.clientHeight
      view.scrollDOM.scrollTop = max * props.tab.scroll_ratio
    })
  }

  void loadBookmarks()

  // 主视图挂载后把焦点给它：用户按下 Ctrl+N 就期待能直接打字。
  if (props.viewId !== 'second') {
    void nextTick(() => view?.focus())
  }
})

onBeforeUnmount(() => {
  unregister(props.tab.id)
  view?.destroy()
  view = null
})

/** 一次文档变更：脏标记、光标、撤销可用性、宏录制。 */
function onUpdate(vu: ViewUpdate) {
  if (vu.docChanged) {
    // 正文变了选区一定也变了：浮条位置要跟着正文重算，
    // 而下面 docChanged 分支会提前 return，所以这里先刷新。
    refreshSelBar(vu.view)
    // 正文版本号：让「跟着正文走」的消费方（Markdown 预览）能感知变化。
    bumpDocVersion(props.tab.id)
    // 宏录制：把插入的文本收成一步。
    let inserted = ''
    vu.changes.iterChanges((_fromA, _toA, _fromB, _toB, ins) => {
      inserted += ins.toString()
    })
    recordDocChange(vu.view, inserted)
    // 脏标记按「与基线不同」判：改了又改回来是干净的。
    const dirty = isDirty(props.tab.id)
    if (props.tab.dirty !== dirty) editor.markDirty(props.tab.id, dirty)
    emitCursor(vu.state)
    emitStats(vu.state)
    emitHistory()
    return
  }
  if (vu.selectionSet) {
    emitCursor(vu.state)
    refreshSelBar(vu.view)
    // 跳转历史：位移超阈值才记，否则打字与方向键微调会把栈撑爆。
    // 只在主视图记：分屏第二视图是只读的，用户不会在它里面跳转。
    if (props.viewId !== 'second') {
      const prevHead = vu.startState.selection.main.head
      const head = vu.state.selection.main.head
      recordJump(props.tab.id, prevHead, head)
    }
    return
  }
  if (vu.viewportChanged && props.viewId !== 'second') {
    emitScroll(vu.view)
  }
}

// ---- 事件出口：状态栏与工具栏从这里拿数据 ----
// 一律带 tabId：分屏与多标签同时挂载，不带 id 的事件会让状态栏串位。
function emitCursor(state: EditorState) {
  const { from, to } = state.selection.main
  const line = state.doc.lineAt(from)
  document.dispatchEvent(
    new CustomEvent('nt-cursor', {
      detail: {
        tabId: props.tab.id,
        line: line.number,
        col: from - line.from + 1,
        selection_len: to - from,
      },
    }),
  )
}

function emitStats(state: EditorState) {
  const text = state.doc.toString()
  document.dispatchEvent(
    new CustomEvent('nt-stats', {
      detail: { tabId: props.tab.id, lines: text === '' ? 0 : text.split('\n').length },
    }),
  )
}

function emitHistory() {
  document.dispatchEvent(
    new CustomEvent('nt-history', { detail: { tabId: props.tab.id } }),
  )
}

function emitScroll(v: EditorView) {
  const max = v.scrollDOM.scrollHeight - v.scrollDOM.clientHeight
  if (max <= 0) return
  editor.setScroll(props.tab.id, v.scrollDOM.scrollTop / max)
}

// ---- 配置变化：只 reconfigure 对应的 compartment ----
watch(() => cfg().word_wrap, (v) =>
  view?.dispatch({ effects: wrapComp.reconfigure(v ? EditorView.lineWrapping : []) }),
)
watch(() => [cfg().tab_size, cfg().insert_spaces], () =>
  view?.dispatch({ effects: tabSizeComp.reconfigure(indentUnit.of(indentText())) }),
)
watch(() => cfg().line_numbers, (v) =>
  view?.dispatch({ effects: lineNumComp.reconfigure(v ? lineNumbers() : []) }),
)
watch(() => cfg().fold_gutter, (v) =>
  view?.dispatch({ effects: foldComp.reconfigure(v ? foldGutter() : []) }),
)
watch(() => cfg().highlight_active_line, (v) =>
  view?.dispatch({
    effects: activeLineComp.reconfigure(
      v ? [highlightActiveLine(), highlightActiveLineGutter()] : [],
    ),
  }),
)
watch(() => cfg().bracket_matching, (v) =>
  view?.dispatch({ effects: bracketComp.reconfigure(v ? bracketMatching() : []) }),
)
watch(() => cfg().show_whitespace, (v) =>
  view?.dispatch({
    effects: whitespaceComp.reconfigure(v ? highlightWhitespace() : placeholder('')),
  }),
)
watch(
  () => [cfg().indent_guides, cfg().bracket_pair_colors, cfg().show_eol],
  () => view?.dispatch({ effects: guidesComp.reconfigure(guideExtensions(cfg())) }),
)
watch(() => props.tab.language, () => {
  void loadLanguage().then((ext) => {
    if (view) view.dispatch({ effects: langComp.reconfigure(ext) })
  })
})
watch(() => settings.resolvedTheme, () =>
  view?.dispatch({ effects: themeComp.reconfigure(themeExtensions()) }),
)
watch([() => cfg().font_size, () => cfg().font_family], () => applyFont())
watch(() => props.readonly, (v) =>
  view?.dispatch({ effects: editableComp.reconfigure(v ? EditorState.readOnly.of(true) : []) }),
)
watch(() => props.tab.id, () => {
  // 理论上不会发生（每个标签一个组件实例），保留给未来复用场景。
})

// ---- 书签 ----

/** 书签集合：行号（1 基）。空数组表示该文件没有书签。 */
const bookmarkLines = ref<number[]>([])

/** 书签 gutter 标记：一个主色小圆点。 */
class BookMark extends GutterMarker {
  override toDOM() {
    const dot = document.createElement('span')
    dot.className = 'nt-bookmark-dot'
    return dot
  }
}

const marker = new BookMark()

/**
 * 书签 gutter。
 *
 * `lineMarker` 按行号查表：gutter 只对可见行求值，几万行的文件也不会因为
 * 「给每一行都算一遍」而变慢。点圆点直接切书签——比「先定位再按 F2」少两步。
 */
function bookmarkGutter() {
  return gutter({
    class: 'nt-bookmark-gutter',
    lineMarker: (v, line) => {
      const n = v.state.doc.lineAt(line.from).number
      return bookmarkLines.value.includes(n) ? marker : null
    },
    domEventHandlers: {
      mousedown: (v, line) => {
        const n = v.state.doc.lineAt(line.from).number
        toggleBookmarkAt(n, v)
        return true
      },
    },
  })
}

/** 切换某一行的书签并重绘 gutter。 */
function toggleBookmarkAt(lineNo: number, v: EditorView) {
  const has = bookmarkLines.value.includes(lineNo)
  bookmarkLines.value = has
    ? bookmarkLines.value.filter((n) => n !== lineNo)
    : [...bookmarkLines.value, lineNo].sort((a, b) => a - b)
  // 重配一次：GutterMarker 的求值不会自己失效。
  v.dispatch({ effects: bookmarkComp.reconfigure(bookmarkGutter()) })
  void syncBookmarksToBackend(lineNo, !has)
}

/** 与后端同步：书签要跨会话保留，否则「标记一下明天看」就失效了。 */
async function syncBookmarksToBackend(lineNo: number, add: boolean) {
  if (!props.tab.path || props.tab.path.startsWith('draft:')) return
  try {
    if (add) {
      await api.addBookmark({ path: props.tab.path, line: lineNo })
    } else {
      const list = await api.listFileBookmarks(props.tab.path)
      const hit = list.find((b) => b.line === lineNo)
      if (hit) await api.removeBookmark(hit.id)
    }
    document.dispatchEvent(new CustomEvent('bookmarks-changed'))
  } catch {
    /* 同步失败不影响编辑：gutter 上已经显示出来了 */
  }
}

/** 把后端已有的书签读进 gutter（挂载与外部改书签后各调一次）。 */
async function loadBookmarks() {
  if (!props.tab.path || props.tab.path.startsWith('draft:')) return
  try {
    const list = await api.listFileBookmarks(props.tab.path)
    bookmarkLines.value = list.map((b) => b.line)
    view?.dispatch({ effects: bookmarkComp.reconfigure(bookmarkGutter()) })
  } catch {
    /* 读不出来就当没有书签 */
  }
}

/** 缩进单位：空格模式给 N 个空格，Tab 模式给 \t。 */
function indentText(): string {
  return cfg().insert_spaces ? ' '.repeat(cfg().tab_size) : '\t'
}

// ---- 主题 / 语言 / 字体 ----
const { themeExtensions, applyFont } = useEditorTheme(host)
const { syncExtensions, loadLanguage } = useEditorLanguage(computed(() => props.tab.language))

/** 行号点击选中整行；行号右键给出「在此行加书签」。 */
function onGutterClick(e: MouseEvent) {
  if (!view) return
  const target = e.target as HTMLElement
  const gutter = target.closest('.cm-lineNumbers')
  if (!gutter) return
  const el = target.closest('.cm-gutterElement') as HTMLElement | null
  if (!el) return
  const n = Number(el.textContent)
  if (!Number.isFinite(n) || n < 1) return
  const line = view.state.doc.line(Math.min(n, view.state.doc.lines))
  view.dispatch({
    selection: { anchor: line.from, head: line.to },
    scrollIntoView: true,
  })
  view.focus()
}

defineExpose({ gutterClick: onGutterClick })

function normalizeErr(e: unknown): string {
  if (typeof e === 'string') return e
  const o = e as { message?: string }
  return o?.message ?? tr('error.unknown', { reason: '' })
}

/**
 * 撤销 / 重做：
 * CodeMirror 的 historyKeymap 只绑了 Mod-z 与 Mod-Shift-z，Windows 用户习惯
 * Ctrl+Y 重做；另外 WebView 自身有「撤销整个文档编辑」的默认行为，
 * 不拦会让 Ctrl+Z 一次退掉整段输入。
 */
function onKeyFallback(e: KeyboardEvent) {
  if (!view || !view.hasFocus) return
  if (!(e.ctrlKey || e.metaKey)) return
  const key = e.key.toLowerCase()
  if (key === 'y') {
    e.preventDefault()
    redo(view)
  } else if (e.shiftKey && key === 'z') {
    e.preventDefault()
    redo(view)
  } else if (key === 'z') {
    e.preventDefault()
    undo(view)
  }
}

/**
 * 编辑器右键菜单。
 *
 * 为什么不复用 WebView 的原生菜单：它给的是「撤销 / 剪切 / 复制 / 粘贴」，
 * 与编辑器的实际能力对不上（我们有行操作、注释、书签、工具），
 * 而且样式与全站两套语言。
 */
function onContextMenu(e: MouseEvent) {
  e.preventDefault()
  menuItems.value = buildMenuItems()
  menu.value = { x: e.clientX, y: e.clientY }
}

/** 菜单条目：每次打开时现算（有选区 / 是否只读）。
 *  顺序就是使用频率：剪贴板 → 选中快操作 → 查找 → 注释与大小写 →
 *  行操作 → 书签行操作（低频，沉底）。 */
function buildMenuItems(): MenuItem[] {
  const v = viewOf(props.tab.id)
  const hasSel = v ? !v.state.selection.main.empty : false
  const ro = props.readonly || props.tab.read_only
  const id = props.tab.id
  return [
    {
      label: tr('menu.undo'),
      hint: 'Ctrl+Z',
      disabled: ro,
      run: () => doUndo(id),
    },
    {
      label: tr('menu.redo'),
      hint: 'Ctrl+Y',
      disabled: ro,
      run: () => doRedo(id),
    },
    { sep: true },
    {
      label: tr('menu.cut'),
      hint: 'Ctrl+X',
      disabled: ro || !hasSel,
      run: () => {
        const sel = getSelection(id)
        if (!sel) return
        void copyText(sel).then((ok) => {
          if (ok) doDeleteSelection(id)
        })
      },
    },
    {
      label: tr('menu.copy'),
      hint: 'Ctrl+C',
      disabled: !hasSel,
      run: () => {
        const sel = getSelection(id)
        if (!sel) return
        void copyText(sel).then((ok) => {
          if (ok) toast.ok(tr('common.copied'))
        })
      },
    },
    {
      label: tr('menu.paste'),
      hint: 'Ctrl+V',
      disabled: ro,
      run: async () => {
        const text = await readClipboard()
        if (!text) {
          toast.warn(tr('toast.clipboardEmpty'))
          return
        }
        commands.run('edit.paste')
      },
    },
    {
      label: tr('menu.selectAll'),
      hint: 'Ctrl+A',
      run: () => doSelectAll(id),
    },
    { sep: true },
    // 选中快操作：一步完成，结果原地替换选区。
    ...(hasSel
      ? ([
          {
            label: tr('quick.jsonFormat'),
            hint: 'Ctrl+Shift+J',
            disabled: ro,
            run: () => void quickJsonFormat(id),
          },
          {
            label: tr('quick.jsonMinify'),
            hint: 'Ctrl+Alt+J',
            disabled: ro,
            run: () => void quickJsonMinify(id),
          },
          {
            label: tr('quick.escape'),
            hint: 'Ctrl+Alt+E',
            disabled: ro,
            run: () => quickEscape(id),
          },
          {
            label: tr('quick.unescape'),
            hint: 'Ctrl+Alt+U',
            disabled: ro,
            run: () => quickUnescape(id),
          },
          {
            label: tr('quick.jsonToYaml'),
            disabled: ro,
            run: () => void quickConvert(id, 'json', 'yaml'),
          },
          {
            label: tr('quick.jsonToToml'),
            disabled: ro,
            run: () => void quickConvert(id, 'json', 'toml'),
          },
          {
            label: tr('quick.jsonToXml'),
            disabled: ro,
            run: () => void quickConvert(id, 'json', 'xml'),
          },
          {
            label: tr('quick.toConvert'),
            disabled: ro,
            run: () => commands.run('quick.toConvert'),
          },
          { sep: true },
        ] as MenuItem[])
      : []),
    {
      label: tr('menu.find'),
      hint: 'Ctrl+F',
      run: () => commands.run('edit.find'),
    },
    {
      label: tr('menu.replace'),
      hint: 'Ctrl+H',
      run: () => commands.run('edit.replace'),
    },
    {
      label: tr('menu.gotoLine'),
      hint: 'Ctrl+G',
      run: () => dialogs.open('gotoLine'),
    },
    { sep: true },
    {
      label: tr('menu.toggleComment'),
      hint: 'Ctrl+/',
      disabled: ro,
      run: () => commands.run('edit.toggleComment'),
    },
    {
      label: tr('menu.caseUpper'),
      hint: 'Ctrl+Shift+U',
      disabled: ro,
      run: () => {
        doCaseTransform(id, 'upper')
        toast.ok(tr('toast.caseDone'))
      },
    },
    {
      label: tr('menu.caseLower'),
      hint: 'Ctrl+Shift+G',
      disabled: ro,
      run: () => {
        doCaseTransform(id, 'lower')
        toast.ok(tr('toast.caseDone'))
      },
    },
    {
      label: tr('menu.caseTitle'),
      disabled: ro,
      run: () => {
        doCaseTransform(id, 'title')
        toast.ok(tr('toast.caseDone'))
      },
    },
    { sep: true },
    {
      label: tr('menu.duplicateLine'),
      hint: 'Ctrl+D',
      disabled: ro,
      run: () => doDuplicateLine(id),
    },
    {
      label: tr('menu.deleteLine'),
      hint: 'Ctrl+Shift+K',
      disabled: ro,
      run: () => doDeleteLine(id),
    },
    {
      label: tr('menu.moveLineUp'),
      hint: 'Alt+↑',
      disabled: ro,
      run: () => doMoveLine(id, 'up'),
    },
    {
      label: tr('menu.moveLineDown'),
      hint: 'Alt+↓',
      disabled: ro,
      run: () => doMoveLine(id, 'down'),
    },
    {
      label: tr('menu.selectLine'),
      run: () => doSelectLine(id),
    },
    { sep: true },
    {
      label: tr('menu.toggleBookmark'),
      hint: 'F2',
      disabled: !props.tab.path,
      run: () => commands.run('bookmark.toggle'),
    },
    { sep: true },
    {
      label: tr('menu.sortAsc'),
      disabled: ro,
      run: () => doSortLines(id, 'asc'),
    },
    {
      label: tr('menu.sortDesc'),
      disabled: ro,
      run: () => doSortLines(id, 'desc'),
    },
    {
      label: tr('menu.dedupLines'),
      disabled: ro,
      run: () => doDedupLines(id),
    },
    {
      label: tr('menu.reverseLines'),
      disabled: ro,
      run: () => doReverseLines(id),
    },
    {
      label: tr('menu.trimTrailing'),
      disabled: ro,
      run: () => doTrimTrailing(id),
    },
    {
      label: tr('menu.tabToSpaces'),
      disabled: ro,
      run: () => doConvertIndent(id, 'tab-to-space', cfg().tab_size, true),
    },
    {
      label: tr('menu.spacesToTabs'),
      disabled: ro,
      run: () => doConvertIndent(id, 'space-to-tab', cfg().tab_size, true),
    },
    { sep: true },
    {
      label: tr('menu.copyBookmarkLines'),
      disabled: bookmarkLines.value.length === 0,
      run: () => doBookmarkLineOp(id, 'copy', new Set(bookmarkLines.value)),
    },
    {
      label: tr('menu.cutBookmarkLines'),
      disabled: ro || bookmarkLines.value.length === 0,
      run: () => doBookmarkLineOp(id, 'cut', new Set(bookmarkLines.value)),
    },
    {
      label: tr('menu.deleteBookmarkLines'),
      disabled: ro || bookmarkLines.value.length === 0,
      run: () => doBookmarkLineOp(id, 'delete', new Set(bookmarkLines.value)),
    },
    {
      label: tr('menu.deleteUnmarkedLines'),
      disabled: ro || bookmarkLines.value.length === 0,
      run: () => doBookmarkLineOp(id, 'delete-unmarked', new Set(bookmarkLines.value)),
    },
    {
      label: tr('menu.pasteBookmarkLines'),
      disabled: ro || !bookmarkClip(),
      run: () => doPasteBookmarkLines(id),
    },
    { sep: true },
    {
      label: tr('menu.markSelection'),
      disabled: ro || !hasSel,
      run: () => doMarkSelection(id, currentMarkColor.value),
    },
    {
      label: tr('menu.clearMarks'),
      run: () => doClearMarks(),
    },
  ]
}

/** 当前标记颜色档：右键「标记」用它，5 色轮换。 */
const currentMarkColor = ref<MarkColor>(1)

/** 别处（命令层 / 侧栏）改了书签，这里跟着重绘。 */
function onBookmarksChanged() {
  void loadBookmarks()
}

onMounted(() => {
  document.addEventListener('bookmarks-changed', onBookmarksChanged)
  document.addEventListener('nt-marks-changed', onMarksChanged)
  host.value?.addEventListener('mousedown', onGutterClick)
  document.addEventListener('keydown', onKeyFallback, true)
})
onBeforeUnmount(() => {
  document.removeEventListener('bookmarks-changed', onBookmarksChanged)
  document.removeEventListener('nt-marks-changed', onMarksChanged)
  host.value?.removeEventListener('mousedown', onGutterClick)
  document.removeEventListener('keydown', onKeyFallback, true)
  if (marksRendererMine) setMarksRenderer(null)
})

/**
 * 标记变了重绘：装饰插件在 ViewPlugin 里，不订阅动作层的状态，
 * 所以这里把渲染函数注册进去，动作层改完集合后调它。
 * 分屏两个视图都要重绘：主视图注册，第二视图只被动响应。
 */
const marksRendererMine = computed(() => props.viewId !== 'second')

function onMarksChanged() {
  renderMarks()
}

if (marksRendererMine.value) {
  setMarksRenderer(() => {
    // 词集合整体换： decorations 依赖构造时传进来的快照，
    // 所以这里直接 reconfigure 一次 marksComp。
    view?.dispatch({ effects: marksComp.reconfigure(markExtension(allMarkWords())) })
  })
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

.nt-code-editor :deep(.cm-editor.cm-focused) {
  outline: none;
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

/* 行号可点：悬停给一点反馈，用户才知道能点 */
.nt-code-editor :deep(.cm-lineNumbers .cm-gutterElement) {
  cursor: pointer;
  transition: color var(--nt-dur-fast) var(--nt-ease);
}

.nt-code-editor :deep(.cm-lineNumbers .cm-gutterElement:hover) {
  color: var(--nt-primary);
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

.nt-code-editor :deep(.nt-bookmark-gutter) {
  width: 12px;
  cursor: pointer;
}

.nt-code-editor :deep(.nt-bookmark-dot) {
  display: inline-block;
  width: 7px;
  height: 7px;
  margin-left: 2px;
  border-radius: 50%;
  background: var(--nt-primary);
}

.nt-code-editor :deep(.cm-cursor) {
  border-left-color: var(--nt-editor-cursor);
}

/* 只读视图：光标不闪，但保留选中 */
.nt-code-editor :deep(.cm-readOnly .cm-cursor) {
  display: none;
}

/* 挂载失败：标签还在，但要画清「为什么是空的」 */
.nt-mount-error {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--nt-sp-2);
  height: 100%;
  padding: var(--nt-sp-6);
}

.nt-mount-error-title {
  font-size: var(--nt-fs-md);
  font-weight: 600;
  color: var(--nt-ink-2);
}

.nt-mount-error-detail {
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-muted);
  max-width: 480px;
  text-align: center;
  word-break: break-all;
}

/* 选区浮条：贴选区上方的一步操作条。出现 / 收起都要有过渡，
   突然弹出会打断选词的视线。 */
.nt-sel-bar {
  position: fixed;
  z-index: var(--nt-z-pop);
  display: flex;
  gap: 2px;
  padding: 3px;
  background: var(--nt-elev-raised);
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius-sm);
  box-shadow: var(--nt-card-shadow);
  animation: nt-sel-bar-in var(--nt-dur-fast) var(--nt-ease);
}

@keyframes nt-sel-bar-in {
  from {
    opacity: 0;
    transform: translateY(3px);
  }
}

.nt-sel-bar-btn {
  height: var(--nt-ctl-h-sm);
  padding: 0 var(--nt-sp-2);
  border-radius: var(--nt-radius-xs);
  font-size: var(--nt-fs-label);
  color: var(--nt-ink-2);
  white-space: nowrap;
  transition:
    background-color var(--nt-dur-fast) var(--nt-ease),
    color var(--nt-dur-fast) var(--nt-ease),
    transform var(--nt-dur-fast) var(--nt-ease);
}

.nt-sel-bar-btn:hover {
  color: var(--nt-ink);
  background: var(--nt-surface-hover);
  transform: scale(1.06);
}

.nt-sel-bar-btn:active {
  transform: scale(0.92);
}
</style>
