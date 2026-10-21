// i18n 装配：中文默认 + 英文；语言偏好走后端配置表的 language 键。
import { createI18n } from 'vue-i18n'
import zhCN from './locales/zh-CN'
import en from './locales/en'

export type Locale = 'zh-CN' | 'en'

export const i18n = createI18n({
  legacy: false,
  locale: 'zh-CN',
  fallbackLocale: 'zh-CN',
  // 缺 key 时回落中文并打警告：文案漏翻译不该静默显示 key 本身。
  missingWarn: import.meta.env.DEV,
  fallbackWarn: import.meta.env.DEV,
  messages: { 'zh-CN': zhCN, en },
})

/** 语言清单：设置页语言切换的取值表。 */
export const LOCALES: Array<{ key: Locale; name: string }> = [
  { key: 'zh-CN', name: '中文' },
  { key: 'en', name: 'English' },
]

export function setLocale(locale: Locale) {
  i18n.global.locale.value = locale
  document.documentElement.setAttribute('lang', locale)
}
