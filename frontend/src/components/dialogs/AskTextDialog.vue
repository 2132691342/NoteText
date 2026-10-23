// 文本输入浮窗：重命名 / 另存为的名字输入。
// 状态与队列在 common/askText.ts，这里只负责渲染。
<template>
  <NtModal
    v-if="current"
    :title="current.opts.title"
    width="440"
    @close="settle(null)"
  >
    <div class="nt-ask">
      <label v-if="current.opts.label" class="nt-label" :for="inputId">
        {{ current.opts.label }}
      </label>
      <ElInput
        :id="inputId"
        ref="inputEl"
        v-model="draft"
        size="small"
        spellcheck="false"
        :placeholder="current.opts.placeholder"
        @keydown.enter.prevent="submit"
        @keydown.esc.prevent="settle(null)"
      />
      <div class="nt-ask-actions">
        <ElButton size="small" @click="settle(null)">
          {{ current.opts.cancelText || t('common.cancel') }}
        </ElButton>
        <ElButton size="small" type="primary" :disabled="!draft.trim()" @click="submit">
          {{ current.opts.okText || t('common.ok') }}
        </ElButton>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed, nextTick, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElInput } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { current as pending, settleTextPrompt, type PromptOptions } from '@/components/common/askText'

const { t } = useI18n()

/** 模块级状态直接拿来用：不包 computed，排队变化时要立刻反映。 */
const current = computed(() => pending.value)
const draft = ref('')
const inputEl = ref<InstanceType<typeof ElInput> | null>(null)
/** label 的 for 要指向真实 input 元素：ElInput 包了一层，id 落在外壳上不管用。 */
const inputId = `nt-ask-${Math.random().toString(36).slice(2, 8)}`

// 每换一个待处理项就把草稿重置、并聚焦全选：
// 用户重命名时想直接打字覆盖旧名字，不想先按 End 再退格。
watch(current, (c) => {
  draft.value = c?.opts.value ?? ''
  if (c) {
    void nextTick(() => {
      inputEl.value?.focus()
      const el = inputEl.value?.input as HTMLInputElement | undefined
      el?.select()
    })
  }
})

function submit() {
  const v = draft.value.trim()
  if (v) settle(v)
}

function settle(v: string | null) {
  settleTextPrompt(v)
}

export type { PromptOptions }
</script>

<style scoped>
.nt-ask {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
}

.nt-ask-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--nt-sp-2);
}
</style>
