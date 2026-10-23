// Markdown 三态：编辑 / 分屏 / 预览。
// 正文由 CodeEditor 持有，预览只读正文渲染结果；两边宽度可拖。
// 根类名不叫 .nt-md：content.css 的 .nt-md 是给帮助页渲染结果用的
// （max-width + margin:auto），挂到面板上会把整块面板压窄居中。
:<template>
  <div class="nt-md-view">
    <div class="nt-md-head">
      <ElRadioGroup v-model="mode" size="small" class="nt-md-modes">
        <ElRadioButton value="edit">{{ t('markdown.edit') }}</ElRadioButton>
        <ElRadioButton value="split">{{ t('markdown.split') }}</ElRadioButton>
        <ElRadioButton value="preview">{{ t('markdown.preview') }}</ElRadioButton>
      </ElRadioGroup>
      <div class="nt-md-spacer" />
      <span class="nt-readout nt-md-name" :title="path">{{ name }}</span>
      <ElButton size="small" :loading="exporting" @click="exportHtml">
        {{ t('markdown.exportHtml') }}
      </ElButton>
    </div>

    <div class="nt-md-body" :class="`is-${mode}`">
      <div v-show="mode !== 'preview'" class="nt-md-edit" :style="editStyle">
        <CodeEditor :tab="tab" view-id="main" />
      </div>
      <div
        v-show="mode === 'split'"
        class="nt-md-divider"
        role="separator"
        :title="t('markdown.dragHint')"
        @mousedown="startDrag"
      />
      <div v-show="mode !== 'edit'" class="nt-md-preview">
        <article ref="doc" class="nt-md-doc" @click="onDocClick" />
      </div>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElRadioButton, ElRadioGroup } from 'element-plus'
import { convertFileSrc } from '@tauri-apps/api/core'
import CodeEditor from '@/components/editor/CodeEditor.vue'
import { useMarkdown } from '@/composables/useMarkdown'
import { useEditorStore } from '@/stores/editor'
import { useSettingsStore } from '@/stores/settings'
import { useToastStore } from '@/stores/toast'
import { api, normalizeError } from '@/api'
import { getText, docVersionOf } from '@/composables/useEditorRegistry'
import { baseName, copyText, dirName, joinPath } from '@/utils'
import type { Tab } from '@/stores/editor'

const props = defineProps<{ path: string; tab: Tab }>()

const { t } = useI18n()
const settings = useSettingsStore()
const editor = useEditorStore()
const toast = useToastStore()
const md = useMarkdown()

type Mode = 'edit' | 'split' | 'preview'

/**
 * 默认编辑态。
 *
 * `.md` 与其他文本文件一样，打开就是左上角的源码视图；要看渲染结果按
 * 分屏 / 预览。不做「记住上次模式」：切过一次就永远不再给源码，
 * 下次打开的人会以为文件坏了。
 */
const mode = ref<Mode>('edit')
/** 渲染落点：阅读栏本体。宽度与内边距归 `.nt-md-doc`，滚动归 `.nt-md-preview`。 */
const doc = ref<HTMLElement | null>(null)
const exporting = ref(false)

const name = computed(() => baseName(props.path))

/**
 * 源文本：从编辑器注册表拿，不往 store 里存正文（AGENTS.md §1.2）。
 *
 * `docVersionOf` 是必需的依赖：注册表是普通 Map，只读 getText 的话
 * computed 求值一次拿到 '' 就再不失效，预览会永远停在空态。
 */
const source = computed(() => {
  docVersionOf(props.tab.id)
  return getText(props.tab.id)
})

md.setDark(settings.resolvedTheme === 'dark')

// ---- 分屏比例：拖中缝，比例记在本次会话里 ----
/** 编辑栏占比（0.25–0.75）。太窄的两栏都读不了。 */
const ratio = ref(0.5)

/**
 * 分屏时编辑栏的宽度。
 *
 * 必须写 `0 0 X%` 而不是只写 flex-basis：剩下一栏是 `flex: 1 1 0`，
 * 只给 basis 的话两边会均分剩余空间，50% 会被撑到 75%。
 */
const editStyle = computed(() =>
  mode.value === 'split' ? { flex: `0 0 ${(ratio.value * 100).toFixed(2)}%` } : undefined,
)

