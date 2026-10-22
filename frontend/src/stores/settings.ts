// 设置 store：配置的权威态 + 主题 / 语言 / 字号缩放的落地。
//
// 归属（AGENTS.md §2.10）：配置的唯一真相源是后端 SQLite `settings` 表，
// 这里只是它的镜像 + 派生量（resolvedTheme、缩放变量）。任何改动都

import { defineStore } from 'pinia'
import { computed, ref, watch } from 'vue'
import { api, normalizeError } from '@/api'
import { setLocale } from '@/i18n'
import type { AppConfig } from '@/types/api'
import { useToastStore } from './toast'

/** 与后端 `AppConfig::default()` 逐字段对应（Rust 侧改默认值必须同步这里）。 */
function defaultConfig(): AppConfig {
  return {
    theme: 'system',
    language: 'zh-CN',
    editor: {
      font_size: 13,
      font_family: "Consolas, 'Courier New', monospace",
      editor_theme: 'auto',
      tab_size: 4,
      insert_spaces: true,
      word_wrap: false,
      line_numbers: true,
      highlight_active_line: true,
      bracket_matching: true,
      bracket_pair_colors: true,
      indent_guides: true,
      show_whitespace: false,
      show_eol: false,
      fold_gutter: true,
      minimap: true,
      auto_save: false,
      auto_save_interval: 60,
      highlight_selection: true,
    },
    file: {
      default_encoding: 'auto',
      auto_detect_encoding: true,
      new_line_ending: 'system',
      ensure_final_newline: false,
      trim_trailing_ws: false,
      ignore_patterns: ['node_modules', '.git', 'target', 'dist'],
      remember_last_folder: true,
      last_folder: '',
    },
    ui: {
      show_menu_bar: true,
      show_toolbar: true,
      show_status_bar: true,
      show_sidebar: true,
      sidebar_width: 260,
      toolbar_icon_size: 24,
      zoom_level: 100,
      close_to_tray: false,
      restore_session: true,
      recent_limit: 20,
      status_bar_items: {},
      sidebar_tab: 'files',
    },
    keybindings: {},
  }
}

/** 系统主题：跟随 `prefers-color-scheme`。 */
function systemTheme(): 'light' | 'dark' {
  return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

export const useSettingsStore = defineStore('settings', () => {
  const config = ref<AppConfig>(defaultConfig())
  const loaded = ref(false)
  const systemDark = ref(systemTheme())

  // 派生：实际生效的主题。`system` 时跟随系统，并在系统切换时实时更新。
  const resolvedTheme = computed<'light' | 'dark'>(() =>
    config.value.theme === 'system' ? systemDark.value : config.value.theme,
  )

  /** 启动时从后端拉配置。失败保持默认值（不让应用起不来）。 */
  async function hydrate() {
    try {
      config.value = await api.loadConfig()
    } catch (e) {
      // 读不到配置（首次启动 / 浏览器预览）：用默认值，且不当错误弹窗——
      // 用户看到的是「设置是默认的」，而不是「应用坏了」。
      const err = normalizeError(e)
      if (err.code !== 'no-backend') {
        useToastStore().warn(`读取设置失败，已用默认值：${err.message}`)
      }
    }
    loaded.value = true
    applyTheme()
    applyZoom()
  }

  /** 改一项配置：立即生效 + 防抖回写。 */
  const saveTimer = ref<number | null>(null)
  function update(patch: Partial<AppConfig>) {
    config.value = { ...config.value, ...patch }
    applyTheme()
    applyZoom()
    scheduleSave()
  }

  /** 改编辑器配置的一项。 */
  function updateEditor(patch: Partial<AppConfig['editor']>) {
    config.value.editor = { ...config.value.editor, ...patch }
    scheduleSave()
  }

  /** 改文件配置的一项。 */
  function updateFile(patch: Partial<AppConfig['file']>) {
    config.value.file = { ...config.value.file, ...patch }
    scheduleSave()
  }

  /** 改 UI 配置的一项。 */
  function updateUI(patch: Partial<AppConfig['ui']>) {
    config.value.ui = { ...config.value.ui, ...patch }
    scheduleSave()
  }

  /** 防抖 400ms 回写：拖动滑块时不会每像素都过一次 IPC。 */
  function scheduleSave() {
    if (saveTimer.value !== null) window.clearTimeout(saveTimer.value)
    saveTimer.value = window.setTimeout(() => {
      saveTimer.value = null
      void save()
    }, 400)
  }

  async function save() {
    try {
      await api.saveConfig(config.value)
    } catch (e) {
      useToastStore().bad(`设置保存失败：${normalizeError(e).message}`)
    }
  }

  /** 恢复默认：只重置本 store 管的部分（快捷键映射由用户自管，不动）。 */
  async function reset() {
    const keybindings = config.value.keybindings
    config.value = { ...defaultConfig(), keybindings }
    applyTheme()
    applyZoom()
    await save()
  }

  // ---- 主题 / 缩放落地 ----

  function applyTheme() {
    // 切换主题时给一次短过渡：只加在 <html> 上，避免每个元素各自跑动画。
    // 一次性：过渡结束后立即摘掉，否则窗口缩放、面板拖拽都会被它拖慢。
    const root = document.documentElement
    root.classList.add('nt-theme-switching')
    root.setAttribute('data-theme', resolvedTheme.value)
    window.setTimeout(() => root.classList.remove('nt-theme-switching'), 220)
  }

  function applyZoom() {
    // 界面字号缩放：只改一个 CSS 变量，全站字号刻度等比跟随。
    const root = document.documentElement
    root.style.setProperty('--nt-fs-scale', String(config.value.ui.zoom_level / 100))
  }

  // 系统主题切换：`system` 模式下实时跟随。
  if (window.matchMedia) {
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    mq.addEventListener?.('change', (e) => {
      systemDark.value = e.matches ? 'dark' : 'light'
      if (config.value.theme === 'system') applyTheme()
    })
  }

  // 语言切换落到 i18n：放这里而不是各处调 setLocale，
  // 保证「设置改语言」与「界面文案」只有一个同步点。
  watch(
    () => config.value.language,
    (lang) => setLocale(lang),
  )

  return {
    config,
    loaded,
    resolvedTheme,
    hydrate,
    update,
    updateEditor,
    updateFile,
    updateUI,
    save,
    reset,
  }
})
