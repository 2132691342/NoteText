// 压缩任务队列 store：任务面板与状态栏指示器的唯一数据源。
// 后端 250ms 心跳 + 终态事件已把快照推来（specs/18），前端不再轮询，免得两条时间线打架。

import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { api } from '@/api'
import { tr } from '@/i18n'
import { useToastStore } from '@/stores/toast'
import type { ArchiveFormatVO, ArchiveTaskVO } from '@/types/api'

export const useArchiveStore = defineStore('archive', () => {
  /** 任务快照：事件驱动更新，打开面板时 list 一次兜底。 */
  const tasks = ref<ArchiveTaskVO[]>([])
  /** 格式能力表：创建 / 转换对话框共用，启动时拉一次。 */
  const formats = ref<ArchiveFormatVO[]>([])

  const activeCount = computed(
    () =>
      tasks.value.filter(
        (t) => t.status === 'queued' || t.status === 'running' || t.status === 'paused',
      ).length,
  )

  let started = false

  /** 挂事件 + 拉初值：EditorView 挂载时调一次。 */
  async function init() {
    if (started) return
    started = true
    try {
      formats.value = (await api.archive.formats()) ?? []
    } catch {
      // 拉不到就空表：创建对话框自己给空态，不阻塞启动。
    }
    try {
      tasks.value = (await api.archive.listTasks()) ?? []
    } catch {
      /* 初值拉不到没关系：事件一来就会覆盖 */
    }
    await listen<ArchiveTaskVO[]>('archive:task-update', (ev) => {
      tasks.value = ev.payload ?? []
    })
    await listen<{ task: ArchiveTaskVO }>('archive:task-finished', (ev) => {
      onFinished(ev.payload?.task)
    })
  }

  /**
   * 终态提示：成功 / 失败 / 取消各说一句人话。
   * 文案里带文件名：队列里同时挂多条时，用户分得清是哪一条完成了。
   */
  function onFinished(task: ArchiveTaskVO | undefined) {
    if (!task) return
    const toast = useToastStore()
    const kind = tr(`archive.taskKind.${task.kind}`)
    if (task.status === 'done') {
      toast.ok(tr('archive.toastDone', { kind, name: task.title }))
    } else if (task.status === 'cancelled') {
      toast.info(tr('archive.toastCancelled', { kind, name: task.title }))
    } else {
      toast.bad(tr('archive.toastFailed', { kind, name: task.title, reason: task.error ?? '' }))
    }
  }

  function cancel(id: string) {
    void api.archive.cancelTask(id)
  }
  function pause(id: string) {
    void api.archive.pauseTask(id)
  }
  function resume(id: string) {
    void api.archive.resumeTask(id)
  }
  function clearFinished() {
    void api.archive.clearTasks()
  }

  return {
    tasks,
    formats,
    activeCount,
    init,
    cancel,
    pause,
    resume,
    clearFinished,
  }
})
