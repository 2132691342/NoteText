// 设置面板：文件与编码（默认编码 / 行尾 / 保存行为 / 忽略规则）。
//
// 这些是「怎么写盘」的全局默认：单个文件可以在状态栏临时改，
// 新建文件按这里的默认来。
<template>
  <div class="nt-set">
    <section class="nt-set-section">
      <h2 class="nt-set-title">{{ t('settings.file') }}</h2>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.defaultEncoding') }}</span>
        <ElSelect v-model="defaultEncoding" size="small" filterable class="nt-set-ctl">
          <ElOption label="自动探测" value="auto" />
          <ElOption v-for="e in encodings" :key="e.id" :label="e.label" :value="e.id" />
        </ElSelect>
        <ElCheckbox v-model="autoDetect">{{ t('settings.autoDetectEncoding') }}</ElCheckbox>
      </div>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.newLineEnding') }}</span>
        <ElRadioGroup v-model="newLineEnding" size="small">
          <ElRadioButton value="crlf">CRLF（Windows）</ElRadioButton>
          <ElRadioButton value="lf">LF</ElRadioButton>
          <ElRadioButton value="system">跟随系统</ElRadioButton>
        </ElRadioGroup>
      </div>

      <div class="nt-set-grid">
        <ElCheckbox v-model="ensureFinalNewline">{{ t('settings.ensureFinalNewline') }}</ElCheckbox>
        <ElCheckbox v-model="trimTrailingWs">{{ t('settings.trimTrailingWs') }}</ElCheckbox>
      </div>

      <div class="nt-set-row nt-set-row-top">
        <span class="nt-set-label">{{ t('settings.ignorePatterns') }}</span>
        <div class="nt-set-patterns">
          <span v-for="(p, i) in patterns" :key="i" class="nt-chip">
            {{ p }}
            <button class="nt-chip-x" @click="removePattern(i)">×</button>
          </span>
          <ElInput
            v-model="newPattern"
            size="small"
            placeholder="*.log"
            class="nt-set-ctl"
            @keydown.enter="addPattern"
          />
        </div>
      </div>
    </section>
  </div>
</template>

<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  ElCheckbox,
  ElInput,
  ElOption,
  ElRadioButton,
  ElRadioGroup,
  ElSelect,
} from 'element-plus'
import { api } from '@/api'
import { useSettingsStore } from '@/stores/settings'
import type { EncodingInfo } from '@/types/api'

const { t } = useI18n()
const settings = useSettingsStore()

const encodings = ref<EncodingInfo[]>([])
const newPattern = ref('')

onMounted(async () => {
  encodings.value = await api.encodings()
})

const defaultEncoding = bind('default_encoding')
const autoDetect = bind('auto_detect_encoding')
const newLineEnding = bind('new_line_ending')
const ensureFinalNewline = bind('ensure_final_newline')
const trimTrailingWs = bind('trim_trailing_ws')
const patterns = bind('ignore_patterns')

function bind<K extends keyof (typeof settings.config.file)>(key: K) {
  return computed({
    get: () => settings.config.file[key],
    set: (v) => settings.updateFile({ [key]: v } as never),
  })
}

function addPattern() {
  const p = newPattern.value.trim()
  if (!p || patterns.value.includes(p)) return
  settings.updateFile({ ignore_patterns: [...patterns.value, p] })
  newPattern.value = ''
}

function removePattern(i: number) {
  settings.updateFile({ ignore_patterns: patterns.value.filter((_, k) => k !== i) })
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
  gap: var(--nt-sp-4);
}

.nt-set-row-top {
  align-items: flex-start;
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

.nt-set-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--nt-sp-2) var(--nt-sp-4);
}

.nt-set-patterns {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--nt-sp-2);
  flex: 1;
  min-width: 0;
}

.nt-chip {
  display: inline-flex;
  align-items: center;
  gap: var(--nt-sp-1);
  padding: 0 var(--nt-sp-2);
  height: var(--nt-ctl-h-sm);
  border-radius: var(--nt-radius-sm);
  background: var(--nt-surface-2);
  border: 1px solid var(--nt-border);
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-2);
}

.nt-chip-x {
  color: var(--nt-ink-muted);
  font-size: 14px;
  line-height: 1;
}

.nt-chip-x:hover {
  color: var(--nt-danger);
}
</style>
