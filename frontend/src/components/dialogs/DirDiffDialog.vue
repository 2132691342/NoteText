// 目录对比浮窗：递归比两个文件夹，按「仅左 / 仅右 / 内容不同 / 相同」分类。
//
// 后端 `dir_diff` 已经做了递归比较，这里只负责选目录、展示、跳转。
// 展示用扁平列表 + 状态标记而不是树：比的是「差异」，树会把
// 大量相同的目录也铺开，用户要在一屏里看完差异。

<template>
  <NtModal :title="t('dirDiff.title')" width="760" @close="dialogs.close()">
    <div class="nt-dd">
      <div class="nt-dd-bar">
        <span class="nt-label">{{ t('dirDiff.left') }}</span>
        <ElInput :model-value="left" size="small" readonly class="nt-dd-path" />
        <ElButton size="small" @click="pick('left')">{{ t('dirDiff.pickLeft') }}</elButton>
        <span class="nt-label">{{ t('dirDiff.right') }}</span>
        <ElInput :model-value="right" size="small" readonly class="nt-dd-path" />
        <ElButton size="small" @click="pick('right')">{{ t('dirDiff.pickRight') }}</elButton>
        <ElButton size="small" type="primary" :loading="loading" :disabled="!left || !right" @click="run">
          {{ t('dirDiff.compare') }}
        </ElButton>
      </div>

      <div v-if="summary" class="nt-dd-summary">{{ summary }}</div>

      <div class="nt-dd-list">
        <div v-if="!entries.length && !loading" class="nt-empty-sm">
          {{ left && right ? t('dirDiff.empty') : t('dirDiff.pickLeft') }}
        </div>
        <button
          v-for="e in entries"
          :key="e.rel_path"
          class="nt-row-item nt-dd-row"
          :class="`is-${e.status}`"
          @click="openSide(e)"
        >
          <span class="nt-dd-status">{{ statusLabel(e.status) }}</span>
          <span class="nt-dd-rel">{{ e.rel_path }}</span>
          <span class="nt-readout">{{ sizeText(e) }}</span>
        </button>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElInput } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { useDialogStore } from '@/stores/dialogs'
import { useToastStore } from '@/stores/toast'
import { api, normalizeError } from '@/api'
import { formatSize } from '@/utils'
import type { DirDiffEntryVO } from '@/types/api'

const { t } = useI18n()
const dialogs = useDialogStore()
const toast = useToastStore()

const left = ref('')
const right = ref('')
const entries = ref<DirDiffEntryVO[]>([])
const loading = ref(false)

const summary = computed(() => {
  if (!entries.value.length) return ''
  const n = (s: string) => entries.value.filter((e) => e.status === s).length
  return t('dirDiff.summary', {
    left: n('left_only'),
    right: n('right_only'),
    diff: n('diff'),
    same: n('same'),
  })
})

function statusLabel(s: DirDiffEntryVO['status']): string {
  if (s === 'left_only') return t('dirDiff.onlyLeft')
  if (s === 'right_only') return t('dirDiff.onlyRight')
  if (s === 'diff') return t('dirDiff.differ')
  return t('dirDiff.same')
}

/** 大小读数：只显示有差异那一侧的大小，两边相同时没有信息量。 */
function sizeText(e: DirDiffEntryVO): string {
  if (e.status === 'same') return ''
  if (e.status === 'left_only') return formatSize(e.left_size)
  if (e.status === 'right_only') return formatSize(e.right_size)
  return `${formatSize(e.left_size)} ↔ ${formatSize(e.right_size)}`
}

async function pick(side: 'left' | 'right') {
  const dir = await api.openFolderDialog()
  if (!dir) return
  if (side === 'left') left.value = dir
  else right.value = dir
}

async function run() {
  loading.value = true
  try {
    const r = await api.dirDiff(left.value, right.value)
    if (!r.ok) {
      toast.bad(r.error_message ?? t('convert.parseFailed', { reason: '' }))
      return
    }
    // 「相同」的条目默认有价值不高，排到最后：用户要看的是差异。
    const order = { left_only: 0, right_only: 1, diff: 2, same: 3 }
    entries.value = [...r.entries].sort((a, b) => order[a.status] - order[b.status])
  } catch (e) {
    toast.bad(normalizeError(e).message)
  } finally {
    loading.value = false
  }
}

/** 点一行：用系统默认程序打开存在的那一侧。 */
function openSide(e: DirDiffEntryVO) {
  const sep = left.value.includes('\\') ? '\\' : '/'
  const rel = e.rel_path.replace(/\//g, sep)
  const side = e.status === 'right_only' ? right.value : left.value
  void api.openPath(`${side}${sep}${rel}`)
}
</script>

<style scoped>
.nt-dd {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
  min-height: 0;
}

.nt-dd-bar {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  flex: none;
  flex-wrap: wrap;
}

.nt-dd-path {
  width: 160px;
  flex: none;
}

.nt-dd-summary {
  flex: none;
  padding: var(--nt-sp-1) var(--nt-sp-2);
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-2);
  background: var(--nt-elev-surface);
  border-radius: var(--nt-radius-xs);
}

.nt-dd-list {
  flex: 1;
  min-height: 0;
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.nt-dd-row {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  width: 100%;
  padding: var(--nt-sp-1) var(--nt-sp-2);
  border-radius: var(--nt-radius-xs);
  font-size: var(--nt-fs-sm);
  text-align: left;
}

/* 四态用语义色的文字而不是底色：一屏几十行铺底色会花 */
.nt-dd-status {
  flex: none;
  width: 64px;
  font-size: var(--nt-fs-hint);
}

.nt-dd-row.is-left_only .nt-dd-status {
  color: var(--nt-warning);
}

.nt-dd-row.is-right_only .nt-dd-status {
  color: var(--nt-primary);
}

.nt-dd-row.is-diff .nt-dd-status {
  color: var(--nt-danger);
}

.nt-dd-row.is-same .nt-dd-status {
  color: var(--nt-ink-muted);
}

.nt-dd-rel {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono);
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink);
}

.nt-dd-row.is-same .nt-dd-rel {
  color: var(--nt-ink-muted);
}
</style>
