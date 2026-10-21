// 设置面板：通用（语言 / 会话 / 托盘 / 最近文件）。
//
// 每项改动即时生效 + 防抖回写（settings store 的 scheduleSave）。
<template>
  <div class="nt-set">
    <section class="nt-set-section">
      <h2 class="nt-set-title">{{ t('settings.general') }}</h2>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.language') }}</span>
        <ElSelect v-model="lang" size="small" class="nt-set-ctl">
          <ElOption v-for="l in LOCALES" :key="l.key" :label="l.name" :value="l.key" />
        </ElSelect>
      </div>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.restoreSession') }}</span>
        <ElSwitch v-model="restoreSession" />
      </div>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.closeToTray') }}</span>
        <ElSwitch v-model="closeToTray" />
      </div>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.recentLimit') }}</span>
        <ElInputNumber v-model="recentLimit" size="small" :min="0" :max="100" class="nt-set-ctl" />
      </div>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.rememberLastFolder') }}</span>
        <ElSwitch v-model="rememberFolder" />
      </div>
    </section>
  </div>
</template>

<script lang="ts" setup>
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElInputNumber, ElOption, ElSelect, ElSwitch } from 'element-plus'
import { LOCALES } from '@/i18n'
import { useSettingsStore } from '@/stores/settings'

const { t } = useI18n()
const settings = useSettingsStore()

const lang = computed({
  get: () => settings.config.language,
  set: (v) => settings.update({ language: v }),
})
const restoreSession = computed({
  get: () => settings.config.ui.restore_session,
  set: (v) => settings.updateUI({ restore_session: v }),
})
const closeToTray = computed({
  get: () => settings.config.ui.close_to_tray,
  set: (v) => settings.updateUI({ close_to_tray: v }),
})
const recentLimit = computed({
  get: () => settings.config.ui.recent_limit,
  set: (v) => settings.updateUI({ recent_limit: v }),
})
const rememberFolder = computed({
  get: () => settings.config.file.remember_last_folder,
  set: (v) => settings.updateFile({ remember_last_folder: v }),
})
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
  gap: var(--nt-sp-4);
}

.nt-set-label {
  flex: none;
  min-width: 220px;
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-2);
}

.nt-set-ctl {
  width: 180px;
}
</style>
