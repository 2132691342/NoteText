// 文件树 store：目录展开状态、工作目录、脏标记、剪切板。
// 归属：树的「形状」（哪些目录展开）是纯 UI 态，留在前端；
import { defineStore } from 'pinia'
import { markRaw, ref } from 'vue'
import { api, normalizeError } from '@/api'
import type { DirEntry } from '@/types/api'
import { useToastStore } from './toast'
import { tr } from '@/i18n'

export const useFileTreeStore = defineStore('fileTree', () => {
  /** 当前工作目录：空表示「未打开文件夹」。 */
  const root = ref('')
  /** 展开的目录路径集合。 */
  const expanded = ref<Set<string>>(new Set())
  /** 每个目录的子项缓存：切目录时不用整棵重读。 */
  const children = ref<Map<string, DirEntry[]>>(new Map())
  /** 树里被修改过的文件：前端自己的脏标记（与 tab 脏标记同源）。 */
  const dirtyPaths = ref<Set<string>>(new Set())
  /** 剪切 / 复制的路径（树内操作）。 */
  const clipboard = ref<{ paths: string[]; cut: boolean } | null>(null)
  /** 树内拖拽的源路径：null = 没在拖。 */
  const dragPath = ref<string | null>(null)
  /** 拖拽悬停的目标目录：高亮 + 落点提示。 */
  const dropTarget = ref<string | null>(null)
  const loading = ref(false)
  /** 单次「刷新全部」最多重读多少个已展开目录：超过就只刷根（防卡死）。 */
  const MAX_REFRESH_DIRS = 24

  /** 打开一个文件夹为根。 */
  async function open(dir: string) {
    loading.value = true
    try {
      root.value = dir
      expanded.value = new Set()
      children.value = new Map()
      watched.value = new Set([dir])
      await api.watchDir(dir)
      await refresh(dir)
    } catch (e) {
      useToastStore().bad(tr('toast.folderOpenFailed', { reason: normalizeError(e).message }))
    } finally {
      loading.value = false
    }
  }

  /** 关闭文件夹。 */
  async function close() {
    if (root.value) {
      try {
        await api.unwatchDir(root.value)
      } catch {
        /* 取消监听失败不影响关闭 */
      }
    }
    root.value = ''
    expanded.value = new Set()
    children.value = new Map()
    // 换/关文件夹时把所有残留监听收干净。
    for (const d of [...watched.value]) {
      try {
        await api.unwatchDir(d)
      } catch {
        /* 已经没了 */
      }
    }
    watched.value = new Set()
  }

  /** 重读一个目录（只读这一层，不递归）；深层目录由外部变更事件单独分发。 */
  async function refresh(dir: string): Promise<boolean> {
    // 只刷树里的目录：标签栏 / 拾取器会传任意路径，不该弹错误提示。
    if (!inTree(dir)) return true
    try {
      const entries = await api.readDir(dir)
      // markRaw：条目数组是纯快照（变更时整体替换，从不原地改），
      // 不给它做 deep-reactive 包装——大目录上千条目逐个代理纯浪费。
      children.value.set(dir, markRaw(entries))
      evictChildren()
      return true
    } catch (e) {
      useToastStore().bad(tr('toast.dirReadFailed', { reason: normalizeError(e).message }))
      return false
    }
  }

  /** 缓存条目上限：超过就淘汰未展开目录的快照。 */
  const MAX_CACHED_DIRS = 400

  /**
   * 淘汰函数：缓存目录数超限时删掉「未展开且不是根」的条目快照。
   * 不淘汰的话 children 只增不减，内存持续上涨、每次响应式检查也变慢。
   */
  function evictChildren() {
    if (children.value.size <= MAX_CACHED_DIRS) return
    const rootPath = root.value
    const next = new Map(children.value)
    for (const key of next.keys()) {
      if (next.size <= MAX_CACHED_DIRS) break
      if (key !== rootPath && !expanded.value.has(key)) next.delete(key)
    }
    if (next.size !== children.value.size) children.value = next
  }

  /** 该路径是否在已打开的树里（根目录本身也算）。 */
  function inTree(path: string): boolean {
    const r = root.value
    if (!r) return false
    if (path === r) return true
    return path.startsWith(r.endsWith('\\') || r.endsWith('/') ? r : `${r}\\`) || path.startsWith(`${r}/`)
  }

  /** 全量重建：只在用户主动「刷新」时用，逐层重读已展开的目录（每次都有上限）。 */
  async function refreshAll() {
    if (!root.value) return
    await refresh(root.value)
    const dirs = [...expanded.value].filter((d) => children.value.has(d))
    for (const d of dirs.slice(0, MAX_REFRESH_DIRS)) {
      await refresh(d)
    }
  }

  /**
   * 按变更路径局部刷新：文件系统事件带来的是具体路径，没必要整个工作目录重读一遍。
   */
  async function refreshPaths(paths: string[]) {
    if (!root.value) return
    const rootValue = root.value
    const dirs = new Set<string>()
    for (const p of paths) {
      const cut = Math.max(p.lastIndexOf('\\'), p.lastIndexOf('/'))
      if (cut > 0) dirs.add(p.slice(0, cut))
    }
    // 没有父目录可算（相对路径），或待刷目录过多（事件洪水）就退回整棵树。
    if (dirs.size === 0 || dirs.size > MAX_REFRESH_DIRS) {
      await refresh(rootValue)
      return
    }
    await Promise.all(
      [...dirs]
        .filter((d) => children.value.has(d))
        .map((d) => refresh(d)),
    )
  }

  function isExpanded(path: string) {
    return expanded.value.has(path)
  }

  /** 切换展开：首次展开时按需加载并加监听，折叠时释放监听。 */
  async function toggle(path: string) {
    if (expanded.value.has(path)) {
      expanded.value.delete(path)
      expanded.value = new Set(expanded.value)
      // 折叠就停止监听该目录，子目录的监听一并释放。
      void releaseWatchesUnder(path)
      return
    }
    expanded.value.add(path)
    expanded.value = new Set(expanded.value)
    // 监听与读取并发：用户不需要等 readDir 回来才能收到变更通知。
    if (!watched.value.has(path)) {
      watched.value.add(path)
      void api.watchDir(path).catch(() => {
        watched.value.delete(path)
      })
    }
    if (!children.value.has(path)) {
      await refresh(path)
    }
  }

  /** 已加监听的目录集合：避免重复 watch，也用于折叠时回收。 */
  const watched = ref<Set<string>>(new Set())

  /** 释放 path 本身及其子路径上的监听。 */
  async function releaseWatchesUnder(path: string) {
    const targets = [...watched.value].filter((d) => d === path || d.startsWith(`${path}\\`) || d.startsWith(`${path}/`))
    for (const d of targets) {
      watched.value.delete(d)
      try {
        await api.unwatchDir(d)
      } catch {
        /* 取消监听失败不影响折叠 */
      }
    }
  }

  function markDirty(path: string, dirty: boolean) {
    if (dirty) dirtyPaths.value.add(path)
    else dirtyPaths.value.delete(path)
    dirtyPaths.value = new Set(dirtyPaths.value)
  }

  function setClipboard(paths: string[], cut: boolean) {
    clipboard.value = { paths, cut }
  }

  function setDragPath(path: string | null) {
    dragPath.value = path
  }

  function setDropTarget(path: string | null) {
    dropTarget.value = path
  }

  return {
    root,
    expanded,
    children,
    dirtyPaths,
    clipboard,
    dragPath,
    dropTarget,
    loading,
    open,
    close,
    refresh,
    refreshAll,
    refreshPaths,
    inTree,
    isExpanded,
    toggle,
    markDirty,
    setClipboard,
    setDragPath,
    setDropTarget,
  }
})
