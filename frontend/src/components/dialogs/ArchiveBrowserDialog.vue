// 压缩包浏览浮窗：条目表 + 增删 / 测试 / 解压 / 转换 / 预览。
// 密码在浮窗内解决：9002 / 9003 不关窗，给一行密码框重试，用户不用从头再开。
<template>
  <NtModal :title="t('archive.browserTitle')" width="860" @close="emit('close')">
    <div class="nt-barch">
      <!-- 头：文件名 + 徽标 + 汇总 -->
      <div class="nt-barch-head">
        <span class="nt-barch-name" :title="path">{{ baseName(path) }}</span>
        <span v-if="info?.encrypted" class="nt-badge is-warn">{{ t('archive.encryptedBadge') }}</span>
        <span v-if="info?.encrypted_names" class="nt-badge is-warn">
          {{ t('archive.encryptedNamesBadge') }}
        </span>
        <span v-if="info?.solid" class="nt-badge">{{ t('archive.solidBadge') }}</span>
        <span v-if="parts.length > 1" class="nt-badge">
          {{ t('archive.splitBadge', { n: parts.length }) }}
        </span>
        <span v-if="info" class="nt-barch-summary">
          {{ t('archive.summary', {
            files: info.file_count,
            dirs: info.dir_count,
            size: formatSize(info.total_size),
            packed: formatSize(info.packed_size),
          }) }}
        </span>
        <span v-if="ratioText" class="nt-barch-ratio">{{ ratioText }}</span>
      </div>

      <!-- 密码行：打开失败（要密码 / 密码错）时出现 -->
      <div v-if="needPwd" class="nt-barch-pwd">
        <ElInput v-model="password" size="small" type="password" show-password
          :placeholder="t('archive.needPassword')" @keyup.enter="open" />
        <ElButton size="small" type="primary" :loading="opening" @click="open">
          {{ t('archive.decrypt') }}
        </ElButton>
      </div>

      <!-- 操作行 -->
      <div class="nt-barch-ops">
        <ElInput v-model="filter" size="small" :placeholder="t('archive.filterPlaceholder')"
          class="nt-barch-filter" clearable />
        <ElButton size="small" type="primary" @click="extract(checked.size > 0)">
          {{ checked.size > 0 ? t('archive.extractSelected', { n: selectedFileCount }) : t('archive.extractAll') }}
        </ElButton>
        <ElButton size="small" @click="test">{{ t('archive.testNow') }}</ElButton>
        <ElButton size="small" @click="pickAdd" :disabled="opening">{{ t('archive.addMore') }}</ElButton>
        <ElButton size="small" :disabled="checked.size === 0" @click="onDelete">
          {{ t('archive.deleteSelected') }}
        </ElButton>
        <div class="nt-barch-conv">
          <ElSelect v-model="convertTo" size="small" class="nt-barch-convsel">
            <ElOption v-for="f in writeFormats" :key="f.id" :value="f.id" :label="t('archive.convertLabel') + ' ' + f.id" />
          </ElSelect>
          <ElButton size="small" :disabled="!convertTo || converting" :loading="converting" @click="onConvert">
            {{ t('common.apply') }}
          </ElButton>
        </div>
      </div>

      <!-- 条目表：表头固定在滚动区外，正文虚拟滚动（几万条目只渲染可见行） -->
      <div class="nt-barch-table">
        <div class="nt-barch-tr is-head">
          <label class="nt-barch-check">
            <input type="checkbox" :checked="allChecked" @change="toggleAll" />
          </label>
          <span>{{ t('archive.colName') }}</span>
          <span>{{ t('archive.colSize') }}</span>
          <span>{{ t('archive.colPacked') }}</span>
          <span>%</span>
          <span>{{ t('archive.colMethod') }}</span>
          <span>{{ t('archive.colModified') }}</span>
        </div>
        <div v-if="opening && !info" class="nt-barch-loading">
          {{ t('archive.opening') }}
        </div>
        <div v-else ref="host" class="nt-vlist nt-barch-rows" @scroll.passive="onScroll">
          <div class="nt-vlist-pad" :style="{ height: `${totalHeight}px` }"></div>
          <div class="nt-vlist-window" :style="{ transform: `translateY(${offset}px)` }">
            <label v-for="e in windowItems" :key="e.name" class="nt-barch-tr"
              :class="{ 'is-dir': e.is_dir }" :style="{ height: `${rowHeight}px` }" :title="e.name">
              <span class="nt-barch-check" @click.prevent="toggle(e.name)">
                <input type="checkbox" :checked="checked.has(e.name)" />
              </span>
              <span class="nt-barch-entry" @click.stop="onEntryClick(e)">
                <NtIcon :name="e.is_dir ? 'folder' : 'file'" size="sm" />
                <span class="nt-barch-ename">{{ shortName(e.name) }}</span>
                <span v-if="e.encrypted" class="nt-barch-dot" :title="t('archive.encryptedBadge')" />
              </span>
              <span class="nt-barch-num">{{ e.is_dir ? '—' : formatSize(e.size) }}</span>
              <span class="nt-barch-num">{{ e.is_dir ? '—' : formatSize(e.packed_size) }}</span>
              <span class="nt-barch-num">{{ e.ratio != null ? Math.round(e.ratio * 100) : '—' }}</span>
              <span class="nt-barch-method">{{ e.method ?? '—' }}</span>
              <span class="nt-barch-num">{{ e.modified ? formatTime(e.modified) : '—' }}</span>
            </label>
          </div>
          <div v-if="visibleEntries.length === 0" class="nt-barch-empty">
            {{ filter ? t('common.empty') : t('archive.emptyArchive') }}
          </div>
        </div>
      </div>

      <!-- 预览区：点文件条目时出现在表格下方 -->
      <div v-if="preview || previewLoading" class="nt-barch-preview">
        <div v-if="previewLoading" class="nt-barch-pvhint">{{ t('archive.previewLoading') }}</div>
        <template v-else>
          <div class="nt-barch-pvhead">
            <span>{{ t('archive.previewOf', { name: shortName(preview!.name) }) }}</span>
            <button class="nt-icon-btn" @click="preview = null">
              <NtIcon name="x" size="sm" />
            </button>
          </div>
          <img v-if="preview!.kind === 'image' && preview!.data_url" :src="preview!.data_url" class="nt-barch-pvimg" />
          <pre v-else-if="preview!.kind === 'text'" class="nt-barch-pvtext">{{ preview!.content ?? '' }}</pre>
          <p v-else class="nt-barch-pvnone">
            {{ preview!.too_large ? t('archive.previewTooLarge') : t('archive.previewOther') }}
          </p>
          <p v-if="preview!.truncated" class="nt-barch-pvnone">{{ t('archive.previewTruncated') }}</p>
        </template>
      </div>

      <!-- 解压选项 -->
      <div class="nt-barch-exopts">
        <span class="nt-label">{{ t('archive.extractTo') }}</span>
        <ElInput v-model="extractDir" size="small" readonly class="nt-barch-exdir" />
        <ElButton size="small" @click="pickExtractDir">{{ t('common.browse') }}</ElButton>
        <ElSelect v-model="overwrite" size="small" class="nt-barch-owsel">
          <ElOption value="overwrite" :label="t('archive.owOverwrite')" />
          <ElOption value="skip" :label="t('archive.owSkip')" />
          <ElOption value="rename" :label="t('archive.owRename')" />
        </ElSelect>
        <ElCheckbox v-model="createSubdir" size="small">{{ t('archive.createSubdir') }}</ElCheckbox>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElCheckbox, ElInput, ElOption, ElSelect } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import NtIcon from '@/components/common/NtIcon.vue'
