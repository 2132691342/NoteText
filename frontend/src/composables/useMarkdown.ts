// Markdown 渲染：markdown-it + 代码高亮 + Mermaid + KaTeX。
//
// 三件事分开做，各自可以单独失败：
// - **代码块高亮**（highlight.js）：同步，失败退化成无高亮；
// - **Mermaid 图表**：异步渲染成 SVG，失败回退显示源码；
// - **KaTeX 公式**：只处理文本节点，跳过代码块（里面的 `$` 是内容不是公式）。
//
// 全部动态 import：只写纯文本的人不该付 Mermaid + KaTeX 的加载代价。
// 实例是模块级单例：切标签不重建，`markdown-it` 的插件注册是一次性的。

import type { Ref } from 'vue'

/** 主题色给 Mermaid 用：深浅两套，切换主题时重新 initialize。 */
let mermaidTheme: 'dark' | 'default' | null = null

/** Mermaid 渲染轮次：内容变化开新一轮，旧轮次的结果不写回 DOM。 */
let mermaidRunToken = 0

/** markdown-it 实例（惰性建）。 */
type MarkdownItCtor = typeof import('markdown-it').default
let md: InstanceType<MarkdownItCtor> | null = null
/** KaTeX 是否已加载过样式。 */
let katexCssLoaded = false

/** 当前是否深色主题。 */
let isDark = false

export interface MarkdownRender {
  /** 当前是否深色（供 mermaid 初始化读）。 */
  setDark: (dark: boolean) => void
  /** 把 Markdown 源文本渲染成 HTML 字符串。 */
  renderHtml: (text: string) => Promise<string>
  /** 渲染容器里的 Mermaid 图（在 DOM 更新后调）。 */
  renderDiagrams: (root: HTMLElement | null) => Promise<void>
  /** 渲染容器里的公式。 */
  renderMath: (root: HTMLElement | null) => void
}

/**
 * 建一个 Markdown 渲染器。
 *
 * `textRef` 只在「内容变了要重渲染」时用，这里不持有它——渲染入口
 * 是 `renderHtml`，调用方（编辑器组件）自己决定何时调。
 */
