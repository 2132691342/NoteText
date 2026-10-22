// 批量重命名浮窗：三种模式（查找替换 / 加序号 / 大小写），预览后才落地。
//
// 冲突（目标已存在）在预览里就标红并不可勾选：覆盖别人的文件没有撤销键，
// 与其应用时报错回滚，不如在勾选阶段就挡住。
<template>
  <NtModal :title="t('tool.batchRename')" width="780" @close="emit('close')">
    <div class="nt-rename">
      <div class="nt-rename-form">
        <div class="nt-rename-row">
          <span class="nt-label">{{ t('tool.renameMode') }}</span>
          <ElRadioGroup v-model="mode" size="small">
            <ElRadioButton value="replace">{{ t('tool.renameReplace') }}</ElRadioButton>
            <ElRadioButton value="sequence">{{ t('tool.renameSequence') }}</ElRadioButton>
            <ElRadioButton value="case">{{ t('tool.renameCase') }}</ElRadioButton>
          </ElRadioGroup>
        </div>

        <div v-if="mode === 'replace'" class="nt-rename-row">
          <span class="nt-label">{{ t('dialog.find') }}</span>
          <ElInput v-model="find" size="small" />
          <span class="nt-label">{{ t('dialog.replaceWith') }}</span>
          <ElInput v-model="replace" size="small" />
        </div>

        <div v-else-if="mode === 'sequence'" class="nt-rename-row">
          <span class="nt-label">{{ t('tool.renameFormat') }}</span>
          <ElInput v-model="replace" size="small" placeholder="{n}_" />
          <span class="nt-label">{{ t('tool.renameStart') }}</span>
          <ElInput v-model="start" size="small" type="number" :min="0" />
          <ElCheckbox v-model="prefix" size="small">{{ t('tool.renamePrefix') }}</ElCheckbox>
        </div>

        <div v-else class="nt-rename-row">
          <span class="nt-label">{{ t('tool.renameCase') }}</span>
          <ElRadioGroup v-model="caseTarget" size="small">
            <ElRadioButton value="upper">{{ t('tool.toUpper') }}</ElRadioButton>
            <ElRadioButton value="lower">{{ t('tool.toLower') }}</ElRadioButton>
          </ElRadioGroup>
        </div>

        <div class="nt-rename-row">
          <ElButton size="small" @click="pickFiles">{{ t('tool.renamePick') }}</ElButton>
          <span class="nt-readout">{{ files.length }} {{ t('tool.renameFileCount') }}</span>
          <div class="nt-rename-spacer" />
          <ElButton
            size="small"
            type="primary"
            :disabled="!files.length"
            :loading="planning"
            @click="onPlan"
          >{{ t('tool.batchPreview') }}</ElButton>
        </div>
      </div>

      <div v-if="planned" class="nt-rename-result">
        <div class="nt-rename-list">
          <div v-if="plan.items.length === 0" class="nt-empty">
            <p>{{ t('tool.renameNoFile') }}</p>
          </div>
          <label
            v-for="(it, i) in plan.items"
            :key="`${it.from}-${i}`"
            class="nt-rename-item"
            :class="{ 'is-blocked': it.conflict || it.unchanged }"
          >
            <input
              type="checkbox"
              :checked="isChecked(it)"
              :disabled="it.conflict || it.unchanged"
              @change="toggle(it)"
            />
            <span class="nt-rename-from">{{ baseOf(it.from) }}</span>
            <span class="nt-rename-arrow">→</span>
            <span class="nt-rename-to">{{ baseOf(it.to) }}</span>
            <span v-if="it.conflict" class="nt-rename-flag">{{ t('tool.renameConflict') }}</span>
            <span v-else-if="it.unchanged" class="nt-rename-flag nt-rename-dim">
              {{ t('tool.renameUnchanged') }}
            </span>
          </label>
        </div>

        <div class="nt-rename-actions">
          <ElButton size="small" @click="emit('close')">{{ t('common.cancel') }}</ElButton>
          <ElButton
            size="small"
            type="primary"
            :disabled="!selected.length"
            :loading="applying"
            @click="onApply"
          >{{ t('tool.batchApply', { count: selected.length }) }}</ElButton>
        </div>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElCheckbox, ElInput, ElRadioButton, ElRadioGroup } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { api, normalizeError } from '@/api'
