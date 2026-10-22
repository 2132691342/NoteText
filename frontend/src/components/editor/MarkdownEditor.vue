// Markdown 三态：编辑 / 分屏 / 预览。
//
// 与 DocPreview 的分工：DocPreview 是「点开一个文件先看看内容」，
// 这里是「要改这个 .md」。编辑态就是普通 CodeEditor（复用它的撤销历史、
// 快捷键、右键菜单），预览态走 composables/useMarkdown。
//
// 预览渲染时机：源文本变 → 重渲染 HTML → nextTick 后画图与公式。
// 三步异步，所以用 token 防止「旧文本的图覆盖新文本的图」。

<template>
  <div class="nt-md">
    <div class="nt-md-head">
      <span class="nt-readout nt-md-name" :title="path">{{ name }}</span>
      <div class="nt-md-spacer" />
      <ElRadioGroup v-model="mode" size="small" class="nt-md-modes">
        <ElRadioButton value="edit">{{ t('markdown.edit') }}</ElRadioButton>
        <ElRadioButton value="split">{{ t('markdown.split') }}</ElRadioButton>
        <ElRadioButton value="preview">{{ t('markdown.preview') }}</ElRadioButton>
      </ElRadioGroup>
      <ElButton size="small" :loading="exporting" @click="exportHtml">
        {{ t('markdown.exportHtml') }}
      </ElButton>
      <ElButton size="small" @click="emit('toggleEdit')">
        <span class="nt-btn-text">{{ t('tool.backToEdit') }}</span>
      </ElButton>
    </div>

    <div class="nt-md-body" :class="`is-${mode}`">
      <div v-show="mode !== 'preview'" class="nt-md-edit">
        <CodeEditor :tab="tab" view-id="main" />
      </div>
      <div v-show="mode !== 'edit'" ref="preview" class="nt-md-preview" />
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElRadioButton, ElRadioGroup } from 'element-plus'
import CodeEditor from '@/components/editor/CodeEditor.vue'
import { useMarkdown } from '@/composables/useMarkdown'
import { useSettingsStore } from '@/stores/settings'
import { useToastStore } from '@/stores/toast'
import { api, normalizeError } from '@/api'
import { getText } from '@/composables/useEditorRegistry'
import { baseName } from '@/utils'
import type { Tab } from '@/stores/editor'

const props = defineProps<{ path: string; tab: Tab }>()
const emit = defineEmits<{ toggleEdit: [] }>()

const { t } = useI18n()
const settings = useSettingsStore()
const toast = useToastStore()
const md = useMarkdown()

type Mode = 'edit' | 'split' | 'preview'
const mode = ref<Mode>('split')
const preview = ref<HTMLElement | null>(null)
const exporting = ref(false)

const name = computed(() => baseName(props.path))

/** 源文本：从编辑器注册表拿，不往 store 里存正文（AGENTS.md §1.2）。 */
const source = computed(() => getText(props.tab.id))

md.setDark(settings.resolvedTheme === 'dark')

/** 渲染一遍：HTML → 图 → 公式。三步异步，中途换文本会让前面的白做。 */
let renderToken = 0

async function render() {
  const token = ++renderToken
  const text = source.value
  if (!preview.value) return
  if (!text.trim()) {
    preview.value.innerHTML = `<p class="nt-md-empty">${t('markdown.previewEmpty')}</p>`
    return
  }
  const html = await md.renderHtml(text)
  if (token !== renderToken || !preview.value) return
  preview.value.innerHTML = html
  await nextTick()
  if (token !== renderToken) return
  await md.renderDiagrams(preview.value)
  if (token !== renderToken) return
  md.renderMath(preview.value)
}

// 源文本或主题变了就重渲染。400ms 防抖：打字过程中每键都渲染会把界面卡住。
let timer: number | null = null
watch(
  [source, () => settings.resolvedTheme],
  () => {
    if (timer !== null) window.clearTimeout(timer)
    timer = window.setTimeout(() => {
      timer = null
      md.setDark(settings.resolvedTheme === 'dark')
      void render()
    }, 400)
  },
)

onMounted(() => void render())
onBeforeUnmount(() => {
  if (timer !== null) window.clearTimeout(timer)
})

/** 导出 HTML：把当前渲染结果包一层样式，落盘到源文件同目录。 */
async function exportHtml() {
  exporting.value = true
  try {
    const html = await md.renderHtml(source.value)
    const out = `${props.path.replace(/\.[^.]+$/, '')}.html`
    const full = wrapHtml(name.value, html)
    const res = await api.writeFile(out, full)
    if (res) toast.ok(t('markdown.exported', { path: out }))
  } catch (e) {
    toast.bad(normalizeError(e).message)
  } finally {
    exporting.value = false
  }
}

/**
 * 导出用的完整 HTML：样式内联，不引外部 CSS。
 *
 * 导出的文件要在别的机器 / 浏览器里打开，那里没有 themes.css；
 * 而且深色偏好也不同，所以固定浅色底 + 等宽正文，保证可读。
 */
function wrapHtml(title: string, body: string): string {
  return `<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>${title}</title>
<style>
body { max-width: 760px; margin: 40px auto; padding: 0 20px; font-family: system-ui, "Microsoft YaHei", sans-serif; line-height: 1.7; color: #212529; }
pre { background: #f8f9fa; border: 1px solid #dee2e6; border-radius: 8px; padding: 12px; overflow: auto; }
code { font-family: "Cascadia Mono", Consolas, monospace; font-size: 0.92em; }
p code, li code { background: #f8f9fa; padding: 1px 5px; border-radius: 4px; }
blockquote { margin: 0; padding: 4px 16px; border-left: 3px solid #adb5bd; color: #495057; }
table { border-collapse: collapse; } th, td { border: 1px solid #dee2e6; padding: 6px 12px; }
img { max-width: 100%; }
.nt-md-mermaid { text-align: center; margin: 16px 0; }
.nt-md-mermaid.is-failed { white-space: pre-wrap; font-family: monospace; color: #dc2626; }
h1, h2 { border-bottom: 1px solid #dee2e6; padding-bottom: 6px; }
</style>
</head>
<body>
${body}
</body>
</html>`
}
</script>

<style scoped>
.nt-md {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  min-width: 0;
}

.nt-md-head {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  height: var(--nt-h-toolbar);
  padding: 0 var(--nt-sp-2);
  flex: none;
  border-bottom: 1px solid var(--nt-border);
  background: var(--nt-chrome);
}

.nt-md-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--nt-fs-label);
}

.nt-md-spacer {
  flex: 1;
}

.nt-md-body {
  display: flex;
  flex: 1;
  min-height: 0;
  min-width: 0;
}

.nt-md-body.is-edit > .nt-md-preview,
.nt-md-body.is-preview > .nt-md-edit {
  display: none;
}

.nt-md-edit {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
}

/* 分屏：中缝 1px，两侧各自滚动 */
.nt-md-body.is-split > .nt-md-edit {
  border-right: 1px solid var(--nt-border);
}

.nt-md-preview {
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow: auto;
  padding: var(--nt-sp-5) var(--nt-sp-6);
  background: var(--nt-editor-bg);
  font-size: var(--nt-fs-md);
  line-height: var(--nt-lh-base);
}

.nt-md-empty {
  color: var(--nt-ink-muted);
}
</style>
