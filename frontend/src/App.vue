// 根组件：主题容器 + 路由出口 + 全局提示宿主。
//
// 单窗口桌面应用只有一个路由视图（编辑器），所以这里不做布局嵌套——
// 布局由 MainLayout.vue 负责，App 只提供 data-theme 挂点与全局层。
<template>
  <ElConfigProvider :locale="epLocale">
    <RouterView />
    <ToastHost />
  </ElConfigProvider>
</template>

<script lang="ts" setup>
import { computed, inject } from 'vue'
import { ElConfigProvider } from 'element-plus'
import zhCn from 'element-plus/es/locale/lang/zh-cn'
import enUs from 'element-plus/es/locale/lang/en'
import ToastHost from '@/components/common/ToastHost.vue'

// main.ts 注入的 computed：设置页切换语言时 EP 文案跟着切。
const epLocale = computed(() => {
  const injected = inject<any>('epLocale', null)
  return injected?.value === 'en' ? enUs : zhCn
})
</script>
