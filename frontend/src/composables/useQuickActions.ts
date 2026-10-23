// 一键快操作：选中即用，结果原地替换选区（无选区时替换全文）。
// 走浮窗要「打开 → 执行 → 确认 → 应用」四步，「把这段 JSON 排版」是一步的事。
import { api, normalizeError } from '@/api'
import { tr } from '@/i18n'
import { get } from '@/composables/useEditorRegistry'
import { useToastStore } from '@/stores/toast'
import {
  convertText,
  escapeText,
  unescapeText,
  type ConvertFormat,
} from '@/utils/formatConvert'

/** 取操作目标：选区优先，没选区取全文。返回空串表示没有可操作的正文。 */
function target(tabId: number | null | undefined): { from: number; to: number; text: string } | null {
  if (tabId == null) return null
  const v = get(tabId)
  if (!v) return null
  const { from, to } = v.state.selection.main
  if (from < to) {
    return { from, to, text: v.state.sliceDoc(from, to) }
  }
  const len = v.state.doc.length
  if (len === 0) return null
  return { from: 0, to: len, text: v.state.doc.toString() }
}

/** 把结果写回原区间，光标收在结果尾部。
 *  不选中新文本：替换后若仍保持选中，选区浮条会立刻再弹一次。 */
function apply(tabId: number | null | undefined, t: { from: number; to: number }, out: string) {
  if (tabId == null) return
  const v = get(tabId)
  if (!v) return
  v.dispatch({
    changes: { from: t.from, to: t.to, insert: out },
    selection: { anchor: t.from + out.length },
  })
  v.focus()
  useToastStore().ok(tr('toast.quickDone'))
}

/**
 * JSON 格式化 / 压缩：走后端拿行列级错误提示；后端不可用（浏览器预览态）
 * 时退回前端实现。两个实现的输出必须一致：JSON.stringify(,,indent)。
 */
async function jsonTransform(text: string, minify: boolean, indent: number): Promise<string> {
  try {
    const r = await api.jsonFormat({
      text,
      format: minify ? 'minify' : 'pretty',
      indent,
      sort_keys: false,
    })
    if (!r.ok) {
      throw new Error(
        tr('error.jsonErrorAtLine', { reason: r.error_message ?? '', line: r.error_line ?? 0 }),
      )
    }
    return r.output
  } catch (e) {
    if (normalizeError(e).code !== 'no-backend') throw e
    const data = JSON.parse(text)
    return minify ? JSON.stringify(data) : JSON.stringify(data, null, indent)
  }
}

/** JSON 格式化：选中优先，错误报出行列。 */
export async function quickJsonFormat(tabId: number | null | undefined, indent = 2): Promise<void> {
  const t = target(tabId)
  if (!t) return
  try {
    apply(tabId, t, await jsonTransform(t.text, false, indent))
  } catch (e) {
    reportError(e)
  }
}

/** JSON 压缩成单行。 */
export async function quickJsonMinify(tabId: number | null | undefined): Promise<void> {
  const t = target(tabId)
  if (!t) return
  try {
    apply(tabId, t, await jsonTransform(t.text, true, 0))
  } catch (e) {
    reportError(e)
  }
}

/** 转换类失败的统一出口：JSON 报行列，其余报原因。 */
function reportError(e: unknown) {
  useToastStore().bad(e instanceof Error ? e.message : normalizeError(e).message)
}

/** 加转义：正文 → 字符串字面量内容。 */
export function quickEscape(tabId: number | null | undefined): void {
  const t = target(tabId)
  if (!t) return
  apply(tabId, t, escapeText(t.text))
}

/** 去转义：字符串字面量内容 → 真实字符。非法序列给出人话提示。 */
export function quickUnescape(tabId: number | null | undefined): void {
  const t = target(tabId)
  if (!t) return
  try {
    apply(tabId, t, unescapeText(t.text))
  } catch {
    useToastStore().warn(tr('toast.badEscapes'))
  }
}

/** 大小写转换（浮条用）：选区原地转换，光标收尾不保持选中——
 *  保持选中的话浮条会立刻再弹一次，按一下弹一下。 */
export function quickCase(
  tabId: number | null | undefined,
  kind: 'upper' | 'lower',
): void {
  if (tabId == null) return
  const v = get(tabId)
  if (!v) return
  const { from, to } = v.state.selection.main
  if (from >= to) return
  const text = v.state.sliceDoc(from, to)
  const out = kind === 'upper' ? text.toUpperCase() : text.toLowerCase()
  if (out === text) return
  apply(tabId, { from, to }, out)
}

/** 把选中内容当作 from 格式转成 to，原地替换。 */
export async function quickConvert(
  tabId: number | null | undefined,
  from: ConvertFormat,
  to: ConvertFormat,
): Promise<void> {
  const t = target(tabId)
  if (!t) return
  try {
    apply(tabId, t, await convertText(t.text, from, to))
  } catch (e) {
    useToastStore().bad(tr('convert.parseFailed', { reason: (e as Error).message }))
  }
}
