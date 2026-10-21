import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import UnoCSS from 'unocss/vite'
import Components from 'unplugin-vue-components/vite'
import { ElementPlusResolver } from 'unplugin-vue-components/resolvers'

/**
 * Vite 配置：Tauri 2 嵌入式前端。
 *
 * 两个硬约束：
 * 1. `build.outDir` 必须是 `dist`，与 src-tauri/tauri.conf.json 的 frontendDist 对应；
 * 2. `clearScreen`/`envPrefix` 让 `npm run dev` 在浏览器里也能跑（无后端时显示空态），
 *    连真后端用 `npm run tauri dev`。
 */
export default defineConfig({
  plugins: [
    vue(),
    UnoCSS(),
    // EP 按需引入：全量注册会把用不到的组件连同样式打进首屏。
    // 解析器按模板里真实出现的标签按需注册。
    Components({
      dts: 'src/components.d.ts',
      resolvers: [ElementPlusResolver()],
    }),
  ],
  resolve: {
    alias: [
      { find: '@', replacement: fileURLToPath(new URL('./src', import.meta.url)) },
    ],
  },
  // Tauri 期望一个固定端口；strictPort 让端口被占时直接报错而不是悄悄换端口
  // （换端口后 Tauri 的 devUrl 就对不上了）。
  server: {
    port: 5173,
    strictPort: true,
    host: '127.0.0.1',
    hmr: {
      protocol: 'ws',
      host: '127.0.0.1',
    },
    watch: {
      // 不监听 Rust 侧产物：target/ 一变就全量重载，开发时很吵。
      ignored: ['**/src-tauri/target/**', '**/dist/**'],
    },
  },
  // 防止 Vite 把运行时才有的 Tauri 常量当成构建期常量内联。
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    sourcemap: false,
    target: 'es2022',
    // WebView2 支持 es2022；不引 legacy 转换（省体积，也避免 polyfill 掩盖真实不兼容）
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes('node_modules')) return undefined
          if (id.includes('codemirror') || id.includes('@codemirror') || id.includes('@lezer')) {
            return 'vendor-cm'
          }
          if (id.includes('echarts') || id.includes('zrender')) return 'vendor-echarts'
          if (id.includes('xlsx') || id.includes('docx-preview') || id.includes('pdf')) {
            return 'vendor-doc'
          }
          if (id.includes('@wangeditor')) return 'vendor-wang'
          // EP 不归堆：按需导入的组件归进单个 chunk 会把内部依赖一起拉回首屏，
          // 让 tree-shaking 与各视图的懒加载同时失效。
          if (id.includes('element-plus') || id.includes('@element-plus')) return undefined
          if (id.includes('vue') || id.includes('pinia') || id.includes('vue-router') || id.includes('vue-i18n')) {
            return 'vendor'
          }
          return undefined
        },
      },
    },
  },
})