import { useVirtualList } from '@/composables/useVirtualList'
import { api, normalizeError } from '@/api'
import { useDialogStore } from '@/stores/dialogs'
import { useToastStore } from '@/stores/toast'
import { baseName, dirName, formatSize, formatTime, joinPath } from '@/utils'
import type { ArchiveEntryVO, ArchiveFormatVO, ArchiveInfoVO, PreviewEntryVO } from '@/types/api'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const toast = useToastStore()
const dialogs = useDialogStore()

/** 打开目标：命令入口 / 树右键经 dialogs.open('archiveBrowser', '', { path }) 传入。 */
const path = ref((dialogs.seedExtra.path as string) ?? '')
const info = ref<ArchiveInfoVO | null>(null)
const parts = ref<string[]>([])
const password = ref((dialogs.seedExtra.password as string) ?? '')
const needPwd = ref(false)
const opening = ref(false)
const filter = ref('')
const checked = ref<Set<string>>(new Set())
const preview = ref<PreviewEntryVO | null>(null)
const previewLoading = ref(false)
const extractDir = ref('')
const overwrite = ref('overwrite')
const createSubdir = ref(true)
const convertTo = ref('')
const converting = ref(false)

const formats = ref<ArchiveFormatVO[]>([])
void api.archive.formats().then((fs) => {
  formats.value = fs ?? []
})
const archiveExts = computed(() => new Set(formats.value.flatMap((f) => f.extensions)))

