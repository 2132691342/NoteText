// 解压浮窗：选包 → 条目勾选 + 密码 → 目录 / 覆盖策略 → 一键解压。
// 与浏览浮窗的分工：这里只做「把包解开」这一件事，增删 / 转换仍在浏览浮窗。
<template>
  <NtModal :title="t('archive.extractTitle')" width="720" @close="emit('close')">
    <div class="nt-xarch">
      <!-- 压缩包：菜单入口带路径进来，也可以在这里现选 / 更换 -->
      <div class="nt-field">
        <span class="nt-label">{{ t('archive.pickArchive') }}</span>
        <div class="nt-xarch-row">
          <ElInput v-model="path" size="small" readonly :placeholder="t('archive.noArchive')" />
          <ElButton size="small" @click="pickArchive">{{ t('common.browse') }}</ElButton>
        </div>
      </div>

      <!-- 密码行：打开失败（要密码 / 密码错）时出现，输完原窗重试 -->
      <div v-if="needPwd" class="nt-xarch-pwd">
        <ElInput v-model="password" size="small" type="password" show-password
          :placeholder="t('archive.needPassword')" @keyup.enter="open" />
        <ElButton size="small" type="primary" :loading="opening" @click="open">
          {{ t('archive.decrypt') }}
        </ElButton>
      </div>

      <template v-if="info">
        <!-- 包信息：徽标 + 汇总 -->
        <div class="nt-xarch-head">
          <span v-if="info.encrypted" class="nt-badge is-warn">{{ t('archive.encryptedBadge') }}</span>
          <span v-if="info.encrypted_names" class="nt-badge is-warn">
            {{ t('archive.encryptedNamesBadge') }}
          </span>
          <span v-if="info.solid" class="nt-badge">{{ t('archive.solidBadge') }}</span>
          <span v-if="parts.length > 1" class="nt-badge">
            {{ t('archive.splitBadge', { n: parts.length }) }}
          </span>
          <span class="nt-xarch-summary">
            {{ t('archive.summary', {
              files: info.file_count,
              dirs: info.dir_count,
              size: formatSize(info.total_size),
              packed: formatSize(info.packed_size),
            }) }}
          </span>
        </div>

        <!-- 条目表：默认全解，勾选后只解勾中的 -->
        <div class="nt-field">
          <div class="nt-xarch-entryhead">
            <label class="nt-xarch-check">
              <input type="checkbox" :checked="allChecked" @change="toggleAll" />
              <span>{{ checked.size > 0 ? t('archive.entriesSelected', { n: checked.size }) : t('archive.allEntries') }}</span>
            </label>
            <ElInput v-model="filter" size="small" :placeholder="t('archive.filterPlaceholder')"
              class="nt-xarch-filter" clearable />
          </div>
          <div class="nt-xarch-table">
            <label v-for="e in visibleEntries" :key="e.name" class="nt-xarch-tr" :title="e.name">
              <span class="nt-xarch-check">
                <input type="checkbox" :checked="checked.has(e.name)" @change="toggle(e.name)" />
              </span>
              <span class="nt-xarch-entry">
                <NtIcon :name="e.is_dir ? 'folder' : 'file'" size="sm" />
                <span class="nt-xarch-ename">{{ shortName(e.name) }}</span>
                <span v-if="e.encrypted" class="nt-xarch-dot" :title="t('archive.encryptedBadge')" />
              </span>
              <span class="nt-xarch-num">{{ e.is_dir ? '—' : formatSize(e.size) }}</span>
            </label>
            <div v-if="visibleEntries.length === 0" class="nt-xarch-empty">
              {{ filter ? t('common.empty') : t('archive.emptyArchive') }}
            </div>
          </div>
        </div>
      </template>

      <!-- 解压选项：目录 + 覆盖策略 + 子文件夹 -->
      <div class="nt-field">
        <span class="nt-label">{{ t('archive.extractTo') }}</span>
        <div class="nt-xarch-row">
          <ElInput v-model="extractDir" size="small" readonly class="nt-grow" />
          <ElButton size="small" @click="pickDir">{{ t('common.browse') }}</ElButton>
        </div>
      </div>
      <div class="nt-xarch-row">
        <div class="nt-field">
          <span class="nt-label">{{ t('archive.overwrite') }}</span>
          <ElSelect v-model="overwrite" size="small" class="nt-xarch-owsel">
            <ElOption value="overwrite" :label="t('archive.owOverwrite')" />
            <ElOption value="skip" :label="t('archive.owSkip')" />
            <ElOption value="rename" :label="t('archive.owRename')" />
          </ElSelect>
        </div>
        <ElCheckbox v-model="createSubdir" size="small" class="nt-xarch-sub">
          {{ t('archive.createSubdir') }}
        </ElCheckbox>
      </div>

      <div class="nt-xarch-actions">
        <ElButton size="small" :disabled="!path" @click="test">{{ t('archive.testNow') }}</ElButton>
        <ElButton size="small" type="primary" :disabled="!canExtract" @click="extract">
          {{ checked.size > 0 ? t('archive.extractSelected', { n: checked.size }) : t('archive.extractNow') }}
        </ElButton>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElCheckbox, ElInput, ElOption, ElSelect } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import NtIcon from '@/components/common/NtIcon.vue'
import { api, normalizeError } from '@/api'
import { useDialogStore } from '@/stores/dialogs'
import { useToastStore } from '@/stores/toast'
import { baseName, dirName, formatSize, joinPath } from '@/utils'
import type { ArchiveInfoVO } from '@/types/api'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const toast = useToastStore()
const dialogs = useDialogStore()

