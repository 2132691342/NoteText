// 编辑器语言：按 language id 给 CodeMirror 的语言扩展。
//
// 语言包按需动态 import：30 种语言全静态引入会把首屏撑大几 MB，
// 而用户一次只开一两种语言的文件。
import { type Ref } from 'vue'
import type { Extension } from '@codemirror/state'
import { LanguageDescription, StreamLanguage } from '@codemirror/language'
import { javascript } from '@codemirror/lang-javascript'
import { json } from '@codemirror/lang-json'
import { html } from '@codemirror/lang-html'
import { css } from '@codemirror/lang-css'
import { xml } from '@codemirror/lang-xml'
import { markdown } from '@codemirror/lang-markdown'
import { sql } from '@codemirror/lang-sql'
import { yaml } from '@codemirror/lang-yaml'

/** 静态引入的几种：常见且体积小。 */
const STATIC: Record<string, () => Extension> = {
  javascript: () => javascript(),
  jsx: () => javascript({ jsx: true }),
  typescript: () => javascript({ typescript: true }),
  tsx: () => javascript({ typescript: true, jsx: true }),
  json: () => json(),
  html: () => html(),
  xml: () => xml(),
  css: () => css(),
  scss: () => css(),
  markdown: () => markdown(),
  sql: () => sql(),
  yaml: () => yaml(),
  vue: () => html(),
}

/** 动态引入的几种：语言包较大，用的时候才加载。 */
const DYNAMIC: Record<string, () => Promise<unknown>> = {
  rust: () => import('@codemirror/lang-rust'),
  go: () => import('@codemirror/lang-go'),
  java: () => import('@codemirror/lang-java'),
  python: () => import('@codemirror/lang-python'),
  cpp: () => import('@codemirror/lang-cpp'),
  php: () => import('@codemirror/lang-php'),
  lua: () => import('@codemirror/legacy-modes/mode/lua'),
}

/** legacy modes（无官方 lang 包的）：走 StreamLanguage。 */
// csharp / ruby / kotlin 没有官方 lang 包，走 legacy modes。
const LEGACY: Record<string, () => Promise<unknown>> = {
  csharp: () => import('@codemirror/legacy-modes/mode/clike'),
  ruby: () => import('@codemirror/legacy-modes/mode/ruby'),
  kotlin: () => import('@codemirror/legacy-modes/mode/clike'),
  shell: () => import('@codemirror/legacy-modes/mode/shell'),
  powershell: () => import('@codemirror/legacy-modes/mode/powershell'),
  ini: () => import('@codemirror/legacy-modes/mode/properties'),
  toml: () => import('@codemirror/legacy-modes/mode/toml'),
  diff: () => import('@codemirror/legacy-modes/mode/diff'),
}

const cache = new Map<string, Extension>()

export function useEditorLanguage(lang: Ref<string>) {
  /** 取语言扩展：命中缓存直接返回，否则动态加载后缓存。 */
  async function languageExtensions(): Promise<Extension[]> {
    const id = lang.value
    if (id === 'text' || !id) return []

    const hit = cache.get(id)
    if (hit) return [hit]

    // 1) 静态包
    const s = STATIC[id]
    if (s) {
      const ext = s()
      cache.set(id, ext)
      return [ext]
    }

    // 2) 官方 lang 包（异步）
    const d = DYNAMIC[id]
    if (d) {
      try {
        const mod = (await d()) as Record<string, unknown>
        const fn = mod[id] ?? Object.values(mod).find((v) => typeof v === 'function')
        if (typeof fn === 'function') {
          const ext = (fn as () => Extension)()
          cache.set(id, ext)
          return [ext]
        }
      } catch {
        /* 包没装上：回落纯文本，不阻塞打开 */
      }
      return []
    }

    // 3) legacy mode
    const l = LEGACY[id]
    if (l) {
      try {
        const mod = (await l()) as Record<string, unknown>
        const mode = mod.default ?? Object.values(mod)[0]
        if (mode) {
          const ext = StreamLanguage.define(mode as never)
          cache.set(id, ext)
          return [ext]
        }
      } catch {
        /* 同上 */
      }
      return []
    }

    return []
  }

  /** 同步首屏用：已知的静态语言立即返回，其余的等异步补上。 */
  function syncExtensions(): Extension[] {
    const id = lang.value
    const hit = cache.get(id)
    if (hit) return [hit]
    const s = STATIC[id]
    if (s) {
      const ext = s()
      cache.set(id, ext)
      return [ext]
    }
    return []
  }

  return { syncExtensions, loadLanguage: languageExtensions }
}

// 供 CodeEditor 的首屏 extensions 用（同步部分）
export function languageExtensions(id: string): Extension[] {
  if (id === 'text' || !id) return []
  const hit = cache.get(id)
  if (hit) return [hit]
  const s = STATIC[id]
  if (s) {
    const ext = s()
    cache.set(id, ext)
    return [ext]
  }
  return []
}

/** CodeMirror 的自动语言检测描述：给 autocomplete 的语义词用。 */
export const LANGUAGE_DESCRIPTIONS = [
  LanguageDescription.of({ name: 'JavaScript', extensions: ['js', 'mjs'], load: async () => javascript() }),
  LanguageDescription.of({ name: 'JSON', extensions: ['json'], load: async () => json() }),
]
