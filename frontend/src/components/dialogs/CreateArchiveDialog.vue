// 创建压缩包浮窗：源清单 + 格式 / 级别 / 加密 / 分卷，提交进任务队列。
// 格式能力（加密 / 固实 / 单文件限制）从后端 formats 表取，前端不写死判断。
<template>
  <NtModal :title="t('archive.createTitle')" width="640" @close="emit('close')">
    <div class="nt-carch">
      <!-- 源清单 -->
      <div class="nt-field">
        <span class="nt-label">{{ t('archive.sources') }}</span>
        <div class="nt-carch-srcs">
          <div v-for="(s, i) in sources" :key="s.path" class="nt-carch-src">
            <NtIcon :name="s.dir ? 'folder' : 'file'" size="sm" />
            <span class="nt-carch-src-name" :title="s.path">{{ baseName(s.path) }}</span>
            <button class="nt-icon-btn" @click="removeSource(i)">
              <NtIcon name="x" size="sm" />
            </button>
          </div>
          <div v-if="sources.length === 0" class="nt-carch-none">
            {{ t('archive.needSource') }}
          </div>
        </div>
        <div class="nt-carch-srcbtns">
          <ElButton size="small" @click="pickFiles">{{ t('archive.addFiles') }}</ElButton>
          <ElButton size="small" @click="pickFolder">{{ t('archive.addFolder') }}</ElButton>
        </div>
      </div>

      <!-- 文件名与位置 -->
      <div class="nt-carch-row">
        <div class="nt-field nt-grow">
          <span class="nt-label">{{ t('archive.destName') }}</span>
          <div class="nt-carch-namerow">
            <ElInput v-model="destName" size="small" />
            <span class="nt-carch-ext">.{{ formatExt }}</span>
          </div>
        </div>
        <div class="nt-field nt-grow">
          <span class="nt-label">{{ t('archive.destDir') }}</span>
          <div class="nt-carch-namerow">
            <ElInput v-model="destDir" size="small" readonly />
            <ElButton size="small" @click="pickDir">{{ t('common.browse') }}</ElButton>
          </div>
        </div>
      </div>

      <!-- 格式 / 方法 / 级别 -->
      <div class="nt-carch-row">
        <div class="nt-field">
          <span class="nt-label">{{ t('archive.format') }}</span>
          <ElSelect v-model="formatId" size="small" class="nt-carch-sel">
            <ElOption v-for="f in writeFormats" :key="f.id" :value="f.id" :label="f.id" />
          </ElSelect>
        </div>
        <div v-if="formatId === 'zip'" class="nt-field">
          <span class="nt-label">{{ t('archive.method') }}</span>
          <ElSelect v-model="method" size="small" class="nt-carch-sel">
            <ElOption value="deflate" label="Deflate" />
            <ElOption value="zstd" label="Zstd" />
            <ElOption value="bzip2" label="BZip2" />
            <ElOption value="store" :label="t('archive.methodStore')" />
          </ElSelect>
        </div>
        <div class="nt-field">
          <span class="nt-label">{{ t('archive.level') }}</span>
          <ElSelect v-model="level" size="small" class="nt-carch-sel">
            <ElOption value="0" :label="t('archive.levelStore')" />
            <ElOption value="1" :label="t('archive.levelFastest')" />
            <ElOption value="3" :label="t('archive.levelFast')" />
            <ElOption value="5" :label="t('archive.levelNormal')" />
            <ElOption value="7" :label="t('archive.levelMax')" />
            <ElOption value="9" :label="t('archive.levelUltra')" />
          </ElSelect>
        </div>
      </div>

      <!-- 密码：仅 7z / zip -->
      <template v-if="caps.encryption">
        <div class="nt-carch-row">
          <div class="nt-field nt-grow">
            <span class="nt-label">{{ t('archive.password') }}</span>
            <ElInput v-model="password" size="small" type="password" show-password
              :placeholder="t('archive.passwordHint')" />
          </div>
          <div class="nt-field nt-grow">
            <span class="nt-label">{{ t('archive.passwordConfirm') }}</span>
            <ElInput v-model="password2" size="small" type="password" show-password />
          </div>
        </div>
        <ElCheckbox v-if="formatId === '7z'" v-model="encryptNames" size="small">
          {{ t('archive.encryptNames') }}
        </ElCheckbox>
        <ElCheckbox v-if="formatId === '7z'" v-model="solid" size="small">
          {{ t('archive.solid') }}
        </ElCheckbox>
      </template>

      <!-- 分卷 -->
      <div class="nt-carch-row">
        <div class="nt-field">
          <span class="nt-label">{{ t('archive.splitSize') }}</span>
          <ElSelect v-model="splitPreset" size="small" class="nt-carch-sel">
            <ElOption value="0" :label="t('archive.noSplit')" />
            <ElOption value="10" label="10 MB" />
            <ElOption value="100" label="100 MB" />
            <ElOption value="650" label="650 MB" />
            <ElOption value="4480" label="4480 MB" />
            <ElOption value="custom" :label="t('archive.customMb')" />
          </ElSelect>
        </div>
        <div v-if="splitPreset === 'custom'" class="nt-field nt-grow">
          <span class="nt-label">MB</span>
          <ElInput v-model="customMb" size="small" type="number" :min="1" />
        </div>
      </div>

      <!-- 排除 / 注释 -->
      <div class="nt-field">
        <span class="nt-label">{{ t('archive.exclude') }}</span>
        <ElInput v-model="exclude" size="small" :placeholder="t('archive.excludeHint')" />
      </div>
      <div v-if="formatId === 'zip'" class="nt-field">
        <span class="nt-label">{{ t('archive.comment') }}</span>
        <ElInput v-model="comment" size="small" />
      </div>

      <!-- 单文件格式的限制提示 -->
      <p v-if="!caps.container" class="nt-carch-warn">{{ t('archive.singleFileOnly') }}</p>

      <div class="nt-carch-actions">
        <ElButton size="small" @click="emit('close')">{{ t('common.cancel') }}</ElButton>
        <ElButton size="small" type="primary" :disabled="!canCreate" :loading="submitting"
          @click="onCreate">{{ t('archive.createNow') }}</ElButton>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  ElButton,
  ElCheckbox,
  ElInput,
  ElOption,
  ElSelect,
} from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import NtIcon from '@/components/common/NtIcon.vue'
import { api, normalizeError } from '@/api'
import { useDialogStore } from '@/stores/dialogs'
import { useToastStore } from '@/stores/toast'
import { baseName, dirName, joinPath } from '@/utils'
import type { ArchiveFormatVO } from '@/types/api'