/** 剥掉末尾最多两层压缩后缀（.tar.gz 两层 / .tgz 一层），与后端 strip_archive_ext 对齐。 */
function stripArchiveExt(name: string): string {
  let s = name
  for (let i = 0; i < 2; i++) {
    const m = s.match(/\.([^.\\/]+)$/)
    if (!m || !archiveExts.value.has(m[1].toLowerCase())) break
    s = s.slice(0, -m[0].length)
  }
  return s || name
}
const writeFormats = computed(() => formats.value.filter((f) => f.capabilities.write))
const ratioText = computed(() =>
  info.value?.ratio != null ? t('archive.ratio', { v: Math.round(info.value.ratio * 100) }) : '',
)

const visibleEntries = computed(() => {
  const list = info.value?.entries ?? []
  const q = filter.value.trim().toLowerCase()
  return q ? list.filter((e) => e.name.toLowerCase().includes(q)) : list
})
const { host, totalHeight, windowItems, offset, onScroll, rowHeight } = useVirtualList(
  visibleEntries,
  28,
)
const selectedFileCount = computed(
  () => visibleEntries.value.filter((e) => !e.is_dir && checked.value.has(e.name)).length,
)
const allChecked = computed(
  () => visibleEntries.value.length > 0 && visibleEntries.value.every((e) => checked.value.has(e.name)),
)

onMounted(() => {
  open()
  extractDir.value = dirName(path.value)
})

async function open() {
  opening.value = true
  try {
    const vo = await api.archive.open(path.value, password.value || undefined)
    info.value = vo.info
    parts.value = vo.parts
    needPwd.value = false
    // 分卷序列：切回首卷路径，增删解压都落在首卷
    if (vo.parts.length > 0) path.value = vo.parts[0]
    if (createSubdir.value) {
      extractDir.value = joinPath(dirName(path.value), stripArchiveExt(baseName(path.value)))
    }
  } catch (e) {
    const err = normalizeError(e)
    if (err.code === '9002' || err.code === '9003') {
      needPwd.value = true
      toast.warn(err.message)
    } else {
      toast.bad(err.message)
      emit('close')
    }
  } finally {
    opening.value = false
  }
}

function toggle(name: string) {
  const next = new Set(checked.value)
  if (next.has(name)) next.delete(name)
  else next.add(name)
  checked.value = next
}
function toggleAll() {
  if (allChecked.value) {
    checked.value = new Set()
  } else {
    checked.value = new Set(visibleEntries.value.map((e) => e.name))
  }
}

function shortName(name: string): string {
  return name.endsWith('/') ? name.slice(0, -1) : name
}

