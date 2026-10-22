// 设置面板：外观（主题 / 字号 / 界面字体）。
//
// 主题三档（浅 / 深 / 跟随系统）与字号缩放都只改一个 CSS 变量，
// 全站刻度等比跟随——不需要为每种分辨率调一套数值。
<template>
  <div class="nt-set">
    <section class="nt-set-section">
      <h2 class="nt-set-title">{{ t('settings.appearance') }}</h2>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.theme') }}</span>
        <ElRadioGroup v-model="theme" size="small">
          <ElRadioButton value="light">{{ t('settings.themeLight') }}</ElRadioButton>
          <ElRadioButton value="dark">{{ t('settings.themeDark') }}</ElRadioButton>
          <ElRadioButton value="system">{{ t('settings.themeSystem') }}</ElRadioButton>
        </ElRadioGroup>
      </div>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.fontSize') }}</span>
        <ElSlider v-model="zoom" :min="80" :max="140" :step="5" class="nt-set-slider" />
        <span class="nt-readout nt-set-value">{{ zoom }}%</span>
      </div>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.fontFamily') }}</span>
        <ElSelect v-model="fontFamily" size="small" class="nt-set-ctl">
          <ElOption v-for="f in FONTS" :key="f.id" :label="f.label" :value="f.id" />
        </ElSelect>
      </div>
    </section>
  </div>
</template>

<script lang="ts" setup>
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElOption, ElRadioButton, ElRadioGroup, ElSelect, ElSlider } from 'element-plus'
import { useSettingsStore } from '@/stores/settings'

const { t } = useI18n()
const settings = useSettingsStore()

/** 界面字体三档：只改 --nt-font-ui，正文与标题的字族不变。 */
const FONTS = computed(() => [
  { id: 'sans', label: t('settings.fontSans') },
  { id: 'yahei', label: t('settings.fontYahei') },
  { id: 'mono', label: t('settings.fontMono') },
])

const theme = computed({
  get: () => settings.config.theme,
  set: (v) => settings.update({ theme: v }),
})

const zoom = computed({
  get: () => settings.config.ui.zoom_level,
  set: (v) => settings.updateUI({ zoom_level: v }),
})

const fontFamily = computed({
  get: () => {
    const f = getComputedStyle(document.documentElement).getPropertyValue('--nt-font-ui')
    return f.includes('YaHei') ? 'yahei' : f.includes('Mono') ? 'mono' : 'sans'
  },
  set: (v) => {
    const map: Record<string, string> = {
      sans: "var(--font-sans)",
      yahei: "'Microsoft YaHei UI', 'Microsoft YaHei', sans-serif",
      mono: "var(--font-mono)",
    }
    document.documentElement.style.setProperty('--nt-font-ui', map[v] ?? map.sans)
  },
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

.nt-set-slider {
  flex: 1;
  max-width: 240px;
}

.nt-set-value {
  min-width: 3.5em;
  text-align: right;
}

.nt-set-ctl {
  width: 180px;
}
</style>
