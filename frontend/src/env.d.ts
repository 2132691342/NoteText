// / <reference types="vite/client" />
// / <reference types="unplugin-vue-components/vue" />

declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<{}, {}, any>
  export default component
}

/** Tauri 2 的 invoke / event API（不引 @tauri-apps/api 的全量类型，只包一层） */
interface Window {
  /** 开发期由 Tauri 注入：存在即说明跑在 WebView 里而不是浏览器 */
  __TAURI_INTERNALS__?: unknown
}
