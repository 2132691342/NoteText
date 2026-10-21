// 文档预览：按扩展名分流到 xlsx / docx / pdf / 图片 / Markdown。
//
// 为什么不让 CodeMirror 直接吃二进制：二进制解成文本是一屏乱码，
// 而用户点开 .xlsx 想看的是表格，不是乱码。扩展名分流在打开时就做，
// 不让二进制进编辑器。
//
// 所有预览包都是动态 import：一个只写 Markdown 的人不该付 xlsx 的钱。
<template>
  <div class="nt-doc-preview">
    <div class="nt-doc-head">
      <span class="nt-readout">{{ fileName }}</span>
      <div class="nt-doc-spacer" />
      <ElButton size="small" @click="openExternal">{{ t('tool.openExternal') }}</ElButton>
    </div>

    <div v-if="kind === 'unsupported'" class="nt-empty">
      <p>{{ t('tool.previewUnsupported') }}</p>
      <p class="nt-empty-hint">{{ t('tool.previewUnsupportedHint') }}</p>
      <ElButton size="small" @click="openHex">{{ t('tool.hexViewer') }}</ElButton>
    </div>

    <div v-else-if="kind === 'tooLarge'" class="nt-empty">
      <p>{{ t('tool.previewTooLarge') }}</p>
      <p class="nt-empty-hint">{{ t('tool.previewTooLargeHint') }}</p>
    </div>

    <div v-else-if="kind === 'image'" class="nt-doc-image">
      <img :src="src" :alt="fileName" />
    </div>

    <div v-else-if="kind === 'markdown'" class="nt-doc-markdown">
      <div ref="host" class="nt-md-body" />
    </div>

    <div v-else ref="host" class="nt-doc-host" />
  </div>
</template>

<script lang="ts" setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton } from 'element-plus'
import { readFile } from '@tauri-apps/plugin-fs'
import { convertFileSrc } from '@tauri-apps/api/core'
import { api, normalizeError } from '@/api'
import { useToastStore } from '@/stores/toast'
import { baseName, extName } from '@/utils'

const props = defineProps<{ path: string }>()
const emit = defineEmits<{ openHex: [] }>()

const { t } = useI18n()
const toast = useToastStore()

const host = ref<HTMLElement | null>(null)
/**
 * Markdown 预览挂载的应用：卸载时要 unmount，否则切换文件会漏实例。
 * 类型是「能挂能卸」的最小形状——不引 App 的完整类型，
 * 那会把 vue 的内部类型拖进这个组件。
 */
let mdApp: { mount: (el: Element) => unknown; unmount: () => void } | null = null
const mdText = ref('')

/** 预览体积上限：再大的文档渲染会把界面卡死，不如让用户用外部程序打开。 */
const MAX_PREVIEW = 20 * 1024 * 1024

const fileName = computed(() => baseName(props.path))
const ext = computed(() => extName(props.path))

type Kind = 'xlsx' | 'docx' | 'pdf' | 'image' | 'markdown' | 'unsupported' | 'tooLarge' | 'loading'
const kind = ref<Kind>('loading')
const src = computed(() => convertFileSrc(props.path))

const IMG = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'ico'])

async function classify() {
  const e = ext.value.toLowerCase()
  if (IMG.has(e)) {
    kind.value = 'image'
    return
  }
  if (e === 'md' || e === 'markdown') {
    await renderMarkdown()
    return
  }
  if (e === 'xlsx' || e === 'xlsm' || e === 'csv') {
    kind.value = 'xlsx'
    await renderXlsx()
    return
  }
  if (e === 'docx') {
    kind.value = 'docx'
    await renderDocx()
    return
  }
  if (e === 'pdf') {
    kind.value = 'pdf'
    await renderPdf()
    return
  }
  kind.value = 'unsupported'
}

/**
 * Markdown 预览：挂 mavon-editor 的组件（只读预览态）。
 *
 * 用它自带的渲染器而不是新引 marked：依赖清单里已经有一个 Markdown 实现，
 * 再加一个是重复付费。mavon 是 UMD 包、导出名 MavonEditor，
 * 所以走动态 import + createApp 挂载，不写进模板——
 * 那样只要打开 .txt 的用户也会加载 Markdown 运行时。
 */
