// 批量查找替换浮窗：目录 + glob，先扫描出预览，勾选后才写入。
//
// 两段式是硬约束：批量改磁盘没有撤销键，所以「扫描」与「应用」分成
// 两个按钮，应用前必须让用户看见自己要改哪些文件。
<template>
  <NtModal :title="t('tool.batchFindReplace')" width="820" @close="emit('close')">
    <div class="nt-batch">
      <!-- 条件区 -->
      <div class="nt-batch-form">
        <div class="nt-batch-row">
          <span class="nt-label">{{ t('tool.batchDir') }}</span>
          <ElInput v-model="dir" size="small" :placeholder="t('tool.batchDirHint')" />
          <ElButton size="small" @click="pickDir">{{ t('common.browse') }}</ElButton>
        </div>
        <div class="nt-batch-row">
          <span class="nt-label">{{ t('tool.batchGlob') }}</span>
          <ElInput v-model="glob" size="small" placeholder="*.ts" />
          <ElCheckbox v-model="recursive" size="small">{{ t('tool.batchRecursive') }}</ElCheckbox>
          <ElCheckbox v-model="caseSensitive" size="small">{{ t('dialog.caseSensitive') }}</ElCheckbox>
          <ElCheckbox v-model="regexMode" size="small">{{ t('tool.regexMode') }}</ElCheckbox>
        </div>
        <div class="nt-batch-row">
          <span class="nt-label">{{ t('dialog.find') }}</span>
          <ElInput v-model="find" size="small" @keydown.enter="onScan" />
          <span class="nt-label">{{ t('dialog.replaceWith') }}</span>
          <ElInput v-model="replace" size="small" />
          <ElButton size="small" type="primary" :disabled="!canScan" :loading="scanning" @click="onScan">
            {{ t('tool.batchScan') }}
          </ElButton>
        </div>
      </div>

      <!-- 结果区 -->
      <div v-if="scanned" class="nt-batch-result">
        <div class="nt-batch-summary">
          <span class="nt-readout">
            {{ t('tool.batchHitSummary', { files: result.hits.length, count: result.total_count }) }}
          </span>
          <span v-if="result.skipped" class="nt-readout nt-batch-skipped">
            {{ t('tool.batchSkipped', { count: result.skipped }) }}
          </span>
          <div class="nt-batch-spacer" />
          <ElCheckbox
            v-if="result.hits.length"
            :model-value="allChecked"
            size="small"
            @change="(v: unknown) => toggleAll(v === true)"
          >{{ t('common.selectAll') }}</ElCheckbox>
        </div>

        <div v-if="result.hits.length === 0" class="nt-empty">
          <p>{{ t('tool.batchNoHit') }}</p>
        </div>

        <div v-else class="nt-batch-list">
          <label v-for="h in result.hits" :key="h.path" class="nt-batch-item">
            <input type="checkbox" :checked="checked.has(h.path)" @change="toggle(h.path)" />
            <span class="nt-batch-path" :title="h.path">{{ h.path }}</span>
            <span class="nt-batch-count">{{ h.count }}</span>
            <span class="nt-batch-sample">{{ h.sample }}</span>
          </label>
        </div>

        <div v-if="replace" class="nt-batch-actions">
          <ElButton size="small" @click="emit('close')">{{ t('common.cancel') }}</ElButton>
          <ElButton
            size="small"
            type="primary"
            :disabled="!checked.size"
            :loading="applying"
            @click="onApply"
          >
            {{ t('tool.batchApply', { count: checked.size }) }}
          </ElButton>
        </div>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElCheckbox, ElInput } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { api, normalizeError } from '@/api'
import { useToastStore } from '@/stores/toast'
import { useFileTreeStore } from '@/stores/fileTree'
import type { BatchFindVO, BatchReplaceVO } from '@/types/api'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const toast = useToastStore()
const tree = useFileTreeStore()

const dir = ref(tree.root || '')
const glob = ref('')
const find = ref('')
const replace = ref('')
const recursive = ref(true)
const caseSensitive = ref(false)
const regexMode = ref(false)

const scanning = ref(false)
const applying = ref(false)
const scanned = ref(false)
const result = ref<BatchFindVO>({ hits: [], total_count: 0, skipped: 0, truncated: false })

/** 勾选的文件；应用时只改这些。 */
const checked = ref<Set<string>>(new Set())

const canScan = computed(() => !!dir.value && !!find.value)

const allChecked = computed(
  () => result.value.hits.length > 0 && checked.value.size === result.value.hits.length,
)

async function pickDir() {
  const p = await api.openFolderDialog(dir.value || undefined)
  if (p) dir.value = p
}

async function onScan() {
  if (!canScan.value) return
  scanning.value = true
  try {
    result.value = await api.batchScanFind({
      dir: dir.value,
      glob: glob.value,
      find: find.value,
      mode: regexMode.value ? 'regex' : 'plain',
      case_sensitive: caseSensitive.value,
      recursive: recursive.value,
    })
    // 默认全选：扫出来就是想改的，让用户反选比正选省事。
    checked.value = new Set(result.value.hits.map((h) => h.path))
    scanned.value = true
  } catch (e) {
    toast.bad(normalizeError(e).message)
  } finally {
    scanning.value = false
  }
}

async function onApply() {
  applying.value = true
  try {
    const res: BatchReplaceVO = await api.batchApplyReplace({
      dir: dir.value,
      glob: glob.value,
      find: find.value,
      replace: replace.value,
      mode: regexMode.value ? 'regex' : 'plain',
      case_sensitive: caseSensitive.value,
      recursive: recursive.value,
      files: [...checked.value],
    })
    toast.ok(t('toast.batchReplaced', { files: res.changed, count: res.replacements }))
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

function toggle(path: string) {
  const next = new Set(checked.value)
  if (next.has(path)) next.delete(path)
  else next.add(path)
  checked.value = next
}

function toggleAll(v: boolean) {
  checked.value = v ? new Set(result.value.hits.map((h) => h.path)) : new Set()
}
</script>

<style scoped>
.nt-batch {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-4);
  min-height: 0;
}

.nt-batch-form {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
  flex: none;
}

.nt-batch-row {
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

.nt-batch-result {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
  min-height: 0;
}

.nt-batch-summary {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-4);
  flex: none;
}

.nt-batch-spacer {
  flex: 1;
}

.nt-batch-skipped {
  color: var(--nt-warning);
}

.nt-batch-list {
  display: flex;
  flex-direction: column;
  gap: 1px;
  max-height: 40vh;
  overflow: auto;
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius-sm);
}

.nt-batch-item {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
  padding: var(--nt-sp-2) var(--nt-sp-3);
  font-size: var(--nt-fs-sm);
  cursor: pointer;
}

.nt-batch-item:hover {
  background: var(--nt-surface-hover);
}

.nt-batch-item:active {
  background: var(--nt-surface-selected);
}

.nt-batch-path {
  flex: none;
  max-width: 40%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--nt-ink);
}

.nt-batch-count {
  flex: none;
  min-width: 2.5em;
  text-align: right;
  color: var(--nt-primary);
  font-variant-numeric: tabular-nums;
}

.nt-batch-sample {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--nt-ink-3);
  font-family: var(--nt-font-editor);
}

.nt-batch-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--nt-sp-2);
  flex: none;
}
</style>
