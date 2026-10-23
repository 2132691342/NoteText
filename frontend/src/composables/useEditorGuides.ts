// 编辑器视觉辅助：缩进参考线 / 括号配对着色 / 行尾标记。
// 三个都是「看法」不是「内容」：全部用装饰与伪元素实现，

import { ViewPlugin, Decoration, WidgetType, type DecorationSet, type PluginValue, EditorView } from '@codemirror/view'
import { RangeSetBuilder, type Extension } from '@codemirror/state'
import { syntaxTree } from '@codemirror/language'

/** 缩进参考线的栅格：每 8 列一条，5 档颜色循环。 */
const GUIDE_COL = 8

/**
 * 缩进参考线。
 *
 * 只画「有内容行」的缩进宽度：空行画线会让一片空白里横着几条线，
 * 读起来像表格而不是代码。深度上限 5 档，再深用第 5 档循环——
 * 档位太多颜色就分不出来了。
 */
function indentGuides(): Extension {
  return ViewPlugin.fromClass(
    class implements PluginValue {
      decorations: DecorationSet

      constructor(view: EditorView) {
        this.decorations = this.build(view)
      }

      update(u: import('@codemirror/view').ViewUpdate) {
        if (u.docChanged || u.viewportChanged) this.decorations = this.build(u.view)
      }

      private build(view: EditorView): DecorationSet {
        const b = new RangeSetBuilder<Decoration>()
        for (const { from, to } of view.visibleRanges) {
          let pos = from
          while (pos <= to) {
            const line = view.state.doc.lineAt(pos)
            if (line.text.trim()) {
              const lead = /^[ \t]*/.exec(line.text)?.[0].length ?? 0
              if (lead > 0) {
                for (let col = GUIDE_COL; col <= lead; col += GUIDE_COL) {
                  const at = line.from + col
                  if (at > line.to) break
                  // 零宽标记挂一个空 span：border-left 画线，不占宽度也不遮光标。
                  b.add(at, at, guideAt((col / GUIDE_COL - 1) % 5))
                }
              }
            }
            pos = line.to + 1
          }
        }
        return b.finish()
      }
    },
    { decorations: (v) => v.decorations },
  )
}

/** 一列缩进的参考线标记：类名挂在装饰上，颜色在 content.css。 */
function guideAt(depth: number): Decoration {
  // widget 而不是 replace：replace 会把那一列的空格吃掉，正文宽度就变了。
  // side: -1 让线落在该字符之前，视觉上就是「这一列左边有一条线」。
  void depth
  return Decoration.widget({ widget: new GuideLine(), side: -1 })
}

class GuideLine extends WidgetType {
  override toDOM(): HTMLElement {
    const span = document.createElement('span')
    span.className = 'nt-indent-guide'
    span.setAttribute('aria-hidden', 'true')
    return span
  }
  /** 线都一样：同类挂件复用 DOM，不必重画。 */
  override eq(): boolean {
    return true
  }
  /** 不占高度：只是 1px 竖线，不该把行高顶开。 */
  override get estimatedHeight(): number {
    return -1
  }
}

/**
 * 括号配对着色（rainbow brackets）。
 *
 * 用「可见区栈扫描」而不是逐个问语法树：一次 O(视口) 的线性扫描就能
 * 给每一对括号定出嵌套深度，比「对每个括号都去找另一半」快一个量级。
 *
 * 字符串与注释里的括号不计入：栈扫描分不清 `"("` 与 `(`，
 * 所以先用语法树把这些区间挖掉，扫描时跳过。
 */