export function useMarkdown(): MarkdownRender {
  /** 惰性建 markdown-it：首次用到才加载模块与插件。 */
  async function ensureMd(): Promise<NonNullable<typeof md>> {
    if (md) return md
    const [{ default: MarkdownIt }, { default: hljs }] = await Promise.all([
      import('markdown-it'),
      import('highlight.js'),
    ])
    const instance = new MarkdownIt({
      html: false,
      linkify: true,
      // 代码块语言未知时不高亮而不是报错：用户写 ```foo 也能看。
      highlight(code, lang) {
        if (lang && hljs.getLanguage(lang)) {
          try {
            return hljs.highlight(code, { language: lang, ignoreIllegals: true }).value
          } catch {
            /* 高亮失败退化成转义后的原文，不抛 */
          }
        }
        return ''
      },
    })
    // 代码块外包一层：mermaid 块要能被 querySelector 找到，所以要带 class。
    const defaultFence = instance.renderer.rules.fence!.bind(instance.renderer.rules)
    instance.renderer.rules.fence = (tokens, idx, options, _env, self) => {
      const token = tokens[idx]
      const info = token.info ? token.info.trim().split(/\s+/)[0] : ''
      if (info === 'mermaid') {
        // 内容原样放进容器：由 renderDiagrams 异步换成 SVG。
        return `<div class="nt-md-mermaid" data-source="${escapeAttr(token.content)}"></div>`
      }
      const html = defaultFence(tokens, idx, options, _env, self)
      return info ? html.replace(/^<pre>/, `<pre class="nt-md-pre" data-lang="${escapeAttr(info)}">`) : html
    }
    md = instance
    return instance
  }

  function setDark(dark: boolean) {
    isDark = dark
  }

  async function renderHtml(text: string): Promise<string> {
    const instance = await ensureMd()
    const html = instance.render(text)
    // DOMPurify 在浏览器里才可用；SSR / 测试环境退化成不净化。
    const [{ default: DOMPurify }] = await Promise.all([import('dompurify')])
    return DOMPurify.sanitize(html, { ADD_ATTR: ['data-source', 'data-lang', 'target'] })
  }

  /** 渲染容器里的 Mermaid 图。幂等：已渲染过的不再动。 */
  async function renderDiagrams(root: HTMLElement | null) {
    if (!root) return
    const nodes = Array.from(root.querySelectorAll<HTMLElement>('.nt-md-mermaid'))
    if (nodes.length === 0) return
    const [{ default: mermaid }, { default: DOMPurify }] = await Promise.all([
      import('mermaid'),
      import('dompurify'),
    ])
    const theme = isDark ? 'dark' : 'default'
    // 主题变了要重新 initialize：mermaid 的配置是全局的。
    if (mermaidTheme !== theme) {
      mermaid.initialize({
        startOnLoad: false,
        theme,
        securityLevel: 'strict',
        fontFamily: 'var(--font-sans)',
      })
      mermaidTheme = theme
    }
    const token = ++mermaidRunToken
    for (const node of nodes) {
      if (node.dataset.processed === '1') continue
      const source = unescapeAttr(node.dataset.source ?? '')
      if (!source.trim()) continue
      try {
        const { svg } = await mermaid.render(`nt-md-mermaid-${mermaidRunToken}-${Math.random().toString(36).slice(2, 7)}`, source)
        // 渲染期间内容可能又变了：旧轮次的结果不写回，否则会把新文本的图覆盖掉。
        if (token !== mermaidRunToken || !node.isConnected) continue
        node.innerHTML = DOMPurify.sanitize(svg, { USE_PROFILES: { svg: true, svgFilters: true } })
        node.dataset.processed = '1'
      } catch {
        // 图渲染失败：退回源码。比留一块空白好——用户能看出是哪句写错了。
        if (token !== mermaidRunToken || !node.isConnected) continue
        node.textContent = source
        node.classList.add('is-failed')
        node.dataset.processed = '1'
      }
    }
  }

  /** 渲染容器里的公式：只走文本节点，代码块里的 `$` 不当公式。 */
  function renderMath(root: HTMLElement | null) {
    if (!root) return
    if (!katexCssLoaded) {
      void import('katex/dist/katex.min.css')
      katexCssLoaded = true
    }
    void import('katex').then(({ default: katex }) => {
      if (!root) return
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
      const targets: Text[] = []
      while (walker.nextNode()) {
        const n = walker.currentNode as Text
        // 跳过 pre / code 里的文本：代码块内的 $ 是内容。
        if (n.parentElement?.closest('pre, code, .nt-md-mermaid')) continue
        if (/\$[^$]+\$/.test(n.data)) targets.push(n)
      }
      for (const node of targets) {
        const frag = renderInlineMath(node.data, katex)
        if (frag) node.parentNode?.replaceChild(frag, node)
      }
    })
  }

  return { setDark, renderHtml, renderDiagrams, renderMath }
}

/** 把一段文本里的 `$...$` 换成 KaTeX 渲染结果。没有公式时返回 null。 */
type KatexModule = typeof import('katex')

function renderInlineMath(text: string, katex: KatexModule): DocumentFragment | null {
  const re = /\$([^$\n]+)\$/g
  let last = 0
  let m: RegExpExecArray | null
  let frag: DocumentFragment | null = null
  while ((m = re.exec(text))) {
    frag ??= document.createDocumentFragment()
    frag.append(document.createTextNode(text.slice(last, m.index)))
    const span = document.createElement('span')
    span.className = 'nt-md-math'
    try {
      katex.render(m[1], span, { throwOnError: false, displayMode: false })
    } catch {
      span.textContent = m[0]
    }
    frag.append(span)
    last = m.index + m[0].length
  }
  if (!frag) return null
  frag.append(document.createTextNode(text.slice(last)))
  return frag
}

function escapeAttr(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

function unescapeAttr(s: string): string {
  return s
    .replace(/&quot;/g, '"')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&')
}

/** 供外部重置（测试用）：清掉单例，下次重新建。 */
export function __resetMarkdownForTest() {
  md = null
  mermaidTheme = null
  katexCssLoaded = false
}

/** 主题引用：测试与调试用。 */
export function __markdownState(): { isDark: boolean } {
  return { isDark }
}

/** 占位：让 Ref 的类型被引用（将来若要把 textRef 传进来）。 */
export type MarkdownTextRef = Ref<string>
