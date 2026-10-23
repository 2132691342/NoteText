// 列表选择浮窗：渲染 common/askPick 的状态。
// 带搜索框：语言 / 编码这类清单有 30+ 项，靠滚轮找太慢。
<template>
  <NtModal
    v-if="current"
    :title="current.opts.title"
    width="420"
    @close="choose(null)"
  >
    <div class="nt-pick">
      <ElInput
        v-model="filter"
        size="small"
        clearable
        spellcheck="false"
        :placeholder="current.opts.placeholder || t('common.search')"
        @keydown.down.prevent="move(1)"
        @keydown.up.prevent="move(-1)"
        @keydown.enter.prevent="choose(shown[cursor]?.value ?? null)"
        @keydown.esc.prevent="choose(null)"
      />
      <div ref="listEl" class="nt-pick-list" @keydown.down.prevent @keydown.up.prevent>
        <div v-if="!shown.length" class="nt-empty-sm">{{ t('common.none') }}</div>
        <button
          v-for="(it, i) in shown"
          :key="it.value"
          ref="rows"
          class="nt-pick-row"
          :class="{ 'is-on': it.value === current.opts.current, 'is-cursor': i === cursor }"
          @click="choose(it.value)"
          @mouseenter="cursor = i"
        >
          <span class="nt-pick-label">{{ it.label }}</span>
          <span v-if="it.hint" class="nt-readout nt-pick-hint">{{ it.hint }}</span>
        </button>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed, nextTick, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElInput } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { current as pending, settlePick, type PickItem } from '@/components/common/askPick'

const { t } = useI18n()

const current = computed(() => pending.value)
const filter = ref('')
const cursor = ref(0)
const listEl = ref<HTMLElement | null>(null)
const rows = ref<HTMLButtonElement[]>([])

const shown = computed<PickItem[]>(() => {
  const items = current.value?.opts.items ?? []
  const q = filter.value.trim().toLowerCase()
  if (!q) return items
  // 标签与提示都参与匹配：用户记得的可能是扩展名而不是语言名。
  return items.filter(
    (i) => i.label.toLowerCase().includes(q) || (i.hint ?? '').toLowerCase().includes(q),
  )
})

// 每换一个待处理项就重置过滤与光标，并把当前项滚到可见处。
watch(current, (c) => {
  filter.value = ''
  cursor.value = c ? Math.max(0, c.opts.items.findIndex((i) => i.value === c.opts.current)) : 0
  if (c) {
    void nextTick(() => {
      const idx = c.opts.items.findIndex((i) => i.value === c.opts.current)
      rows.value[idx]?.scrollIntoView({ block: 'nearest' })
    })
  }
})

function move(delta: number) {
  const n = shown.value.length
  if (n === 0) return
  cursor.value = (cursor.value + delta + n) % n
  void nextTick(() => rows.value[cursor.value]?.scrollIntoView({ block: 'nearest' }))
}

function choose(value: string | null) {
  settlePick(value)
}
</script>

<style scoped>
.nt-pick {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
}

.nt-pick-list {
  display: flex;
  flex-direction: column;
  gap: 1px;
  max-height: 360px;
  overflow-y: auto;
}

.nt-pick-row {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
  width: 100%;
  padding: var(--nt-sp-1) var(--nt-sp-2);
  border-radius: var(--nt-radius-xs);
  text-align: left;
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-2);
  transition:
    background-color var(--nt-dur-fast) var(--nt-ease),
    color var(--nt-dur-fast) var(--nt-ease);
}

.nt-pick-row:hover,
.nt-pick-row.is-cursor {
  background: var(--nt-surface-hover);
  color: var(--nt-ink);
}

/* 当前生效项：主色字 + 软底。与「光标所在」是两回事，所以两条规则并存。 */
.nt-pick-row.is-on {
  background: var(--nt-primary-soft);
  color: var(--nt-primary);
  font-weight: 600;
}

.nt-pick-label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.nt-pick-hint {
  flex: none;
  font-variant-numeric: tabular-nums;
}
</style>
