// 查找结果：目录级查找的命中，供侧栏面板显示。
// 从浮窗搬到 store：浮窗关掉结果就没了，而用户要反复点开好几处。
import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { BatchFileHits } from '@/types/api'

/** 面板渲染的文件组上限：超过只显示前 500 个并提示截断。 */
const MAX_GROUPS = 500

export const useFindResultsStore = defineStore('findResults', () => {
  const groups = ref<BatchFileHits[]>([])
  const total = ref(0)
  /** 命中文件数超过上限被截断：面板据此提示「结果太多」。 */
  const truncated = ref(false)

  function set(hits: BatchFileHits[], count: number) {
    // 空数组契约（AGENTS.md §2.3.2）：切片一律初始化，不存 null。
    const list = hits ?? []
    truncated.value = list.length > MAX_GROUPS
    groups.value = truncated.value ? list.slice(0, MAX_GROUPS) : list
    total.value = count
  }

  function clear() {
    groups.value = []
    total.value = 0
    truncated.value = false
  }

  return { groups, total, truncated, set, clear }
})