function startDrag(e: MouseEvent) {
  e.preventDefault()
  const body = (e.currentTarget as HTMLElement).parentElement
  const width = body?.clientWidth ?? 0
  if (!width) return
  const startX = e.clientX
  const startRatio = ratio.value
  document.body.classList.add('nt-col-resizing')
  const onMove = (ev: MouseEvent) => {
    const next = startRatio + (ev.clientX - startX) / width
    ratio.value = Math.min(0.75, Math.max(0.25, next))
  }
  const onUp = () => {
    document.body.classList.remove('nt-col-resizing')
    document.removeEventListener('mousemove', onMove)
    document.removeEventListener('mouseup', onUp)
  }
  document.addEventListener('mousemove', onMove)
  document.addEventListener('mouseup', onUp)
}

/** 渲染一遍：HTML → 图 → 公式。三步异步，中途换文本会让前面的白做。 */
let renderToken = 0

async function render() {
  // 编辑态不渲染：打开一个几百 KB 的 .md 不该先付一遍 markdown 解析的钱。
  // 切到分屏 / 预览时由 mode 的 watcher 补。
  if (mode.value === 'edit') return
  const token = ++renderToken
  const text = source.value
  // 预览容器可能还没挂上（切模式后首帧）：等一个 nextTick 再取，
  // 取不到就整轮作废——不能写成静默 return，那会让预览永远空着。
  if (!doc.value) {
    await nextTick()
    if (token !== renderToken) return
  }
  if (!doc.value) return
  if (!text.trim()) {
    doc.value.innerHTML = `<p class="nt-md-empty">${t('markdown.previewEmpty')}</p>`
    return
  }
  const html = await md.renderHtml(text)
  if (token !== renderToken || !doc.value) return
  doc.value.innerHTML = html
  resolveImages(doc.value)
  await nextTick()
  if (token !== renderToken) return
  await md.renderDiagrams(doc.value)
  if (token !== renderToken) return
  md.renderMath(doc.value)
}

/**
 * 相对路径的图片改走 Tauri 的 asset 协议。
 *
 * 文档里的 `![](./img/a.png)` 是相对当前文件的：WebView 按页面 URL 解析会
 * 404，必须换成 `convertFileSrc(绝对路径)` 才加载得出来。
 */
function resolveImages(root: HTMLElement) {
  for (const img of Array.from(root.querySelectorAll('img'))) {
    const src = img.getAttribute('src') ?? ''
    if (!src || /^(https?:|data:|blob:|asset:|tauri:)/i.test(src)) continue
    const abs = /^[a-zA-Z]:[\\/]/.test(src) ? src : joinPath(dirName(props.path), safeDecode(src))
    img.src = convertFileSrc(abs)
  }
}

/** 解码 URL：文档里的中文文件名常被转义，非法转义则按原样用。 */
function safeDecode(s: string): string {
  try {
    return decodeURI(s)
  } catch {
    return s
  }
}

// 源文本或主题变了就重渲染。400ms 防抖：打字过程中每键都渲染会把界面卡住。
let timer: number | null = null
watch(
  [source, () => settings.resolvedTheme],
  () => {
    md.setDark(settings.resolvedTheme === 'dark')
    if (mode.value === 'edit') return
    if (timer !== null) window.clearTimeout(timer)
    timer = window.setTimeout(() => {
      timer = null
      void render()
    }, 400)
  },
)

/** 切到含预览的模式就重渲染一次：编辑期间跳过的改动要在这里补上。 */
watch(mode, (m) => {
  if (m !== 'edit') void render()
})

onMounted(() => void render())
onBeforeUnmount(() => {
  if (timer !== null) window.clearTimeout(timer)
})

/**
 * 预览里的链接点击。
 *
 * 三种目标三种走法：`#anchor` 在本文内跳转（markdown-it 给标题生成 id），
 * `http(s)` 交给系统浏览器，其余当相对路径打开同目录的另一个文件——
 * 文档集里最常见的就是「点进下一篇」。
 */
