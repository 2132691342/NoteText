import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import UnoCSS from 'unocss/vite'
import Components from 'unplugin-vue-components/vite'
import { ElementPlusResolver } from 'unplugin-vue-components/resolvers'
import ElementPlus from 'unplugin-element-plus/vite'

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
    //
    // **两个插件缺一不可**：Components 负责把 `<ElButton>` 解析成组件并把
    // 组件样式引进来；unplugin-element-plus 负责改写源码里 `import { ElButton }`
    // 这类显式导入，给它补上样式副作用导入。
    // 少了后者，凡是「脚本里显式 import 再在模板用」的组件都只有 DOM 没有样式
    // ——按钮会渲染成裸文字，浮窗里最明显。
    ElementPlus({ useSource: false }),
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
    //
    // 不写 manualChunks：重库（xlsx / docx-preview / vue-pdf-embed / mermaid）
    // 都写在动态 import 里，Rollup 默认策略只在真实边界切块，
    // 按需落到异步 chunk，既省首屏 preload 也不需要维护包名清单。
  },
})
