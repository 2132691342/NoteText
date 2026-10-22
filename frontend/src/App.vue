// 根组件：主题容器 + 路由出口 + 全局提示宿主。
//
// 单窗口桌面应用只有一个路由视图（编辑器），所以这里不做布局嵌套——
// 布局由 MainLayout.vue 负责，App 只提供 data-theme 挂点与全局层。
<template>
  <ElConfigProvider :locale="epLocale">
    <!-- 标题栏挂在根上：设置页 / 帮助页也要有最小化与关闭，
         否则用户进设置后没法用鼠标关窗。 -->
    <div class="nt-shell">
      <TitleBar />
      <RouterView />
    </div>
    <ToastHost />
    <!-- 未保存确认：挂根节点，标签关闭与退出共用同一个队列 -->
    <UnsavedDialog />
  </ElConfigProvider>
</template>

<script lang="ts" setup>
import { computed, inject } from 'vue'
import { ElConfigProvider } from 'element-plus'
import zhCn from 'element-plus/es/locale/lang/zh-cn'
import enUs from 'element-plus/es/locale/lang/en'
import ToastHost from '@/components/common/ToastHost.vue'
import TitleBar from '@/components/layout/TitleBar.vue'
import UnsavedDialog from '@/components/dialogs/UnsavedDialog.vue'

// main.ts 注入的 computed：设置页切换语言时 EP 文案跟着切。
const epLocale = computed(() => {
  const injected = inject<any>('epLocale', null)
  return injected?.value === 'en' ? enUs : zhCn
})
</script>

<style scoped>
/* 外壳：标题栏固定在上，路由视图吃满剩余高度 */
.nt-shell {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.nt-shell > :nth-child(2) {
  flex: 1;
  min-height: 0;
}
</style>