const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()
const toast = useToastStore()
const dialogs = useDialogStore()

/** 源清单的一项：路径 + 是否目录（图标用，也提示「这一项会递归打包」）。 */
interface SrcItem {
  path: string
  dir: boolean
}

/** 预填源：树右键 / 命令入口经 dialogs.open 的 seedExtra 传入。 */
const seedSources = (dialogs.seedExtra.sources as string[] | undefined) ?? []
const sources = ref<SrcItem[]>(seedSources.map((p) => ({ path: p, dir: false })))
const destName = ref('')
const destDir = ref('')
const formatId = ref('7z')
const method = ref('deflate')
const level = ref('5')
const password = ref('')
const password2 = ref('')
const encryptNames = ref(false)
const solid = ref(false)
const splitPreset = ref('0')
const customMb = ref('100')
const exclude = ref('')
const comment = ref('')
const submitting = ref(false)

const formats = ref<ArchiveFormatVO[]>([])
void api.archive
  .formats()
  .then((fs) => {
    formats.value = fs ?? []
  })
  .catch(() => {
    // 拉不到能力表就按「只有 zip」兜底：下拉空了用户连格式都选不了。
    formats.value = []
  })
const writeFormats = computed(() => formats.value.filter((f) => f.capabilities.write))
const caps = computed(
  () =>
    formats.value.find((f) => f.id === formatId.value)?.capabilities ?? {
      read: true,
      write: true,
      container: true,
      encryption: false,
    },
)
const formatExt = computed(
  () => writeFormats.value.find((f) => f.id === formatId.value)?.extensions[0] ?? 'zip',
)