function bracketPairColors(): Extension {
  return ViewPlugin.fromClass(
    class implements PluginValue {
      decorations: DecorationSet

      constructor(view: EditorView) {
        this.decorations = this.build(view)
      }

      update(u: import('@codemirror/view').ViewUpdate) {
        if (u.docChanged || u.viewportChanged) this.decorations = this.build(u.view)
      }

      private build(view: EditorView): DecorationSet {
        const b = new RangeSetBuilder<Decoration>()
        const OPEN: Record<string, string> = { '(': ')', '[': ']', '{': '}' }
        const CLOSE = new Set([')', ']', '}'])
        const skip = skippedRanges(view)

        for (const { from, to } of view.visibleRanges) {
          const text = view.state.doc.sliceString(from, to)
          const stack: Array<{ pos: number; depth: number }> = []
          for (let i = 0; i < text.length; i += 1) {
            const abs = from + i
            if (isSkipped(skip, abs)) continue
            const c = text[i]
            if (OPEN[c]) {
              stack.push({ pos: abs, depth: stack.length })
              continue
            }
            if (CLOSE.has(c)) {
              const open = stack.pop()
              if (open) {
                const color = (open.depth % 5) + 1
                b.add(open.pos, open.pos + 1, bracketDeco(color))
                b.add(abs, abs + 1, bracketDeco(color))
              }
            }
          }
        }
        return b.finish()
      }
    },
    { decorations: (v) => v.decorations },
  )
}

function bracketDeco(color: number): Decoration {
  return Decoration.mark({ class: `nt-bracket nt-bracket-${color}` })
}

/**
 * 语法树里「括号不当括号看」的区间：字符串 / 注释 / 模板字符串。
 *
 * 区间可能很大（一整个多行字符串），所以合并相邻区间再交给扫描跳过。
 */
function skippedRanges(view: EditorView): Array<[number, number]> {
  const out: Array<[number, number]> = []
  const push = (from: number, to: number) => {
    if (to <= from) return
    const last = out[out.length - 1]
    if (last && last[1] >= from) last[1] = Math.max(last[1], to)
    else out.push([from, to])
  }
  for (const { from, to } of view.visibleRanges) {
    syntaxTree(view.state).iterate({
      from,
      to,
      enter(node) {
        const n = node.name
        if (n === 'String' || n === 'Comment' || n === 'BlockComment' || n === 'LineComment' || n === 'TemplateString') {
          push(node.from, node.to)
        }
        // 模板串里的 ${...} 是表达式、括号要算，所以不整体跳过 TemplateString；
        // 代价是模板串里的表达式括号不着色，观感好过整体挖空。
      },
    })
  }
  return out
}

function isSkipped(ranges: Array<[number, number]>, pos: number): boolean {
  for (const [a, b] of ranges) {
    if (pos < a) return false
    if (pos <= b) return true
  }
  return false
}

/**
 * 行尾标记：在每行末尾插一个 ↵ 挂件。
 *
 * 用挂件而不是 `.cm-line::after`：开了自动换行后一篇落会被拆成多个
 * `.cm-line`，伪元素会出现在段落中间，而行尾只有一个物理换行点。
 */
function eolMarkers(): Extension {
  return ViewPlugin.fromClass(
    class implements PluginValue {
      decorations: DecorationSet

      constructor(view: EditorView) {
        this.decorations = this.build(view)
      }

      update(u: import('@codemirror/view').ViewUpdate) {
        if (u.docChanged || u.viewportChanged) this.decorations = this.build(u.view)
      }

      private build(view: EditorView): DecorationSet {
        const b = new RangeSetBuilder<Decoration>()
        for (const { from, to } of view.visibleRanges) {
          let pos = from
          while (pos <= to) {
            const line = view.state.doc.lineAt(pos)
            b.add(line.to, line.to, eolWidget())
            pos = line.to + 1
          }
        }
        return b.finish()
      }
    },
    { decorations: (v) => v.decorations },
  )
}

class EolMarker extends WidgetType {
  override toDOM(): HTMLElement {
    const span = document.createElement('span')
    span.className = 'nt-eol-mark'
    span.textContent = '↵'
    span.setAttribute('aria-hidden', 'true')
    return span
  }
  /** 标记内容固定：同类的挂件直接复用 DOM，不必重画。 */
  override eq(): boolean {
    return true
  }
}

function eolWidget(): Decoration {
  return Decoration.widget({ widget: new EolMarker(), side: 1 })
}

/** 三个视觉辅助的开关：对应 EditorConfig 里的三个布尔项。 */
export interface GuideFlags {
  indent_guides: boolean
  bracket_pair_colors: boolean
  show_eol: boolean
}

export function guideExtensions(flags: GuideFlags): Extension[] {
  const out: Extension[] = []
  if (flags.indent_guides) out.push(indentGuides())
  if (flags.bracket_pair_colors) out.push(bracketPairColors())
  if (flags.show_eol) out.push(eolMarkers())
  return out
}