import { useToastStore } from '@/stores/toast'
import { baseName } from '@/utils'
import type { BatchRenameVO, RenamePlanItem } from '@/types/api'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const toast = useToastStore()

const mode = ref<'replace' | 'sequence' | 'case'>('replace')
const find = ref('')
const replace = ref('')
const start = ref(1)
const prefix = ref(true)
const caseTarget = ref<'upper' | 'lower'>('upper')

const files = ref<string[]>([])
const planning = ref(false)
const applying = ref(false)
const planned = ref(false)
const plan = ref<BatchRenameVO>({ items: [], applicable: 0 })

const checked = ref<Set<string>>(new Set())

/** 可应用的项：冲突与「没变」的排除在外。 */
const applicable = computed(() => plan.value.items.filter((i) => !i.conflict && !i.unchanged))
const selected = computed(() => applicable.value.filter((i) => checked.value.has(i.from)))

/** 模式或参数变了就作废上一次预览：拿旧计划去应用是危险的。 */
watch([mode, find, replace, start, prefix, caseTarget, files], () => {
  planned.value = false
  checked.value = new Set()
})

async function pickFiles() {
  const p = await api.openFileDialog()
  if (p) {
    if (!files.value.includes(p)) files.value = [...files.value, p]
  }
}

async function onPlan() {
  planning.value = true
  try {
    plan.value = await api.batchPlanRename({
      files: files.value,
      mode: mode.value,
      find: find.value,
      replace: mode.value === 'case' ? caseTarget.value : replace.value,
      start: start.value,
      prefix: prefix.value,
    })
    // 默认勾选所有可应用的：冲突的本来就不让勾。
    checked.value = new Set(applicable.value.map((i) => i.from))
    planned.value = true
  } catch (e) {
    toast.bad(normalizeError(e).message)
  } finally {
    planning.value = false
  }
}

async function onApply() {
  applying.value = true
  try {
    const res = await api.batchApplyRename({
      items: selected.value.map((i) => ({ from: i.from, to: i.to })),
    })
    toast.ok(t('toast.batchRenamed', { count: res.changed }))
    if (res.failures.length) {
      toast.bad(t('toast.batchPartialFail', { count: res.failures.length }))
    }
    emit('close')
  } catch (e) {
    toast.bad(normalizeError(e).message)
  } finally {
    applying.value = false
  }
}

function isChecked(it: RenamePlanItem): boolean {
  return checked.value.has(it.from)
}

function toggle(it: RenamePlanItem) {
  const next = new Set(checked.value)
  if (next.has(it.from)) next.delete(it.from)
  else next.add(it.from)
  checked.value = next
}

/** 列表里只显示文件名：整条路径会把行撑爆，而目录通常是一样的。 */
function baseOf(p: string): string {
  return baseName(p)
}
</script>

<style scoped>
.nt-rename {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-4);
  min-height: 0;
}

.nt-rename-form {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
  flex: none;
}

.nt-rename-row {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
}

.nt-label {
  font-size: var(--nt-fs-label);
  font-weight: 600;
  color: var(--nt-ink-2);
  flex: none;
  min-width: 4em;
}

.nt-rename-spacer {
  flex: 1;
}

.nt-rename-result {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
  min-height: 0;
}

.nt-rename-list {
  display: flex;
  flex-direction: column;
  gap: 1px;
  max-height: 40vh;
  overflow: auto;
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius-sm);
}

.nt-rename-item {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  padding: var(--nt-sp-2) var(--nt-sp-3);
  font-size: var(--nt-fs-sm);
  cursor: pointer;
}

.nt-rename-item:hover {
  background: var(--nt-surface-hover);
}

.nt-rename-item:active {
  background: var(--nt-surface-selected);
}

/* 不可应用的行压暗：让用户一眼看出哪几行是「不会动的」 */
.nt-rename-item.is-blocked {
  opacity: 0.55;
  cursor: not-allowed;
}

.nt-rename-from {
  color: var(--nt-ink-3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 40%;
}

.nt-rename-arrow {
  color: var(--nt-ink-3);
  flex: none;
}

.nt-rename-to {
  color: var(--nt-ink);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}

.nt-rename-flag {
  flex: none;
  font-size: var(--nt-fs-hint);
  color: var(--nt-danger);
}

.nt-rename-dim {
  color: var(--nt-ink-3);
}

.nt-rename-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--nt-sp-2);
  flex: none;
}
</style>
