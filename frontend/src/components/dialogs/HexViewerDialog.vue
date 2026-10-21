// Hex 查看浮窗：包一层 NtModal，主体复用 HexView。
//
// 分开是为了让 HexView 能被编辑区内嵌复用（view_type='hex'）；
// 内嵌时不需要遮罩与焦点圈，那只会挡住整个界面。
<template>
  <NtModal :title="t('tool.hexViewer')" width="880" @close="emit('close')">
    <HexView :path="path" />
  </NtModal>
</template>

<script lang="ts" setup>
import { useI18n } from 'vue-i18n'
import NtModal from '@/components/common/NtModal.vue'
import HexView from '@/components/editor/HexView.vue'
import { useDialogStore } from '@/stores/dialogs'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const dialogs = useDialogStore()

// 路径从 dialog store 的 seedExtra 取：与其余浮窗保持同一签名（只有 title + close），
// 否则 DialogHost 的异步组件联合类型会因多一个必填 prop 而报错。
const path = dialogs.seedExtra.path as string
</script>
