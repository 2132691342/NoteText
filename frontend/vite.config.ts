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
    // 这里**故意不写 manualChunks**。曾经有一份按包名归类的分块表
    // （xlsx / docx-preview / pdfjs -> vendor-doc，vue / pinia / i18n -> vendor），
    // 注释还写着「必须这样分才能避免白屏」。事实相反：那正是白屏的原因。
    //
    // 机制：这些包内嵌 CJS/UMD 产物，Vite 转 ESM 时会生成 `require$$0` 包装，
    // 并要求一个 Vue 实例（vue-pdf-embed 之类）。手工把「被 vue 依赖」和
    // 「依赖 vue」的模块劈成两个 chunk，就得到 vendor <-> vendor-doc 的双向环；
    // ESM 循环里顶层求值命中 TDZ —— `Cannot access 'require$$0' before initialization`，
    // 入口模块整个作废、`#app` 永不挂载，用户看到一个纯白窗口。
    //
    // 分包交给 Rollup 默认策略：它只在动态 import 的真实边界上切，环不可能出现。
    // 重库（xlsx / docx-preview / vue-pdf-embed / mermaid）本来就写在
    // DocPreview.vue、useMarkdown.ts 的动态 import 里，按需自然落到异步 chunk，
    // 既省了首屏 preload，也不需要任何人再维护一张会过期的包名清单。
  },
})
