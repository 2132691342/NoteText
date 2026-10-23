// 路由表：hash 模式，单窗口桌面应用。
// 为什么 hash 而不是 history：Tauri 的 asset 协议下没有服务端做重写，
import { createRouter, createWebHashHistory } from 'vue-router'

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    // 编辑器主界面：唯一的常驻视图。
    { path: '/', name: 'editor', component: () => import('@/views/EditorView.vue') },
    // 设置页：用 /settings/:tab 而不是子路由，刷新与前进后退都能停在原处。
    { path: '/settings/:tab?', name: 'settings', component: () => import('@/views/SettingsView.vue') },
    // 帮助页：内置手册（assets/docs 的渲染入口）。
    { path: '/help/:name?', name: 'help', component: () => import('@/views/HelpView.vue') },
    { path: '/:pathMatch(.*)*', redirect: '/' },
  ],
})

export default router