async function extract(selectedOnly: boolean) {
  try {
    await api.archive.extract({
      archive: path.value,
      dest: extractDir.value,
      entries: selectedOnly ? [...checked.value] : null,
      password: password.value || null,
      overwrite: overwrite.value,
      create_subdir: createSubdir.value,
    })
    toast.ok(t('archive.queued'))
    emit('close')
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}

async function test() {
  try {
    await api.archive.test({ archive: path.value, password: password.value || null })
    toast.ok(t('archive.queued'))
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}

async function pickAdd() {
  const p = await api.openFileDialog(dirName(path.value))
  if (!p) return
  try {
    await api.archive.add({
      archive: path.value,
      sources: [p],
      password: password.value || null,
      exclude: [],
    })
    toast.ok(t('archive.queued'))
    emit('close')
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}

async function onDelete() {
  const names = [...checked.value]
  if (names.length === 0) return
  const ok = await api.showConfirm(t('common.confirm'), t('archive.deleteConfirmBody', { n: names.length }))
  if (!ok) return
  try {
    await api.archive.deleteEntries({
      archive: path.value,
      entries: names,
      password: password.value || null,
    })
    toast.ok(t('archive.queued'))
    emit('close')
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}

async function onConvert() {
  converting.value = true
  try {
    const ext = writeFormats.value.find((f) => f.id === convertTo.value)?.extensions[0] ?? 'zip'
    const stem = stripArchiveExt(baseName(path.value))
    await api.archive.convert({
      archive: path.value,
      dest: joinPath(dirName(path.value), `${stem}.${ext}`),
      format: convertTo.value,
      password: password.value || null,
      new_password: null,
      level: 5,
    })
    toast.ok(t('archive.queued'))
    emit('close')
  } catch (e) {
    toast.bad(normalizeError(e).message)
  } finally {
    converting.value = false
  }
}

async function onEntryClick(e: ArchiveEntryVO) {
  if (e.is_dir) {
    filter.value = shortName(e.name)
    return
  }
  if (preview.value?.name === e.name) {
    preview.value = null
    return
  }
  // 固实 7z 预览单条目要解整块：可能要等几秒，给出 loading 而不是无反馈
  previewLoading.value = true
  try {
    preview.value = await api.archive.previewEntry({
      archive: path.value,
      entry: e.name,
      password: password.value || null,
    })
  } catch (err) {
    toast.bad(normalizeError(err).message)
  } finally {
    previewLoading.value = false
  }
}

async function pickExtractDir() {
  const p = await api.openFolderDialog(extractDir.value)
  if (p) {
    extractDir.value = p
    createSubdir.value = false
  }
}
</script>

<style scoped>
.nt-barch {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
  min-height: 0;
}

.nt-barch-head {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  flex-wrap: wrap;
}

.nt-barch-name {
  font-size: var(--nt-fs-md);
  font-weight: 600;
  color: var(--nt-ink);
}

.nt-barch-summary {
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-3);
  font-variant-numeric: tabular-nums;
}

.nt-barch-ratio {
  font-size: var(--nt-fs-hint);
  color: var(--nt-success);
  font-variant-numeric: tabular-nums;
}

.nt-badge {
  padding: 1px var(--nt-sp-2);
  border-radius: var(--nt-radius-full);
  font-size: var(--nt-fs-2xs);
  color: var(--nt-ink-2);
  background: var(--nt-surface-hover);
  border: 1px solid var(--nt-border);
}

.nt-badge.is-warn {
  color: var(--nt-warning);
  border-color: var(--nt-warning);
  background: var(--nt-warning-bg);
}

.nt-barch-pwd {
  display: flex;
  gap: var(--nt-sp-2);
}

.nt-barch-ops {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  flex-wrap: wrap;
}

.nt-barch-filter {
  width: 180px;
}

.nt-barch-conv {
  display: flex;
  gap: var(--nt-sp-1);
  margin-left: auto;
}

.nt-barch-convsel {
  width: 150px;
}

.nt-barch-table {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius-sm);
  overflow: hidden;
}

.nt-barch-rows {
  max-height: 42vh;
}

.nt-barch-tr {
  display: grid;
  grid-template-columns: 26px minmax(180px, 1fr) 76px 76px 48px 70px 110px;
  align-items: center;
  gap: var(--nt-sp-2);
  width: 100%;
  padding: 0 var(--nt-sp-2);
  box-sizing: border-box;
  font-size: var(--nt-fs-sm);
  cursor: pointer;
}

.nt-barch-tr:hover {
  background: var(--nt-surface-hover);
}

.nt-barch-tr.is-head {
  background: var(--nt-elev-surface);
  color: var(--nt-ink-3);
  font-size: var(--nt-fs-hint);
  cursor: default;
  height: auto;
  flex: none;
  padding: var(--nt-sp-1) var(--nt-sp-2);
}

.nt-barch-loading {
  padding: var(--nt-sp-6);
  text-align: center;
  color: var(--nt-ink-3);
  font-size: var(--nt-fs-sm);
}

.nt-barch-empty {
  padding: var(--nt-sp-6);
  text-align: center;
  color: var(--nt-ink-muted);
  font-size: var(--nt-fs-sm);
}

.nt-barch-tr.is-dir .nt-barch-ename {
  color: var(--nt-ink-2);
  font-weight: 500;
}

.nt-barch-check {
  display: flex;
  align-items: center;
}

.nt-barch-entry {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-1);
  min-width: 0;
}

.nt-barch-ename {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--nt-ink);
}

.nt-barch-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--nt-warning);
  flex: none;
}

.nt-barch-num {
  font-variant-numeric: tabular-nums;
  color: var(--nt-ink-2);
  text-align: right;
}

.nt-barch-method {
  font-family: var(--font-mono);
  font-size: var(--nt-fs-2xs);
  color: var(--nt-ink-3);
}

.nt-barch-preview {
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius-sm);
  max-height: 24vh;
  overflow: auto;
}

.nt-barch-pvhead {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--nt-sp-1) var(--nt-sp-2);
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-2);
  border-bottom: 1px solid var(--nt-border);
  position: sticky;
  top: 0;
  background: var(--nt-elev-surface);
}

.nt-barch-pvhint {
  padding: var(--nt-sp-3);
  text-align: center;
  color: var(--nt-ink-3);
  font-size: var(--nt-fs-sm);
}

.nt-barch-pvtext {
  margin: 0;
  padding: var(--nt-sp-2);
  font-family: var(--font-mono);
  font-size: var(--nt-fs-xs);
  white-space: pre-wrap;
  word-break: break-all;
  color: var(--nt-ink);
}

.nt-barch-pvimg {
  display: block;
  max-width: 100%;
  max-height: 20vh;
  margin: var(--nt-sp-2) auto;
}

.nt-barch-pvnone {
  padding: var(--nt-sp-3);
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-muted);
}

.nt-barch-exopts {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  flex-wrap: wrap;
}

.nt-barch-exdir {
  width: 260px;
}

.nt-barch-owsel {
  width: 120px;
}
</style>
