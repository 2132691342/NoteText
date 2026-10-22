// 应用入口：装配状态、路由、样式并挂载根组件。
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { computed } from 'vue'
import zhCn from 'element-plus/es/locale/lang/zh-cn'
import enUs from 'element-plus/es/locale/lang/en'

import App from './App.vue'
import router from './router'
import { i18n, LOCALES, setLocale, tr } from './i18n'
import { useSettingsStore } from './stores/settings'
import { useToastStore } from './stores/toast'
import './themes.css'
import './base.css'
import './ep-theme.css'
import './content.css'
import './print.css'
import 'virtual:uno.css'

// 渲染层异常必须浮出水面：无声白屏是「文件内容全没了」这类报告的温床。
const app = createApp(App)
app.config.errorHandler = (err, _inst, info) => {
  console.error('[NoteText]', info, err)
  try {
    useToastStore().bad(tr('toast.uiError', { reason: (err as Error)?.message || tr('toast.restartHint') }))
  } catch {
    /* toast 还没装好（pinia 之前）：只落 console */
  }
}

app.use(createPinia())
app.use(router)
app.use(i18n)

// EP 的文案跟随 i18n：由根部的 ElConfigProvider 消费这个 computed。
app.provide('epLocale', computed(() => (i18n.global.locale.value === 'en' ? enUs : zhCn)))

// 主题与语言在挂载前从配置同步：晚一步会闪一帧默认主题。
const settings = useSettingsStore()
settings.hydrate().then(() => {
  const cfg = settings.config
  document.documentElement.setAttribute('data-theme', settings.resolvedTheme)
  setLocale(LOCALES.some((l) => l.key === cfg.language) ? cfg.language : 'zh-CN')
})

app.mount('#app')