async function renderMarkdown() {
  try {
    if (!host.value) return
    const data = await readFile(props.path)
    mdText.value = new TextDecoder('utf-8').decode(data)
    const mod = (await import('mavon-editor')) as unknown as {
      default?: { mavonEditor?: unknown }
      mavonEditor?: unknown
    }
    const comp = (mod.mavonEditor ?? (mod.default as { mavonEditor?: unknown })?.mavonEditor) as
      | unknown
      | undefined
    if (!comp) {
      // 拿不到组件就退回纯文本：总比一片空白好。
      kind.value = 'unsupported'
      return
    }
    const { createApp, h } = await import('vue')
    mdApp?.unmount()
    mdApp = createApp({
      render: () =>
        h(comp as never, {
          modelValue: mdText.value,
          defaultOpen: 'preview',
          editable: false,
          toolbarsFlag: false,
         subfield: false,
          // 预览区高度交给容器：写死会把弹窗撑破。
          boxShadow: false,
        }),
    })
    mdApp.mount(host.value)
    kind.value = 'markdown'
  } catch (e) {
    toast.bad(normalizeError(e).message)
    kind.value = 'unsupported'
  }
}

async function renderXlsx() {
  if (!host.value) return
  const data = await readFile(props.path)
  if (data.length > MAX_PREVIEW) {
    kind.value = 'tooLarge'
    return
  }
  const XLSX = await import('xlsx')
  const wb = XLSX.read(data, { type: 'array' })
  // 只渲染第一张表：预览定位是「看一眼里面是什么」，多表切换属于编辑器功能。
  const html = XLSX.utils.sheet_to_html(wb.Sheets[wb.SheetNames[0]])
  host.value.innerHTML = html
}

async function renderDocx() {
  if (!host.value) return
  const data = await readFile(props.path)
  if (data.length > MAX_PREVIEW) {
    kind.value = 'tooLarge'
    return
  }
  const { renderAsync } = await import('docx-preview')
  await renderAsync(data, host.value)
}

async function renderPdf() {
  if (!host.value) return
  const data = await readFile(props.path)
  if (data.length > MAX_PREVIEW) {
    kind.value = 'tooLarge'
    return
  }
  const mod = await import('vue-pdf-embed')
  const { createApp, h } = await import('vue')
  // pdf 组件是 Vue 组件：动态建一个 app 挂进来，避免把它写进模板
  // （那会让只要打开 txt 的用户也加载 pdf 运行时）。
  const app = createApp({
    render: () =>
      h(mod.default, {
        source: new Blob([data], { type: 'application/pdf' }),
      }),
  })
  app.mount(host.value)
  pdfApp = app
}

let pdfApp: { unmount: () => void } | null = null

function openExternal() {
  void api.openPath(props.path)
}

function openHex() {
  emit('openHex')
}

onMounted(classify)
onBeforeUnmount(() => {
  pdfApp?.unmount()
  pdfApp = null
  mdApp?.unmount()
  mdApp = null
})

watch(() => props.path, () => {
  // 切换文件前先卸掉上一个挂载：Markdown 与 PDF 都是自己 mount 的独立 app，
  // 不卸会越积越多，而且旧内容会盖在新容器上。
  pdfApp?.unmount()
  pdfApp = null
  mdApp?.unmount()
  mdApp = null
  void classify()
})
</script>

<style scoped>
.nt-doc-preview {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  gap: var(--nt-sp-2);
}

.nt-doc-head {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
  flex: none;
  padding: var(--nt-sp-2) var(--nt-sp-3);
}

.nt-doc-spacer {
  flex: 1;
}

.nt-doc-host {
  flex: 1;
  min-height: 0;
  overflow: auto;
  background: var(--nt-surface);
  padding: var(--nt-sp-4);
}

.nt-doc-image {
  flex: 1;
  min-height: 0;
  overflow: auto;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--nt-elev-1);
}

.nt-doc-image img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
}

.nt-doc-markdown {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: var(--nt-sp-4);
}

/* Markdown 预览走 content.css 的排版：与编辑器里的渲染保持一致 */
.nt-md-body {
  max-width: 72ch;
  line-height: 1.7;
}
</style>
