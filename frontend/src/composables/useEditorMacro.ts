// 宏录制与回放：把一串编辑动作收成 MacroStep，再按序派发回去。
// 权威源在前端：回放要驱动编辑器命令与文档变更，后端只做持久层。
import { ref } from 'vue'
import type { EditorView } from '@codemirror/view'
import type { MacroStep } from '@/types/api'

/** 录制状态：状态栏读它显示「录制中 · N 步」。 */
export const recording = ref(false)
export const steps = ref<MacroStep[]>([])

/** 回放中：此时录制的变更不能再进步骤表，否则自己录自己。 */
export const playing = ref(false)

/** 开始录制。 */
export function startRecording() {
  steps.value = []
  recording.value = true
}

/** 停止录制，返回收到的步骤。 */
export function stopRecording(): MacroStep[] {
  recording.value = false
  return steps.value
}

/** 录制中收到一次文档变更：插入文本收成 Text 步骤。 */
export function recordDocChange(_view: EditorView, inserted: string) {
  if (!recording.value || playing.value || !inserted) return
  const prev = steps.value[steps.value.length - 1]
  // 连续输入合并成一个 Text：否则打一行字会录出几十步，回放也慢。
  if (prev && prev.kind === 'text') {
    prev.text += inserted
    return
  }
  steps.value.push({ kind: 'text', text: inserted })
}

/** 录制中收到一次命令：收成 Key 步骤。 */
export function recordCommand(command: string) {
  if (!recording.value || playing.value) return
  steps.value.push({ kind: 'key', command })
}

/**
 * 回放一串步骤。
 *
 * `key` 步骤交给回调执行（命令表在 useCommands 里，这里不 import 它——
 * 那会造成 composable 之间的循环依赖）。
 */
export async function playSteps(
  view: EditorView,
  list: MacroStep[],
  runCommand: (id: string) => void,
  delayMs = 40,
): Promise<void> {
  if (playing.value) return
  playing.value = true
  try {
    for (const step of list) {
      if (step.kind === 'text') {
        const { from, to } = view.state.selection.main
        view.dispatch({
          changes: { from, to, insert: step.text },
          selection: { anchor: from + step.text.length },
          scrollIntoView: true,
        })
      } else if (step.kind === 'key') {
        runCommand(step.command)
      }
      // pause：让用户自己输入，回放到此为止。
      else break
      if (delayMs > 0) await new Promise((r) => setTimeout(r, delayMs))
    }
  } finally {
    playing.value = false
  }
}

/** 回放后刷新录制基线：回放产生的变更不该算「用户改了」。 */
export function resetRecording() {
  steps.value = []
  recording.value = false
}
