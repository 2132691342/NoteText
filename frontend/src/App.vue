// 根组件：主题容器 + 路由出口 + 全局提示宿主。
// 单窗口桌面应用只有一个路由视图（编辑器），所以这里不做布局嵌套——
<template>
  <ElConfigProvider :locale="locale">
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
import { inject } from 'vue'
import type { ComputedRef } from 'vue'
import { ElConfigProvider } from 'element-plus'
import type { Language } from 'element-plus/es/locale'
import ToastHost from '@/components/common/ToastHost.vue'
import TitleBar from '@/components/layout/TitleBar.vue'
import UnsavedDialog from '@/components/dialogs/UnsavedDialog.vue'
import { useQuitGuard } from '@/composables/useQuitGuard'
import { useGlobalKeys } from '@/composables/useGlobalKeys'

// inject 必须在 setup 顶层同步调用；放进 computed getter 会每次求值都告警。
// main.ts 注入的就是 computed，直接交给 EP 即可。
const locale = inject<ComputedRef<Language>>('epLocale')

// 退出守卫与全局键必须挂在根上：设置页 / 帮助页是独立路由，挂在编辑器视图里
// 会随路由卸载——设置页点 X 就没人应答，等于「点了关闭没反应」。
useQuitGuard()
useGlobalKeys()
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
