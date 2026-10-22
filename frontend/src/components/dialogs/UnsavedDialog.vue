<script lang="ts">
// 未保存确认的状态与队列：与视图分开，命令层只 import 函数不必碰组件。
//
// 为什么不用后端二选一消息框：二选一里「不保存」和「取消」只能留一个，
// 于是「现在不想存、但也不想关」这个最正常的选择消失了，用户只能被迫丢内容。
import { ref } from 'vue'

export type UnsavedChoice = 'save' | 'discard' | 'cancel'

interface Pending {
  name: string
  resolve: (c: UnsavedChoice) => void
}

/** 同一时刻只显示一个确认框，后来的排队。 */
const current = ref<Pending | null>(null)
const queue: Pending[] = []

/** 弹一个未保存确认，等用户选。 */
export function askUnsaved(name: string): Promise<UnsavedChoice> {
  return new Promise((resolve) => {
    const item: Pending = { name, resolve }
    if (current.value) queue.push(item)
    else current.value = item
  })
}

/** 当前是否有待处理的确认：退出流程用它避免重复弹。 */
export function hasUnsavedPrompt(): boolean {
  return current.value !== null
}

function settle(choice: UnsavedChoice) {
  const cur = current.value
  if (!cur) return
  current.value = null
  cur.resolve(choice)
  const next = queue.shift()
  if (next) current.value = next
}
</script>

<template>
  <NtModal
    v-if="current"
    :title="t('dialog.unsavedTitle', { name: current.name })"
    width="480"
    @close="settle('cancel')"
  >
    <div class="nt-unsaved">
      <p class="nt-unsaved-body">{{ t('dialog.unsavedBody') }}</p>
      <div class="nt-unsaved-actions">
        <ElButton size="small" @click="settle('cancel')">
          {{ t('dialog.cancelClose') }}
        </ElButton>
        <ElButton size="small" @click="settle('discard')">
          {{ t('dialog.discardAndClose') }}
        </ElButton>
        <ElButton size="small" type="primary" @click="settle('save')">
          {{ t('dialog.saveAndClose') }}
        </ElButton>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { useI18n } from 'vue-i18n'
import { ElButton } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'

const { t } = useI18n()
</script>

<style scoped>
.nt-unsaved {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-4);
}

.nt-unsaved-body {
  margin: 0;
  font-size: var(--nt-fs-md);
  color: var(--nt-ink-2);
  line-height: var(--nt-lh-base);
}

.nt-unsaved-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--nt-sp-2);
}
</style>
