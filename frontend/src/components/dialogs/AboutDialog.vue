// 关于浮窗：版本、数据目录、日志目录、运行环境。
//
// 排障第一步是「日志在哪」——把它直接摆在关于里，用户自己能找到文件，
// 比让用户描述环境快得多。
<template>
  <NtModal :title="t('menu.about')" width="440" @close="emit('close')">
    <div class="nt-about">
      <div class="nt-about-brand">
        <span class="nt-about-logo">N</span>
        <div>
          <div class="nt-about-name">NoteText</div>
          <div class="nt-readout">v{{ info?.version ?? '—' }}</div>
        </div>
      </div>

      <p class="nt-about-desc">
        轻量级 Windows 桌面文本编辑器。Tauri 2 + Rust + Vue 3。
      </p>

      <div class="nt-about-rows">
        <div class="nt-about-row">
          <span class="nt-label">{{ t('settings.dataDir') }}</span>
          <span class="nt-about-path" :title="info?.data_dir">{{ info?.data_dir }}</span>
          <ElButton size="small" text @click="openDir(info?.data_dir)">
            {{ t('settings.openDir') }}
          </ElButton>
        </div>
        <div class="nt-about-row">
          <span class="nt-label">{{ t('settings.logDir') }}</span>
          <span class="nt-about-path" :title="info?.log_dir">{{ info?.log_dir }}</span>
          <ElButton size="small" text @click="openDir(info?.log_dir)">
            {{ t('settings.openDir') }}
          </ElButton>
        </div>
        <div class="nt-about-row">
          <span class="nt-label">{{ t('common.system') }}</span>
          <span class="nt-readout">{{ info?.os }} / {{ info?.arch }}</span>
        </div>
      </div>

      <div class="nt-about-actions">
        <ElButton size="small" @click="router.push('/help')">{{ t('menu.userManual') }}</ElButton>
        <ElButton size="small" @click="emit('close')">{{ t('common.close') }}</ElButton>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { ElButton } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { api, normalizeError } from '@/api'
import { useToastStore } from '@/stores/toast'
import type { AppInfoVO } from '@/types/api'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const router = useRouter()
const toast = useToastStore()

const info = ref<AppInfoVO | null>(null)

onMounted(async () => {
  try {
    info.value = await api.appInfo()
  } catch (e) {
    toast.bad(normalizeError(e).message)
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
</script>

<style scoped>
.nt-about {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-4);
}

.nt-about-brand {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
}

.nt-about-logo {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: var(--nt-tile);
  height: var(--nt-tile);
  border-radius: var(--nt-radius);
  background: var(--nt-primary);
  color: var(--nt-primary-contrast);
  font-family: var(--font-display);
  font-size: var(--nt-fs-xl);
  font-weight: 700;
}

.nt-about-name {
  font-size: var(--nt-fs-lg);
  font-weight: 600;
  color: var(--nt-ink);
}

.nt-about-desc {
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-2);
}

.nt-about-rows {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-2);
}

.nt-about-row {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
}

.nt-label {
  font-size: var(--nt-fs-label);
  font-weight: 600;
  color: var(--nt-ink-2);
  flex: none;
  min-width: 4.5em;
}

.nt-about-path {
  flex: 1;
  min-width: 0;
  font-family: var(--font-mono);
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-2);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.nt-about-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--nt-sp-2);
}
</style>
