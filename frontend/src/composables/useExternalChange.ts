// 外部变更：文件被别的程序改了怎么办。
// 三种情况分开处理（这是编辑器最容易被骂的地方）：标签干净 → 静默重载；
import { onMounted, onUnmounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { useEditorStore } from '@/stores/editor'
import { useFileTreeStore } from '@/stores/fileTree'
import { useI18n } from 'vue-i18n'
import { useToastStore } from '@/stores/toast'
import { api, isDesktop, normalizeError } from '@/api'
import { doReplaceDoc } from '@/composables/useEditorActions'
import { getText, getTableProvider } from '@/composables/useEditorRegistry'
import { baseName } from '@/utils'

export function useExternalChange() {
  const { t } = useI18n()
  const editor = useEditorStore()
  const tree = useFileTreeStore()
  const toast = useToastStore()

  if (!isDesktop) return

  let unlisten: (() => void) | null = null

  /** 每个路径 600ms 去抖 + 加锁：一次保存送来的多个文件事件只处理一次。 */
  const pendingTimers = new Map<string, number>()
  const inFlight = new Set<string>()

  function schedule(path: string) {
    const old = pendingTimers.get(path)
    if (old !== undefined) window.clearTimeout(old)
    const timer = window.setTimeout(() => {
      pendingTimers.delete(path)
      if (inFlight.has(path)) return
      inFlight.add(path)
      void handlePath(path).finally(() => inFlight.delete(path))
    }, 600)
    pendingTimers.set(path, timer)
  }

  /** 文件树刷新：800ms 去抖 + 单飞 + 40 目录上限，只刷变更路径所在目录。 */
  const pendingDirs = new Set<string>()
  let treeTimer: number | undefined
  let refreshing = false
  /** 待刷目录上限：超过就降级为只刷根。 */
  const MAX_PENDING_DIRS = 40

  function scheduleTreeRefresh(paths: string[]) {
    for (const p of paths) {
      const dir = parentDirOf(p)
      if (dir) pendingDirs.add(dir)
    }
    if (treeTimer !== undefined) window.clearTimeout(treeTimer)
    treeTimer = window.setTimeout(() => {
      treeTimer = undefined
      void flushTreeRefresh()
    }, 800)
  }

  async function flushTreeRefresh() {
    const root = tree.root
    if (!root) {
      pendingDirs.clear()
      return
    }
    // 上一轮还没回来：这一轮的路径留在集合里，等下一轮一并处理。
    if (refreshing) return
    refreshing = true
    try {
      let dirs = [...pendingDirs]
      pendingDirs.clear()
      const inRoot = (d: string) =>
        d === root || d.startsWith(`${root}\\`) || d.startsWith(`${root}/`)
      let targets = dirs.filter(inRoot)
      if (targets.length > MAX_PENDING_DIRS) {
        // 事件洪水：只刷根目录，叶子交给它自己的事件（树是懒展开的）。
        targets = [root]
      }
      for (const d of targets) {
        if (!tree.root) break
        await tree.refresh(d)
      }
      dirs = []
    } finally {
      refreshing = false
      // 刷完还有新路径积压：再排一轮，但不立即递归（避免自锁）。
      if (pendingDirs.size > 0) {
        if (treeTimer !== undefined) window.clearTimeout(treeTimer)
        treeTimer = window.setTimeout(() => {
          treeTimer = undefined
          void flushTreeRefresh()
        }, 800)
      }
    }
  }

  function parentDirOf(p: string): string | null {
    const i = Math.max(p.lastIndexOf('\\'), p.lastIndexOf('/'))
    return i > 0 ? p.slice(0, i) : null
  }

  onMounted(async () => {
    unlisten = await listen<string[]>('fs:changed', (ev) => {
      for (const path of ev.payload) schedule(path)
      // 文件树只刷变化路径的父目录：外部新建 / 删除文件要反映出来。
      scheduleTreeRefresh(ev.payload)
    })
  })

  onUnmounted(() => {
    unlisten?.()
    for (const timer of pendingTimers.values()) window.clearTimeout(timer)
    pendingTimers.clear()
    if (treeTimer !== undefined) window.clearTimeout(treeTimer)
  })

  async function handlePath(path: string) {
    const tab = editor.findByPath(path)
    if (!tab) return
    // 分块标签不重读全文：读块时后端按修改时间报 stale。
    if (tab.chunked) return
    // 大表格标签：正文在磁盘上，不在编辑器里。外部变更由读页时的 stale 标记提示，
    // 这里不重读全文——几十万行的 CSV 读进来会卡住界面。
    if (getTableProvider(tab.id)) return
    if (tab.dirty) {
      // 脏标签不覆盖：用户手里有未保存的修改，替用户做决定就是丢内容。
      toast.warn(t('toast.fileChangedDirty', { name: baseName(path) }))
      return
    }
    // 干净标签：必须真的把新正文推进编辑器，只改元数据会让编辑区停在旧内容。
    // 那时提示说文件变了、编辑区还是旧的，用户以为提示在撒谎。
    try {
      // 截断标签只有前段内容：全量读会被后端拒绝，外部变更就永远同步不过来
      const content = await api.readFile(path, undefined, tab.truncated ? { mode: 'head' } : undefined)
      const changed = getText(tab.id) !== content.text
      editor.applyContent(tab, content)
      if (changed) {
        // 只有正文真的不一样才重绘与提示：文件系统常送「属性变更」这类
        // 无关事件，每次都重绘会让光标位置丢失。
        doReplaceDoc(tab.id, content.text)
        toast.info(t('toast.fileReloaded', { name: baseName(path) }))
      }
    } catch (e) {
      toast.bad(t('toast.openFailed', { reason: normalizeError(e).message }))
    }
  }
}
