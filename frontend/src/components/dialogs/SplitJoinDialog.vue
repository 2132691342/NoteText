// 文件分卷 / 合并分卷浮窗：一个对话框两种模式。
// 合并的省事点：选中 .001 后自动扫同目录找全部 .NNN 兄弟卷，按序填满清单。
<template>
  <NtModal :title="mode === 'split' ? t('archive.splitTitle') : t('archive.joinTitle')" width="560"
    @close="emit('close')">
    <div class="nt-sj">
      <ElRadioGroup v-model="mode" size="small">
        <ElRadioButton value="split">{{ t('archive.splitTitle') }}</ElRadioButton>
        <ElRadioButton value="join">{{ t('archive.joinTitle') }}</ElRadioButton>
      </ElRadioGroup>

      <!-- 分卷 -->
      <template v-if="mode === 'split'">
        <div class="nt-field">
          <span class="nt-label">{{ t('archive.pickFile') }}</span>
          <div class="nt-sj-row">
            <ElInput v-model="filePath" size="small" readonly />
            <ElButton size="small" @click="pickFile">{{ t('common.browse') }}</ElButton>
          </div>
        </div>
        <div class="nt-sj-row">
          <div class="nt-field nt-grow">
            <span class="nt-label">{{ t('archive.partSizeMb') }}</span>
            <ElInput v-model="partMb" size="small" type="number" :min="1" />
          </div>
          <div class="nt-field nt-grow">
            <span class="nt-label">{{ t('archive.destDir') }}</span>
            <div class="nt-sj-row">
              <ElInput v-model="destDir" size="small" readonly />
              <ElButton size="small" @click="pickDir">{{ t('common.browse') }}</ElButton>
            </div>
          </div>
        </div>
        <div class="nt-sj-actions">
          <ElButton size="small" type="primary" :disabled="!canSplit" :loading="busy"
            @click="doSplit">{{ t('archive.splitTitle') }}</ElButton>
        </div>
      </template>

      <!-- 合并 -->
      <template v-else>
        <div class="nt-field">
          <span class="nt-label">{{ t('tree.joinParts') }}</span>
          <div class="nt-sj-row">
            <ElInput v-model="firstPart" size="small" readonly placeholder=".001" />
            <ElButton size="small" @click="pickFirstPart">{{ t('common.browse') }}</ElButton>
          </div>
          <p v-if="joinParts.length > 0" class="nt-sj-found">
            {{ t('archive.partsFound', { n: joinParts.length }) }}
          </p>
        </div>
        <div class="nt-field">
          <span class="nt-label">{{ t('archive.joinDestName') }}</span>
          <ElInput v-model="joinDestName" size="small" />
        </div>
        <div class="nt-field">
          <span class="nt-label">{{ t('archive.destDir') }}</span>
          <div class="nt-sj-row">
            <ElInput v-model="destDir" size="small" readonly />
            <ElButton size="small" @click="pickDir">{{ t('common.browse') }}</ElButton>
          </div>
        </div>
        <div class="nt-sj-actions">
          <ElButton size="small" type="primary" :disabled="!canJoin" :loading="busy"
            @click="doJoin">{{ t('archive.joinTitle') }}</ElButton>
        </div>
      </template>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElInput, ElRadioButton, ElRadioGroup } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { api, normalizeError } from '@/api'
import { useDialogStore } from '@/stores/dialogs'
import { useToastStore } from '@/stores/toast'
import { baseName, dirName, joinPath } from '@/utils'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const dialogs = useDialogStore()
const toast = useToastStore()

const mode = ref<'split' | 'join'>((dialogs.seedExtra.mode as 'split' | 'join') ?? 'split')
const filePath = ref((dialogs.seedExtra.path as string) ?? '')
const firstPart = ref((dialogs.seedExtra.path as string) ?? '')
const joinParts = ref<string[]>([])
const joinDestName = ref('')
const partMb = ref('100')
const destDir = ref('')
const busy = ref(false)

const canSplit = computed(
  () => filePath.value && Number(partMb.value) >= 1 && destDir.value,
)
const canJoin = computed(
  () => joinParts.value.length >= 2 && joinDestName.value.trim() !== '' && destDir.value,
)

onMounted(() => {
  if (filePath.value && !destDir.value) destDir.value = dirName(filePath.value)
  if (/^\d{3}$/.test(firstPart.value.split('.').pop() ?? '')) {
    mode.value = 'join'
    scanParts(firstPart.value)
  }
})

/** 换了首卷：重扫兄弟卷并预填合并名。 */
watch(firstPart, (v) => {
  if (v && mode.value === 'join') scanParts(v)
})

/** 选中 .001 后自动扫齐同目录全部 .NNN：用户只点一次。 */
async function scanParts(first: string) {
  const dir = dirName(first)
  const stem = baseName(first).replace(/\.\d{3}$/, '')
  try {
    const entries = await api.readDir(dir)
    const parts = (entries ?? [])
      .map((e) => e.path)
      .filter((p) => {
        const name = baseName(p)
        return name.startsWith(`${stem}.`) && /^\d{3}$/.test(name.slice(stem.length + 1))
      })
      .sort()
    joinParts.value = parts
    joinDestName.value = stem
    if (!destDir.value) destDir.value = dir
  } catch {
    joinParts.value = [first]
    joinDestName.value = stem
  }
}

async function pickFile() {
  const p = await api.openFileDialog(destDir.value || undefined)
  if (p) {
    filePath.value = p
    if (!destDir.value) destDir.value = dirName(p)
  }
}
async function pickFirstPart() {
  const p = await api.openFileDialog(destDir.value || undefined)
  if (p) firstPart.value = p
}
async function pickDir() {
  const p = await api.openFolderDialog(destDir.value || undefined)
  if (p) destDir.value = p
}

async function doSplit() {
  busy.value = true
  try {
    await api.archive.splitFile({
      file: filePath.value,
      dest_dir: destDir.value,
      part_size: Math.round(Number(partMb.value) * 1024 * 1024),
    })
    toast.ok(t('archive.queued'))
    emit('close')
  } catch (e) {
    toast.bad(normalizeError(e).message)
  } finally {
    busy.value = false
  }
}

async function doJoin() {
  busy.value = true
  try {
    await api.archive.joinParts({
      parts: joinParts.value,
      dest: joinPath(destDir.value, joinDestName.value.trim()),
    })
    toast.ok(t('archive.queued'))
    emit('close')
  } catch (e) {
    toast.bad(normalizeError(e).message)
  } finally {
    busy.value = false
  }
}
</script>

<style scoped>
.nt-sj {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
}

.nt-field {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-1);
  min-width: 0;
}

.nt-label {
  font-size: var(--nt-fs-label);
  color: var(--nt-ink-2);
}

.nt-grow {
  flex: 1;
}

.nt-sj-row {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  min-width: 0;
}

.nt-sj-found {
  margin: 0;
  font-size: var(--nt-fs-hint);
  color: var(--nt-success);
}

.nt-sj-actions {
  display: flex;
  justify-content: flex-end;
}
</style>
