// 文件树 store：目录展开状态、工作目录、脏标记、剪切板。
//
// 归属：树的「形状」（哪些目录展开）是纯 UI 态，留在前端；
// 目录内容由后端 read_dir 给，外部变更由 `fs:changed` 事件驱动重读。
import { defineStore } from 'pinia'
import { ref } from 'vue'
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

  /** 打开一个文件夹为根。 */
  async function open(dir: string) {
    loading.value = true
    try {
      root.value = dir
      expanded.value = new Set()
      children.value = new Map()
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
  }

  /** 重读一个目录（含其已展开的子目录）。 */
  async function refresh(dir: string) {
    try {
      const entries = await api.readDir(dir)
      children.value.set(dir, entries)
      // 已展开的子目录一起刷：外部改了一个深层文件，整条路径都该反映出来。
      for (const e of entries) {
        if (e.is_dir && expanded.value.has(e.path)) {
          await refresh(e.path)
        }
      }
    } catch (e) {
      useToastStore().bad(tr('toast.dirReadFailed', { reason: normalizeError(e).message }))
    }
  }

  function isExpanded(path: string) {
    return expanded.value.has(path)
  }

  /** 切换展开：首次展开时按需加载。 */
  async function toggle(path: string) {
    if (expanded.value.has(path)) {
      expanded.value.delete(path)
      expanded.value = new Set(expanded.value)
      return
    }
    expanded.value.add(path)
    expanded.value = new Set(expanded.value)
    if (!children.value.has(path)) {
      await refresh(path)
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
    isExpanded,
    toggle,
    markDirty,
    setClipboard,
    setDragPath,
    setDropTarget,
  }
})
