// 文件拖放：外部拖入走 WebView 原生 drop 事件（dragDropEnabled=false 是为了
// 保住页内 HTML5 拖拽），路径取 File.path，拿不到就降级读文本开无路径标签。
import { onMounted, onUnmounted } from 'vue'
import { useEditorStore } from '@/stores/editor'
import { useFileTreeStore } from '@/stores/fileTree'
import { useToastStore } from '@/stores/toast'
import { i18n } from '@/i18n'
import { api, isDesktop, normalizeError } from '@/api'
import { baseName } from '@/utils'
import { isTextKind, kindOf } from '@/utils/fileKind'
import { doReplaceDoc } from '@/composables/useEditorActions'
import { commitBaseline, get } from '@/composables/useEditorRegistry'

/** 降级读文本的上限，与后端 read_file 的 20MB 一致。 */
const MAX_DROP_BYTES = 20 * 1024 * 1024
/** 超限提示用：文案里的数字要和这个值一致。 */
const MAX_DROP_MB = 20

export function useFileDrop() {
  const editor = useEditorStore()
  const tree = useFileTreeStore()
  const toast = useToastStore()
  const t = (k: string, n: Record<string, unknown> = {}) => i18n.global.t(k, n)

  if (!isDesktop) return

  // 拖放进入 / 离开会连续触发多次：用计数避免提示层闪烁。
  let depth = 0

  function signal(active: boolean) {
    document.dispatchEvent(new CustomEvent('nt-drag-state', { detail: { active } }))
  }

  function hasFiles(e: DragEvent): boolean {
    return !!e.dataTransfer?.types.includes('Files')
  }

  function onDragEnter(e: DragEvent) {
    if (!hasFiles(e)) return
    depth += 1
    signal(true)
  }

  // 不 preventDefault，后面的 drop 事件根本不会派发。
  function onDragOver(e: DragEvent) {
    if (!hasFiles(e)) return
    e.preventDefault()
  }

  function onDragLeave(e: DragEvent) {
    if (!hasFiles(e)) return
    depth -= 1
    if (depth <= 0) {
      depth = 0
      signal(false)
    }
  }

  function onDrop(e: DragEvent) {
    if (!hasFiles(e)) return
    // 不拦的话 WebView 会把拖进来的文件当导航目标，整个界面被顶掉。
    e.preventDefault()
    depth = 0
    signal(false)
    void handleFiles([...(e.dataTransfer?.files ?? [])])
  }

  onMounted(() => {
    window.addEventListener('dragenter', onDragEnter)
    window.addEventListener('dragover', onDragOver)
    window.addEventListener('dragleave', onDragLeave)
    window.addEventListener('drop', onDrop)
  })
  onUnmounted(() => {
    window.removeEventListener('dragenter', onDragEnter)
    window.removeEventListener('dragover', onDragOver)
    window.removeEventListener('dragleave', onDragLeave)
    window.removeEventListener('drop', onDrop)
  })

  async function handleFiles(files: File[]) {
    for (const f of files) {
      const p = (f as File & { path?: unknown }).path
      if (typeof p === 'string' && p) await openDroppedPath(p)
      else await openDroppedBlob(f)
    }
  }

  /** 有路径：走与「打开文件」完全相同的链路（编码探测 / 预览分流 / 最近文件）。 */
  async function openDroppedPath(path: string) {
    try {
      // 目录 → 当工作区打开（拖文件夹的诉求通常是「在这个目录里干活」）。
      // 判据用 readDir 能不能成功：stat 对目录也成功，没法靠它区分。
      const entries = await api.readDir(path).catch(() => null)
      if (entries) {
        await tree.open(path)
        toast.ok(t('toast.folderOpened', { name: baseName(path) }))
        return
      }
      const tab = await editor.openPath(path)
      if (tab) {
        void api.touchRecent(path)
        toast.ok(t('toast.opened', { name: baseName(path) }))
      }
    } catch (e) {
      toast.bad(t('toast.openFailed', { reason: normalizeError(e).message }))
    }
  }

  /** 拿不到路径（浏览器 dev / 个别 WebView 版本）：文本类读进来开无路径标签。 */
  async function openDroppedBlob(f: File) {
    if (!isTextKind(kindOf(f.name))) {
      toast.bad(t('toast.dropBinaryHint', { name: f.name }))
      return
    }
    if (f.size > MAX_DROP_BYTES) {
      toast.bad(t('toast.dropTooLarge', { name: f.name, size: MAX_DROP_MB }))
      return
    }
    try {
      const text = await f.text()
      const tab = editor.newTab({ title: f.name })
      // newTab 只建元数据；正文等编辑器视图挂载后补进去，并记为基线（不脏）。
      fillAfterMount(tab.id, () => {
        doReplaceDoc(tab.id, text)
        commitBaseline(tab.id)
      })
    } catch (e) {
      toast.bad(t('toast.openFailed', { reason: normalizeError(e).message }))
    }
  }

  /** 视图已挂载就直接填，否则等一次 nt-editor-ready（detail 是 { tabId }）。 */
  function fillAfterMount(tabId: number, fill: () => void) {
    if (get(tabId)) {
      fill()
      return
    }
    const onReady = (e: Event) => {
      const d = (e as CustomEvent).detail as { tabId?: number }
      if (d?.tabId !== tabId) return
      document.removeEventListener('nt-editor-ready', onReady)
      fill()
    }
    document.addEventListener('nt-editor-ready', onReady)
  }
}