async function onDocClick(e: MouseEvent) {
  const target = e.target as HTMLElement

  // 代码块标题栏的「复制」：取 pre 的纯文本，不碰行号与标题栏。
  const copyBtn = target.closest<HTMLButtonElement>('.nt-md-code-copy')
  if (copyBtn) {
    const code = copyBtn.closest('.nt-md-code')?.querySelector('pre')?.textContent ?? ''
    if (await copyText(code)) flashCopied(copyBtn)
    return
  }

  const a = target.closest('a')
  if (!a) return
  const href = a.getAttribute('href') ?? ''
  if (!href) return
  e.preventDefault()

  if (href.startsWith('#')) {
    jumpToAnchor(href.slice(1))
    return
  }
  if (/^(https?|mailto):/i.test(href)) {
    try {
      await api.openPath(href)
    } catch (err) {
      toast.bad(normalizeError(err).message)
    }
    return
  }
  // 相对路径：按当前文件所在目录解析，去掉锚点后再打开。
  const rel = decodeURI(href.split('#')[0])
  if (!rel) return
  const abs = /^[a-zA-Z]:[\\/]/.test(rel) ? rel : joinPath(dirName(props.path), rel)
  try {
    await editor.openPath(abs)
  } catch (err) {
    toast.warn(normalizeError(err).message)
  }
}

/** 复制成功后就地反馈 1.2 秒：只改文字不换尺寸，按钮不会跳。 */
function flashCopied(btn: HTMLButtonElement) {
  const original = btn.textContent ?? ''
  btn.textContent = t('markdown.copied')
  btn.disabled = true
  window.setTimeout(() => {
    btn.textContent = original || t('markdown.copyCode')
    btn.disabled = false
  }, 1200)
}

/** 跳到标题锚点：命中后给一段底色反馈，用户才知道自己落到哪了。 */
function jumpToAnchor(rawId: string) {
  const root = doc.value
  if (!root) return
  let id = rawId
  try {
    id = decodeURIComponent(rawId)
  } catch {
    /* 非法转义就按原文找 */
  }
  const hit =
    root.querySelector<HTMLElement>(`[id="${id.replace(/"/g, '\\"')}"]`) ??
    root.querySelector<HTMLElement>(`[id="${id.toLowerCase()}"]`)
  if (!hit) {
    toast.warn(t('markdown.anchorMissing', { id }))
    return
  }
  hit.scrollIntoView({ block: 'start', behavior: 'smooth' })
  hit.classList.remove('is-anchor-flash')
  // 强制重排后再加：同一次点击的第二次跳转要能再播一遍
  void hit.offsetWidth
  hit.classList.add('is-anchor-flash')
  window.setTimeout(() => hit.classList.remove('is-anchor-flash'), 1000)
}

/** 导出 HTML：把当前渲染结果包一层样式，落盘到源文件同目录。 */
async function exportHtml() {
  exporting.value = true
  try {
    // 导出件要能在别的机器上双击打开：去掉代码块的复制按钮（没有脚本可跑）。
    const html = (await md.renderHtml(source.value)).replace(
      /<div class="nt-md-code-bar">[\s\S]*?<\/div>/g,
      '',
    )
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
body { max-width: 760px; margin: 40px auto; padding: 0 20px; font-family: system-ui, "Microsoft YaHei", sans-serif; line-height: 1.7; color: #212529; overflow-wrap: anywhere; }
.nt-md-code { margin: 16px 0; background: #f8f9fa; border: 1px solid #dee2e6; border-radius: 8px; overflow: hidden; }
.nt-md-code pre { margin: 0; border: 0; border-radius: 0; }
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
.nt-md-view {
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
  max-width: 42%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--nt-fs-label);
  color: var(--nt-ink-muted);
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

.nt-md-edit {
  flex: 1 1 0;
  min-width: 0;
  min-height: 0;
  display: flex;
}

/* 中缝：6px 热区，视觉上 1px。热区太窄时鼠标抓不住。 */
.nt-md-divider {
  flex: none;
  width: 6px;
  margin: 0 -2px;
  z-index: 2;
  cursor: col-resize;
  position: relative;
  transition: background-color var(--nt-dur-fast) var(--nt-ease);
}

.nt-md-divider::after {
  content: '';
  position: absolute;
  inset: 0 auto 0 2px;
  width: 1px;
  background: var(--nt-border);
  transition: background-color var(--nt-dur-fast) var(--nt-ease);
}

.nt-md-divider:hover::after {
  background: var(--nt-primary);
}

.nt-md-preview {
  flex: 1 1 0;
  min-width: 0;
  min-height: 0;
  background: var(--nt-editor-bg);
  font-size: var(--nt-fs-md);
  line-height: var(--nt-lh-loose);
  user-select: text;
}

/* 单栏模式：另一种视图收起来，剩下的一栏吃满 */
.nt-md-body.is-edit > .nt-md-edit,
.nt-md-body.is-preview > .nt-md-preview {
  flex: 1 1 100%;
}

</style>
