// 设置面板：编辑器（字号 / 字体 / 制表符 / 各类开关）。
// 每一项都对应 CodeMirror 的一个 compartment（见 CodeEditor.vue 的 watch），
<template>
  <div class="nt-set">
    <section class="nt-set-section">
      <h2 class="nt-set-title">{{ t('settings.editor') }}</h2>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.editorFontSize') }}</span>
        <ElInputNumber v-model="fontSize" size="small" :min="9" :max="32" class="nt-set-ctl" />
      </div>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.editorFontFamily') }}</span>
        <ElInput v-model="fontFamily" size="small" class="nt-set-ctl" />
      </div>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.tabSize') }}</span>
        <ElRadioGroup v-model="tabSize" size="small">
          <ElRadioButton :value="2">2</ElRadioButton>
          <ElRadioButton :value="4">4</ElRadioButton>
          <ElRadioButton :value="8">8</ElRadioButton>
        </ElRadioGroup>
        <ElCheckbox v-model="insertSpaces">{{ t('settings.insertSpaces') }}</ElCheckbox>
      </div>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.wordWrap') }}</span>
        <ElSwitch v-model="wordWrap" />
      </div>

      <div class="nt-set-grid">
        <ElCheckbox v-model="lineNumbers">{{ t('settings.lineNumbers') }}</ElCheckbox>
        <ElCheckbox v-model="highlightActiveLine">{{ t('settings.highlightActiveLine') }}</ElCheckbox>
        <ElCheckbox v-model="bracketMatching">{{ t('settings.bracketMatching') }}</ElCheckbox>
        <ElCheckbox v-model="bracketPairColors">{{ t('settings.bracketPairColors') }}</ElCheckbox>
        <ElCheckbox v-model="indentGuides">{{ t('settings.indentGuides') }}</ElCheckbox>
        <ElCheckbox v-model="showWhitespace">{{ t('settings.showWhitespace') }}</ElCheckbox>
        <ElCheckbox v-model="showEol">{{ t('settings.showEol') }}</ElCheckbox>
        <ElCheckbox v-model="foldGutter">{{ t('settings.foldGutter') }}</ElCheckbox>
        <ElCheckbox v-model="minimap">{{ t('settings.minimap') }}</ElCheckbox>
        <ElCheckbox v-model="highlightSelection">{{ t('settings.highlightSelection') }}</ElCheckbox>
      </div>

      <div class="nt-set-row">
        <span class="nt-set-label">{{ t('settings.autoSave') }}</span>
        <ElSwitch v-model="autoSave" />
        <ElInputNumber
          v-if="autoSave"
          v-model="autoSaveInterval"
          size="small"
          :min="5"
          :max="600"
          :step="5"
        />
        <span v-if="autoSave" class="nt-readout">{{ t('settings.seconds') }}</span>
      </div>
    </section>
  </div>
</template>

<script lang="ts" setup>
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  ElCheckbox,
  ElInput,
  ElInputNumber,
  ElRadioButton,
  ElRadioGroup,
  ElSwitch,
} from 'element-plus'
import { useSettingsStore } from '@/stores/settings'

const { t } = useI18n()
const settings = useSettingsStore()
const c = computed(() => settings.config.editor)

/** 生成一个双向绑定到 editor 配置某项的 computed。 */
function bind<K extends keyof typeof c.value>(key: K) {
  return computed({
    get: () => c.value[key],
    set: (v) => settings.updateEditor({ [key]: v } as never),
  })
}

const fontSize = bind('font_size')
const fontFamily = bind('font_family')
const tabSize = bind('tab_size')
const insertSpaces = bind('insert_spaces')
const wordWrap = bind('word_wrap')
const lineNumbers = bind('line_numbers')
const highlightActiveLine = bind('highlight_active_line')
const bracketMatching = bind('bracket_matching')
const bracketPairColors = bind('bracket_pair_colors')
const indentGuides = bind('indent_guides')
const showWhitespace = bind('show_whitespace')
const showEol = bind('show_eol')
const foldGutter = bind('fold_gutter')
const minimap = bind('minimap')
const highlightSelection = bind('highlight_selection')
const autoSave = bind('auto_save')
const autoSaveInterval = bind('auto_save_interval')
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

.nt-set-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--nt-sp-2) var(--nt-sp-4);
}
</style>
