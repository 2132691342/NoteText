// 查找结果：目录级查找的命中，供侧栏面板显示。
//
// 从浮窗搬到 store：浮窗关掉结果就没了，而用户要反复点开好几处。
import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { BatchFileHits } from '@/types/api'

export const useFindResultsStore = defineStore('findResults', () => {
  const groups = ref<BatchFileHits[]>([])
  const total = ref(0)

  function set(hits: BatchFileHits[], count: number) {
    // 空数组契约（AGENTS.md §2.3.2）：切片一律初始化，不存 null。
    groups.value = hits ?? []
    total.value = count
  }

  function clear() {
    groups.value = []
    total.value = 0
  }

  return { groups, total, set, clear }
})
