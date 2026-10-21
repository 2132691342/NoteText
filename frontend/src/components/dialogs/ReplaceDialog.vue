// 替换浮窗：复用 FindDialog 的交互，只把默认模式设成 replace。
//
// 不复制一份组件：查找与替换的交互几乎相同，复制会让两处逻辑漂移。
// 这里显式声明 emits，让 defineAsyncComponent 的类型推导不出错。
<template>
  <FindDialog mode="replace" @close="emit('close')" />
</template>

<script lang="ts" setup>
import FindDialog from './FindDialog.vue'

const emit = defineEmits<{ close: [] }>()
</script>