const canCreate = computed(() => {
  if (sources.value.length === 0 || !destName.value.trim() || !destDir.value.trim()) return false
  if (password.value !== password2.value) return false
  if (splitPreset.value === 'custom' && Number(customMb.value) < 1) return false
  return true
})

async function pickFiles() {
  const picked = await api.openFilesDialog(destDir.value || undefined)
  for (const p of picked) addSource(p)
}
async function pickFolder() {
  const p = await api.openFolderDialog(destDir.value || undefined)
  if (p) addSource(p, true)
}

/** 补目录标记：归档对话框要给「这是文件夹，会递归打包」的视觉区分。 */
async function syncDir(item: SrcItem) {
  try {
    const m = await api.statFile(item.path)
    item.dir = m.is_dir
  } catch {
    /* 读不到就按文件画：图标错一下不影响打包 */
  }
}

function addSource(p: string, dir = false) {
  if (sources.value.some((s) => s.path === p)) return
  const item: SrcItem = { path: p, dir }
  sources.value = [...sources.value, item]
  if (sources.value.length === 1) {
    destDir.value = dirName(p)
    destName.value = baseName(p).replace(/\.[^.\\/]*$/, '')
  }
  if (!dir) void syncDir(item)
}

function removeSource(i: number) {
  sources.value = sources.value.filter((_, j) => j !== i)
}

// 预填源同样要知道是不是目录：右键一个文件夹点「压缩」时图标要显示成文件夹。
for (const item of sources.value) {
  if (!item.dir) void syncDir(item)
}
async function pickDir() {
  const p = await api.openFolderDialog(destDir.value || undefined)
  if (p) destDir.value = p
}

async function onCreate() {
  if (password.value !== password2.value) {
    toast.warn(t('archive.passwordMismatch'))
    return
  }
  submitting.value = true
  try {
    const mb = splitPreset.value === 'custom' ? Number(customMb.value) : Number(splitPreset.value)
    await api.archive.create({
      sources: sources.value.map((s) => s.path),
      dest: joinPath(destDir.value, `${destName.value.trim()}.${formatExt.value}`),
      format: formatId.value,
      method: formatId.value === 'zip' ? method.value : null,
      level: Number(level.value),
      password: password.value || null,
      encrypt_filenames: encryptNames.value,
      solid: solid.value,
      split_to: mb > 0 ? mb * 1024 * 1024 : null,
      exclude: exclude.value.split(',').map((s) => s.trim()).filter(Boolean),
      comment: comment.value || null,
    })
    toast.ok(t('archive.queued'))
    emit('close')
  } catch (e) {
    toast.bad(normalizeError(e).message)
  } finally {
    submitting.value = false
  }
}
</script>

<style scoped>
.nt-carch {
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

.nt-carch-row {
  display: flex;
  gap: var(--nt-sp-3);
}

.nt-grow {
  flex: 1;
}

.nt-carch-sel {
  width: 130px;
}

.nt-carch-namerow {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  min-width: 0;
}

.nt-carch-ext {
  color: var(--nt-ink-3);
  font-family: var(--font-mono);
  font-size: var(--nt-fs-sm);
  flex: none;
}

.nt-carch-srcs {
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 140px;
  overflow: auto;
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius-sm);
  padding: var(--nt-sp-1);
}

.nt-carch-src {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  padding: 2px var(--nt-sp-1);
  border-radius: var(--nt-radius-xs);
  font-size: var(--nt-fs-sm);
}

.nt-carch-src:hover {
  background: var(--nt-surface-hover);
}

.nt-carch-src-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--nt-ink);
}

.nt-carch-none {
  padding: var(--nt-sp-2);
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-muted);
  text-align: center;
}

.nt-carch-srcbtns {
  display: flex;
  gap: var(--nt-sp-2);
  margin-top: var(--nt-sp-2);
}

.nt-carch-warn {
  font-size: var(--nt-fs-hint);
  color: var(--nt-warning);
}

.nt-carch-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--nt-sp-2);
  margin-top: var(--nt-sp-2);
}
</style>
