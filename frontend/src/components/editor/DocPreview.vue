// 文档预览：按扩展名分流到 xlsx / docx / pdf / 图片 / Markdown / 纯文本。
//
// 二进制不进 CodeMirror：解成文本是一屏乱码，而用户点开 .xlsx 想看的是表格。
// 所有预览包都是动态 import——只写 Markdown 的人不该付 xlsx 的钱。
<template>
  <div class="nt-doc-preview">
    <div class="nt-doc-head">
      <span class="nt-readout">{{ fileName }}</span>
      <div class="nt-doc-spacer" />
      <ElButton size="small" @click="emit('toggleEdit')">
        <NtIcon name="braces" size="sm" />
        <span class="nt-btn-text">{{ t('tool.backToEdit') }}</span>
      </ElButton>
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

    <div v-else ref="host" class="nt-doc-host" />
  </div>
</template>

<script lang="ts" setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton } from 'element-plus'
import { convertFileSrc } from '@tauri-apps/api/core'
import { api, normalizeError } from '@/api'
import { useToastStore } from '@/stores/toast'
import NtIcon from '@/components/common/NtIcon.vue'
import { baseName, extName } from '@/utils'

const props = defineProps<{ path: string }>()
const emit = defineEmits<{ openHex: []; toggleEdit: [] }>()

const { t } = useI18n()
const toast = useToastStore()

const host = ref<HTMLElement | null>(null)
const fileName = computed(() => baseName(props.path))
const ext = computed(() => extName(props.path))

type Kind =
  | 'xlsx'
  | 'docx'
  | 'pdf'
  | 'image'
  | 'plain'
  | 'unsupported'
  | 'tooLarge'
  | 'loading'
const kind = ref<Kind>('loading')
const src = computed(() => convertFileSrc(props.path))

const IMG = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'ico'])

async function classify() {
  const e = ext.value.toLowerCase()
  if (IMG.has(e)) {
    kind.value = 'image'
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
  // 其余一律按纯文本预览：用户主动按了「预览」，就该看到只读全文。
  await renderPlainText()
}

async function renderPlainText() {
  if (!host.value) return
  const data = await readBytes()
  if (!data) return
  // 二进制不进纯文本预览：解成文本是一屏 NUL 与乱码，改用十六进制。
  if (looksBinary(data)) {
    kind.value = 'unsupported'
    return
  }
  // 纯文本预览：不解析，原样展示。之前按「预览」对 .txt 什么也不做
  // （命令被错接到了十六进制视图）。
  host.value.textContent = new TextDecoder('utf-8').decode(data)
  host.value.classList.add('is-plain')
  kind.value = 'plain'
}

/**
 * 读文件字节：走后端 `read_binary`（base64），不走 WebView 的 fs 插件。
 *
 * 为什么不用 plugin-fs：那等于把整块磁盘的读权限交给渲染层，而预览
 * 只需要「读这一个文件」。后端命令还带了 20MB 上限，超限直接给可读提示。
 * 返回 null = 已经给出提示，调用方直接返回即可。
 */
async function readBytes(): Promise<Uint8Array | null> {
  try {
    const b64 = await api.readBinary(props.path)
    return base64ToBytes(b64)
  } catch (e) {
    const err = normalizeError(e)
    toast.warn(err.message)
    kind.value = 'unsupported'
    return null
  }
}

/** base64 → 字节。atob 只处理 Latin-1，所以逐字节取 charCode。 */
function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i += 1) out[i] = bin.charCodeAt(i)
  return out
}

/** 前 8KB 出现 NUL 就当二进制：足够区分文本与可执行文件 / 压缩包。 */
function looksBinary(data: Uint8Array): boolean {
  const n = Math.min(data.length, 8192)
  for (let i = 0; i < n; i += 1) {
    if (data[i] === 0) return true
  }
  return false
}

async function renderXlsx() {
  if (!host.value) return
  const data = await readBytes()
  if (!data) return
  const XLSX = await import('xlsx')
  const wb = XLSX.read(data, { type: 'array' })
  // 只渲染第一张表：预览定位是「看一眼里面是什么」，多表切换属于编辑器功能。
  const html = XLSX.utils.sheet_to_html(wb.Sheets[wb.SheetNames[0]])
  host.value.innerHTML = html
}

async function renderDocx() {
  if (!host.value) return
  const data = await readBytes()
  if (!data) return
  const { renderAsync } = await import('docx-preview')
  await renderAsync(data, host.value)
}

async function renderPdf() {
  if (!host.value) return
  const data = await readBytes()
  if (!data) return
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
})

watch(() => props.path, () => {
  // 切换文件前先卸掉上一个挂载：Markdown 与 PDF 都是自己 mount 的独立 app，
  // 不卸会越积越多，而且旧内容会盖在新容器上。
  pdfApp?.unmount()
  pdfApp = null
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

/* 纯文本预览：只读全文，保留换行与等宽对齐 */
.nt-doc-host.is-plain {
  font-family: var(--nt-font-editor);
  font-size: var(--nt-fs-md);
  line-height: var(--nt-lh-base);
  color: var(--nt-ink);
  white-space: pre-wrap;
  overflow-wrap: break-word;
  tab-size: 4;
  user-select: text;
  cursor: text;
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
