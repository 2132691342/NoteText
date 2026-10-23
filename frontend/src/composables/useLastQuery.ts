// 上次查找词：F3 / Shift+F3 复用，关掉查找浮窗后照样能跳下一个。
// 放在独立模块而不是 FindDialog 的 ref：浮窗一卸载 ref 就没了，

import { ref } from 'vue'
import { doSelectRange } from '@/composables/useEditorActions'
import { focusedTabId, get } from '@/composables/useEditorRegistry'
import { useEditorStore } from '@/stores/editor'
import { useToastStore } from '@/stores/toast'
import { tr } from '@/i18n'

/** 上次查找用的查询词与选项。空串 = 从没查过。 */
const last = ref({ query: '', matchCase: false, wholeWord: false, regex: false })

/** 查找浮窗每次扫描时调：把当前条件记下来。 */
export function rememberQuery(q: {
  query: string
  matchCase?: boolean
  wholeWord?: boolean
  regex?: boolean
}) {
  last.value = {
    query: q.query,
    matchCase: !!q.matchCase,
    wholeWord: !!q.wholeWord,
    regex: !!q.regex,
  }
}

/** 读上次查询条件（查找浮窗打开时预填用）。 */
export function lastQueryState(): {
  query: string
  matchCase: boolean
  wholeWord: boolean
  regex: boolean
} {
  return { ...last.value }
}

/**
 * 按方向跳一个命中。
 *
 * 从当前光标之后（或之前）找第一个匹配：不维护一份命中表，
 * 因为正文随时在变，缓存的偏移会失效；一次线性扫描足够快。
 * 到头没找到就循环到另一端——用户按 F3 时不会想去记「这是第几圈」。
 */
export function stepLastQuery(dir: 1 | -1) {
  const q = last.value.query
  if (!q) {
    useToastStore().warn(tr('toast.noLastQuery'))
    return
  }
  // 焦点兜底：F3 按下时焦点可能在浮窗上，退回活跃标签。
  const tabId = focusedTabId() ?? useEditorStore().activeId
  const view = tabId != null ? get(tabId) : undefined
  if (!view) return

  const text = view.state.doc.toString()
  const cursor = view.state.selection.main.head
  let src = q
  if (!last.value.regex) src = q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  if (last.value.wholeWord) src = `\\b(?:${src})\\b`
  const re = new RegExp(src, last.value.matchCase ? 'g' : 'gi')

  let hit: [number, number] | null = null
  let m: RegExpExecArray | null
  let guard = 0
  while ((m = re.exec(text)) !== null && guard < 100000) {
    guard += 1
    if (m[0] === '') {
      re.lastIndex += 1
      continue
    }
    if (dir === 1 ? m.index > cursor : m.index + m[0].length - 1 < cursor) {
      hit = [m.index, m.index + m[0].length]
      break
    }
  }
  if (!hit) {
    re.lastIndex = 0
    while ((m = re.exec(text)) !== null && guard < 200000) {
      guard += 1
      if (m[0] === '') {
        re.lastIndex += 1
        continue
      }
      hit = [m.index, m.index + m[0].length]
      break
    }
  }
  if (!hit) {
    useToastStore().warn(tr('dialog.noMatches'))
    return
  }
  doSelectRange(tabId, hit[0], hit[1])
}