/** 解压目标：命令入口 / 树右键经 dialogs.open('extractArchive', '', { path }) 传入。 */
const path = ref((dialogs.seedExtra.path as string) ?? '')
const info = ref<ArchiveInfoVO | null>(null)
const parts = ref<string[]>([])
const password = ref('')
const needPwd = ref(false)
const opening = ref(false)
const filter = ref('')
const checked = ref<Set<string>>(new Set())
const extractDir = ref('')
const overwrite = ref('overwrite')
const createSubdir = ref(true)

const visibleEntries = computed(() => {
  const list = info.value?.entries ?? []
  const q = filter.value.trim().toLowerCase()
  return q ? list.filter((e) => e.name.toLowerCase().includes(q)) : list
})
const allChecked = computed(
  () => visibleEntries.value.length > 0 && visibleEntries.value.every((e) => checked.value.has(e.name)),
)
const canExtract = computed(() => !!info.value && !!extractDir.value.trim())

onMounted(() => {
  if (path.value) void open()
})

/** 已打开过的路径：分卷包打开时会把 path 归一到首卷，那不该再触发一次重开。 */
const openedPath = ref('')

/** 换包：清掉上一包的状态（勾选 / 密码 / 信息），再读新包。 */
watch(path, (v) => {
  if (!v || v === openedPath.value) return
  info.value = null
  parts.value = []
  checked.value = new Set()
  needPwd.value = false
  filter.value = ''
  void open()
})

async function open() {
  opening.value = true
  try {
    const vo = await api.archive.open(path.value, password.value || undefined)
    info.value = vo.info
    parts.value = vo.parts
    needPwd.value = false
    // 分卷序列：解压落在首卷。
    if (vo.parts.length > 0) {
      openedPath.value = vo.parts[0]
      path.value = vo.parts[0]
    } else {
      openedPath.value = path.value
    }
    resetDest()
  } catch (e) {
    const err = normalizeError(e)
    if (err.code === '9002' || err.code === '9003') {
      needPwd.value = true
      toast.warn(err.message)
    } else {
      toast.bad(err.message)
    }
  } finally {
    opening.value = false
  }
}

/** 默认目标：包同级目录；勾「同名子文件夹」时再往里一层。 */
function resetDest() {
  const dir = dirName(path.value)
  const stem = baseName(path.value).replace(/\.[^.\\/]+$/, '')
  extractDir.value = createSubdir.value ? joinPath(dir, stem) : dir
  destAuto.value = true
}

/** extractDir 是否还是自动推导值：用户手选过目录后，开关子文件夹不再覆盖它。 */
const destAuto = ref(true)

/** 子文件夹开关只重算默认目标：用户手选过目录就别覆盖他的选择。 */
watch(createSubdir, () => {
  if (path.value && destAuto.value) resetDest()
})

async function pickArchive() {
  const p = await api.openFileDialog(path.value ? dirName(path.value) : undefined)
  if (p) path.value = p
}

async function pickDir() {
  const p = await api.openFolderDialog(extractDir.value || undefined)
  if (p) {
    extractDir.value = p
    destAuto.value = false
    createSubdir.value = false
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

async function extract() {
  try {
    await api.archive.extract({
      archive: path.value,
      dest: extractDir.value,
      entries: checked.value.size > 0 ? [...checked.value] : null,
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

/** 测试不关窗：结果由任务 toast 说，密码对不对也能在这里先验证。 */
async function test() {
  try {
    await api.archive.test({ archive: path.value, password: password.value || null })
    toast.ok(t('archive.queued'))
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}
</script>

<style scoped>
.nt-xarch {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
  min-height: 0;
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

.nt-xarch-row {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
}

.nt-xarch-pwd {
  display: flex;
  gap: var(--nt-sp-2);
}

.nt-xarch-head {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  flex-wrap: wrap;
}

.nt-xarch-summary {
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-3);
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

.nt-xarch-entryhead {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--nt-sp-2);
}

.nt-xarch-check {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-1);
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-2);
  cursor: pointer;
}

.nt-xarch-filter {
  width: 180px;
}

.nt-xarch-table {
  display: flex;
  flex-direction: column;
  max-height: 30vh;
  overflow: auto;
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius-sm);
}

.nt-xarch-tr {
  display: grid;
  grid-template-columns: 24px minmax(200px, 1fr) 80px;
  align-items: center;
  gap: var(--nt-sp-2);
  padding: 2px var(--nt-sp-2);
  font-size: var(--nt-fs-sm);
  cursor: pointer;
}

.nt-xarch-tr:hover {
  background: var(--nt-surface-hover);
}

.nt-xarch-entry {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-1);
  min-width: 0;
}

.nt-xarch-ename {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--nt-ink);
}

.nt-xarch-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--nt-warning);
  flex: none;
}

.nt-xarch-num {
  font-variant-numeric: tabular-nums;
  color: var(--nt-ink-2);
  text-align: right;
}

.nt-xarch-empty {
  padding: var(--nt-sp-6);
  text-align: center;
  color: var(--nt-ink-muted);
  font-size: var(--nt-fs-sm);
}

.nt-xarch-owsel {
  width: 130px;
}

.nt-xarch-sub {
  margin-left: var(--nt-sp-3);
}

.nt-xarch-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--nt-sp-2);
  margin-top: var(--nt-sp-1);
}
</style>
