// 设置面板：关于（版本 / 数据目录 / 备份导入导出）。
//
// 备份包含书签 / 片段 / 宏 / 草稿 / 最近文件——用户自己攒的资产。
// 不含 settings：那是本机偏好，换机器不一定要带。
<template>
  <div class="nt-set">
    <section class="nt-set-section">
      <h2 class="nt-set-title">{{ t('settings.about') }}</h2>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.version') }}</span>
        <span class="nt-readout">v{{ info?.version ?? '—' }}</span>
      </div>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.dataDir') }}</span>
        <span class="nt-set-path" :title="info?.data_dir">{{ info?.data_dir }}</span>
        <ElButton size="small" text @click="openDir(info?.data_dir)">
          {{ t('settings.openDir') }}
        </ElButton>
      </div>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.logDir') }}</span>
        <span class="nt-set-path" :title="info?.log_dir">{{ info?.log_dir }}</span>
        <ElButton size="small" text @click="openDir(info?.log_dir)">
          {{ t('settings.openDir') }}
        </ElButton>
      </div>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.exportBackup') }}</span>
        <ElButton size="small" :loading="busy" @click="exportBackup">
          {{ t('settings.exportBackup') }}
        </ElButton>
        <ElButton size="small" :loading="busy" @click="importBackup">
          {{ t('settings.importBackup') }}
        </ElButton>
      </div>

      <p class="nt-set-note">
        {{ t('settings.backupScope') }}
      </p>
    </section>
  </div>
</template>

<script lang="ts" setup>
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElMessage } from 'element-plus'
import { api, normalizeError } from '@/api'
import { useToastStore } from '@/stores/toast'
import type { AppInfoVO } from '@/types/api'

const { t } = useI18n()
const toast = useToastStore()

const info = ref<AppInfoVO | null>(null)
const busy = ref(false)

onMounted(async () => {
  try {
    info.value = await api.appInfo()
  } catch {
    /* 关于页拿不到版本号不影响其他设置 */
  }
})

async function openDir(dir?: string) {
  if (!dir) return
  try {
    await api.openPath(dir)
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}

async function exportBackup() {
  busy.value = true
  try {
    const json = await api.exportBackup()
    const text = JSON.stringify(json, null, 2)
    const path = await api.saveFileDialog('notetext-backup.json')
    if (!path) return
    // 备份是 UTF-8 JSON：显式给编码与 LF 行尾，走同一条写盘链路（含原子写）。
    await api.writeFile(path, text, 'utf-8', 'lf', false)
    toast.ok(t('toast.backupExported', { path }))
  } catch (e) {
    toast.bad(`${t('settings.importFailed', { reason: normalizeError(e).message })}`)
  } finally {
    busy.value = false
  }
}

async function importBackup() {
  busy.value = true
  try {
    const path = await api.openFileDialog()
    if (!path) return
    // 备份可能带 BOM 或是 GBK 存过的：交给后端的编码探测，别自己解。
    const { text } = await api.readFile(path)
    const json = JSON.parse(text)
    const n = await api.importBackup(json)
    ElMessage.success(t('settings.importDone', { count: n }))
    // 导入后刷新各面板：书签 / 片段 / 宏可能都变了。
    document.dispatchEvent(new CustomEvent('bookmarks-changed'))
    document.dispatchEvent(new CustomEvent('snippets-changed'))
  } catch (e) {
    toast.bad(`${t('settings.importFailed', { reason: normalizeError(e).message })}`)
  } finally {
    busy.value = false
  }
}
</script>

<style scoped>
.nt-set {
  max-width: 640px;
}

.nt-set-section {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
}

.nt-set-title {
  font-size: var(--nt-fs-md);
  font-weight: 600;
  color: var(--nt-ink);
  padding-bottom: var(--nt-sp-2);
  border-bottom: 1px solid var(--nt-border);
}

.nt-set-row {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
}

.nt-set-label {
  flex: none;
  min-width: 220px;
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-2);
}

.nt-set-path {
  flex: 1;
  min-width: 0;
  font-family: var(--font-mono);
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-2);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.nt-set-note {
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-muted);
}
</style>
