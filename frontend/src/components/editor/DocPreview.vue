// 文档预览：markdown / xlsx / docx / pdf / 图片 / 纯文本各走一条链路。
// 二进制不进 CodeMirror：解成文本是一屏乱码，而用户点开 .xlsx 想看的是表格。
:<template>
  <div class="nt-doc-preview">
    <div class="nt-doc-head">
      <span class="nt-readout">{{ fileName }}</span>
      <span v-if="meta" class="nt-readout nt-doc-meta" :title="meta">{{ meta }}</span>
      <div class="nt-doc-spacer" />
      <!-- 缩放只对表格 / Word 有意义：PDF 与图片由它们自己管 -->
      <div v-if="showZoom" class="nt-doc-zoom">
        <button class="nt-icon-btn" :title="t('viewer.zoomOut')" @click="stepZoom(-1)">
          <NtIcon name="zoom-out" size="sm" />
        </button>
        <button class="nt-doc-zoom-val" :title="t('viewer.zoomReset')" @click="zoom = 1">
          {{ Math.round(zoom * 100) }}%
        </button>
        <button class="nt-icon-btn" :title="t('viewer.zoomIn')" @click="stepZoom(1)">
          <NtIcon name="zoom-in" size="sm" />
        </button>
      </div>
      <ElButton size="small" @click="emit('openHex')">
        <NtIcon name="hash" size="sm" />
        <span class="nt-btn-text">{{ t('tool.hexViewer') }}</span>
      </ElButton>
      <ElButton size="small" @click="openExternal">
        <NtIcon name="external" size="sm" />
        <span class="nt-btn-text">{{ t('tool.openExternal') }}</span>
      </ElButton>
      <ElButton size="small" @click="emit('toggleEdit')">
        <NtIcon name="braces" size="sm" />
        <span class="nt-btn-text">{{ t('tool.backToEdit') }}</span>
      </ElButton>
    </div>

    <div v-if="kind === 'unsupported'" class="nt-empty">
      <p>{{ t('tool.previewUnsupported') }}</p>
      <p class="nt-empty-hint">{{ t('tool.previewUnsupportedHint') }}</p>
      <div class="nt-empty-actions">
        <ElButton size="small" @click="emit('openHex')">{{ t('tool.hexViewer') }}</ElButton>
        <ElButton size="small" type="primary" @click="openExternal">{{ t('tool.openExternal') }}</ElButton>
      </div>
    </div>

    <div v-else-if="kind === 'tooLarge'" class="nt-empty">
      <p>{{ t('tool.previewTooLarge') }}</p>
      <p class="nt-empty-hint">{{ t('tool.previewTooLargeHint') }}</p>
      <div class="nt-empty-actions">
        <ElButton size="small" type="primary" @click="openExternal">{{ t('tool.openExternal') }}</ElButton>
      </div>
    </div>

    <div v-else-if="kind === 'image'" class="nt-doc-image">
      <img :src="src" :alt="fileName" />
    </div>

    <!-- PDF 交给 WebView2 自带的阅读器：流式读取、支持滚动 / 缩放 / 搜索 / 打印，
         不必把整个文件读成 base64 再交给 JS 解析（那还有 20MB 上限）。 -->
    <div v-else-if="kind === 'pdf'" class="nt-doc-pdf">
      <embed type="application/pdf" :src="assetUrl" />
      <p class="nt-doc-foot">{{ t('tool.pdfHint') }}</p>
    </div>

    <!-- HTML：沙箱 iframe 静态预览。sandbox 全空 = 脚本、表单、同源全禁，
         style 标签与内联样式照常生效；正文随编辑 300ms 防抖刷新。 -->
    <div v-else-if="kind === 'html'" class="nt-doc-html">
      <iframe class="nt-html-frame" sandbox="" :srcdoc="htmlShown" :title="fileName" />
      <p class="nt-doc-foot">{{ t('tool.htmlHint') }}</p>
    </div>

    <!-- 表格：正文区 + 底部工作表页签。多表是常态，只给第一张等于没打开。 -->
    <div v-else-if="kind === 'sheet'" class="nt-doc-sheet">
      <div ref="host" class="nt-doc-host nt-sheet-host" :style="{ zoom }" @click="onHostClick" />
      <div v-if="sheets.length > 1" class="nt-doc-sheet-tabs">
        <button
          v-for="name in sheets"
          :key="name"
          class="nt-sheet-tab"
          :class="{ 'is-on': name === activeSheet }"
          @click="selectSheet(name)"
        >
          {{ name }}
        </button>
      </div>
    </div>

    <div
      v-else
      ref="host"
      class="nt-doc-host"
      :class="{ 'nt-md-preview': kind === 'markdown', 'is-plain': kind === 'text', 'nt-docx-host': kind === 'docx' }"
      :style="kind === 'docx' ? { zoom } : undefined"
      @click="onHostClick"
    />
  </div>
</template>

<script lang="ts" setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton } from 'element-plus'
import { convertFileSrc } from '@tauri-apps/api/core'
import { api, normalizeError } from '@/api'
import { useToastStore } from '@/stores/toast'
import { useMarkdown } from '@/composables/useMarkdown'
import { useSettingsStore } from '@/stores/settings'
import NtIcon from '@/components/common/NtIcon.vue'
import { baseName, copyText } from '@/utils'
import { kindOf } from '@/utils/fileKind'
import { docVersionOf, get } from '@/composables/useEditorRegistry'

const props = defineProps<{ path: string; tabId?: number }>()
const emit = defineEmits<{ openHex: []; toggleEdit: [] }>()

const { t } = useI18n()
const toast = useToastStore()
const settings = useSettingsStore()
const md = useMarkdown()

const host = ref<HTMLElement | null>(null)
const fileName = computed(() => baseName(props.path))
/** asset 协议 URL：图片与 PDF 都直接交给 Chromium 加载，不走 base64。 */
const assetUrl = computed(() => convertFileSrc(props.path))
const src = assetUrl

type Kind = 'sheet' | 'docx' | 'pdf' | 'image' | 'markdown' | 'html' | 'text' | 'unsupported' | 'tooLarge' | 'loading'
const kind = ref<Kind>('loading')

/** 头部读数：表格是区域与行列数，Word 是页数。 */
const meta = ref('')
/** 缩放：只作用于表格与 Word，PDF/图片由各自组件管。 */
const zoom = ref(1)
const showZoom = computed(() => kind.value === 'sheet' || kind.value === 'docx')

/** 工作表页签。单表不显示页签栏。 */
const sheets = ref<string[]>([])
const activeSheet = ref('')

/** 缩放档位：连续乘 1.1，限制在 50%~300%，避免缩到看不见或撑爆布局。 */
function stepZoom(dir: 1 | -1) {
  const next = dir > 0 ? zoom.value * 1.1 : zoom.value / 1.1
  zoom.value = Math.min(3, Math.max(0.5, Math.round(next * 100) / 100))
}

/** 渲染轮次：切换文件时旧轮次的异步结果不许写回新容器。 */
let token = 0

async function classify() {
  const run = ++token
  meta.value = ''
  sheets.value = []
  activeSheet.value = ''
  const k = kindOf(props.path)
  switch (k) {
    case 'image':
      kind.value = 'image'
      return
    case 'sheet':
      kind.value = 'sheet'
      await renderXlsx(run)
      return
    case 'docx':
      kind.value = 'docx'
      await renderDocx(run)
      return
    case 'pdf':
      // 不预读字节：WebView2 的阅读器会自己按需请求 range。
      kind.value = 'pdf'
      return
    case 'markdown':
      kind.value = 'markdown'
      await renderMarkdown(run)
      return
    case 'html':
      kind.value = 'html'
      await loadDiskHtml()
      return
    case 'text':
      await renderPlainText(run)
      return
    default:
      // 二进制：没有内嵌预览就直接给两条出路，不留一块空白
      kind.value = 'unsupported'
  }
}

/**
 * 纯文本预览走 read_file（带编码探测），不走字节硬解 UTF-8——
 * 后者对 GBK / UTF-16 文件只会得到乱码。
 */
async function renderPlainText(run: number) {
  if (!host.value) return
  try {
    const c = await api.readFile(props.path)
    if (run !== token || !host.value) return
    host.value.textContent = c.text
    kind.value = 'text'
  } catch (e) {
    if (run !== token) return
    toast.warn(normalizeError(e).message)
    kind.value = 'unsupported'
  }
}

/**
 * HTML 正文：编辑器里的实时内容优先（改动立刻能看），视图不在再退回磁盘。
 * computed 里读 docVersion 建立响应依赖——正文变了 iframe 才会跟着换。
 */
const diskHtml = ref('')
const htmlText = computed(() => {
  if (kind.value !== 'html') return ''
  if (props.tabId != null) {
    docVersionOf(props.tabId)
    const v = get(props.tabId)
    if (v) return v.state.doc.toString()
  }
  return diskHtml.value
})

/** iframe 的 srcdoc 防抖 300ms：每个键击重排 iframe 会闪，攒一下再换。 */
const htmlShown = ref('')
let htmlTimer: number | undefined
watch(
  htmlText,
  (v) => {
    window.clearTimeout(htmlTimer)
    htmlTimer = window.setTimeout(() => {
      htmlShown.value = v
    }, 300)
  },
  { immediate: true },
)

onBeforeUnmount(() => window.clearTimeout(htmlTimer))

/** HTML 兜底正文：编辑器视图不在时从磁盘读一份（带编码探测）。 */
async function loadDiskHtml() {
  try {
    const c = await api.readFile(props.path)
    diskHtml.value = c.text
  } catch {
    diskHtml.value = ''
  }
}

/** Markdown 预览：与编辑器里的预览同一条渲染链路，排版也同一份 CSS。 */
async function renderMarkdown(run: number) {
  if (!host.value) return
  md.setDark(settings.resolvedTheme === 'dark')
  try {
    const c = await api.readFile(props.path)
    if (run !== token || !host.value) return
    host.value.innerHTML = '<article class="nt-md-doc"></article>'
    const article = host.value.querySelector<HTMLElement>('.nt-md-doc')
    if (!article) return
    article.innerHTML = await md.renderHtml(c.text)
    if (run !== token) return
    await md.renderDiagrams(article)
    if (run !== token) return
    md.renderMath(article)
    kind.value = 'markdown'
  } catch (e) {
    if (run !== token) return
    toast.warn(normalizeError(e).message)
    kind.value = 'unsupported'
  }
}

/**
 * 读文件字节走后端 `read_binary`（base64），不走 WebView 的 fs 插件——
 * 后者等于把整块磁盘读权限交给渲染层，且缺少 20MB 上限保护。
 * 返回 null 表示已给出可读提示，调用方直接返回即可。
 */
async function readBytes(): Promise<Uint8Array | null> {
  try {
    const b64 = await api.readBinary(props.path)
    return base64ToBytes(b64)
  } catch (e) {
    const err = normalizeError(e)
    // 超出预览上限不算「不支持」：给一句能照做的提示，别让人以为文件坏了
    kind.value = err.code === '1004' ? 'tooLarge' : 'unsupported'
    toast.warn(err.message)
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

/** 单表最多渲染的行数：几万行的表全铺进 DOM 会把界面卡死。 */
const MAX_SHEET_ROWS = 2000
/** 单表最多渲染的列数：再宽也没人横向翻到底。 */
const MAX_SHEET_COLS = 80

/** 工作簿常驻：切页签不必重新解压文件。 */
let workbook: import('xlsx').WorkBook | null = null
let xlsxLib: typeof import('xlsx') | null = null

async function renderXlsx(run: number) {
  if (!host.value) return
  const data = await readBytes()
  if (!data || run !== token || !host.value) return
  try {
    const lib = await import('xlsx')
    xlsxLib = lib
    workbook = lib.read(data, { type: 'array', cellDates: true })
    sheets.value = workbook.SheetNames.slice()
    if (sheets.value.length === 0) {
      kind.value = 'unsupported'
      return
    }
    kind.value = 'sheet'
    // 换分支会换掉 ref 指向的元素：等 DOM 落地再往里写表格
    await nextTick()
    if (run !== token) return
    selectSheet(sheets.value[0])
  } catch (e) {
    // 表格损坏 / 加密 / 不是真的 xlsx：给可读的出路，不要留白屏
    reportBroken(e)
  }
}

/** 切到某张工作表：重建表格 HTML，并刷新头部读数。 */
function selectSheet(name: string) {
  activeSheet.value = name
  const ws = workbook?.Sheets[name]
  if (!ws || !host.value || !xlsxLib) return
  const built = buildSheetTable(xlsxLib, ws)
  host.value.innerHTML = built.html
  meta.value = built.meta
}

/** 单元格转义：表格内容来自文件，必须按文本插入。 */
function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

/**
 * 把工作表渲染成表格：自己拼 HTML（读列宽 / 数字右对齐 / 行数上限 / 合并单元格），
 * 不用 sheet_to_html——它不读列宽、不给数字对齐、也不处理行数上限。
 */
function buildSheetTable(
  lib: typeof import('xlsx'),
  ws: import('xlsx').WorkSheet,
): { html: string; meta: string } {
  const ref = ws['!ref']
  if (!ref) return { html: `<p class="nt-sheet-note">${t('viewer.sheetEmpty')}</p>`, meta: '' }

  const range = lib.utils.decode_range(ref)
  const totalRows = range.e.r - range.s.r + 1
  const totalCols = range.e.c - range.s.c + 1
  const colEnd = range.s.c + Math.min(totalCols, MAX_SHEET_COLS) - 1
  const rowEnd = Math.min(range.e.r, range.s.r + MAX_SHEET_ROWS - 1)
  // raw:false 取的是「显示值」（日期格式化、百分比带 %），与 Excel 里看到的一致。
  const rows = lib.utils.sheet_to_json<unknown[]>(ws, {
    header: 1,
    defval: '',
    raw: false,
    blankrows: true,
  })

  const anchors = new Map<string, import('xlsx').Range>()
  const covered = new Set<string>()
  for (const m of ws['!merges'] ?? []) {
    anchors.set(`${m.s.r}:${m.s.c}`, m)
    for (let r = m.s.r; r <= m.e.r; r += 1) {
      for (let c = m.s.c; c <= m.e.c; c += 1) {
        if (r !== m.s.r || c !== m.s.c) covered.add(`${r}:${c}`)
      }
    }
  }

  // 列宽：xlsx 里有 wch 就用它（Excel 的「字符宽度」）；CSV 没有列信息，
  // 按内容估一个——否则所有列都是同一个宽度，长名字全被裁成「…」。
  const sampleEnd = Math.min(rowEnd, range.s.r + 200)
  const cols: string[] = []
  for (let c = range.s.c; c <= colEnd; c += 1) {
    const wch = ws['!cols']?.[c]?.wch
    let width: number
    if (wch && wch > 0) {
      width = Math.round(wch * 7.4) + 16
    } else {
      let longest = 4
      for (let r = range.s.r; r <= sampleEnd; r += 1) {
        const v = rows[r]?.[c]
        if (v == null) continue
        // 中日韩字符按 2 个半角宽度算，否则中文列会明显偏窄
        const s = String(v)
        let w = 0
        for (const ch of s) w += /[\u2e80-\u9fff\uff00-\uffef]/.test(ch) ? 2 : 1
        if (w > longest) longest = w
      }
      width = Math.round(longest * 7.2) + 18
    }
    cols.push(`${Math.min(360, Math.max(56, width))}px`)
  }

  // 行号列固定 44px，与列字母行同宽：两者对齐才有「表格骨架」的观感
  const parts: string[] = [
    '<table class="nt-sheet-table"><colgroup><col style="width:44px">',
    cols.map((w) => `<col style="width:${w}">`).join(''),
    '</colgroup>',
  ]
  // 列字母行（A、B、C…）：宽表横向滚动时定位全靠它
  parts.push('<tr class="nt-sheet-head"><td class="nt-sheet-corner"></td>')
  for (let c = range.s.c; c <= colEnd; c += 1) {
    parts.push(`<td class="nt-sheet-col">${lib.utils.encode_col(c)}</td>`)
  }
  parts.push('</tr>')

  // 第一行是否值得冻结：只有「一行普通表头」才冻。跨列合并的第一行通常是
  // 大标题（如「XX 明细（1=已使用）」），冻住会把真正的列名挤出视野。
  const firstRowTitle = [...anchors.values()].some((m) => m.s.r === range.s.r && m.e.c > m.s.c)

  for (let r = range.s.r; r <= rowEnd; r += 1) {
    const freeze = r === range.s.r && !firstRowTitle ? ' class="nt-sheet-freeze"' : ''
    parts.push(`<tr${freeze}><td class="nt-sheet-rownum">${r + 1}</td>`)
    for (let c = range.s.c; c <= colEnd; c += 1) {
      const key = `${r}:${c}`
      if (covered.has(key)) continue
      const m = anchors.get(key)
      let span = ''
      if (m) {
        const rs = m.e.r - m.s.r + 1
        const cs = Math.min(m.e.c, colEnd) - m.s.c + 1
        span = ` rowspan="${rs}" colspan="${cs}"`
      }
      const raw = rows[r]?.[c]
      const text = raw == null ? '' : String(raw)
      // 数字右对齐：一列金额左对齐就没法看列尾数
      const numeric = /^-?[\d,]+(\.\d+)?%?$/.test(text.trim()) && text.trim() !== ''
      parts.push(`<td${span}${numeric ? ' class="is-num"' : ''}>${escapeHtml(text)}</td>`)
    }
    parts.push('</tr>')
  }
  parts.push('</table>')
  if (totalRows > MAX_SHEET_ROWS) {
    parts.push(
      `<p class="nt-sheet-note">${t('viewer.sheetTruncated', { shown: MAX_SHEET_ROWS, total: totalRows })}</p>`,
    )
  }
  return { html: parts.join(''), meta: `${ref} · ${totalRows} × ${totalCols}` }
}

async function renderDocx(run: number) {
  if (!host.value) return
  const data = await readBytes()
  if (!data || run !== token) return
  try {
    const { renderAsync } = await import('docx-preview')
    // experimental 打开后表格 / 图形 / 页眉页脚的还原度高得多；
    // useBase64URL 让文档里的图片不依赖相对路径解析。
    await renderAsync(data, host.value, undefined, {
      className: 'docx',
      inWrapper: true,
      breakPages: true,
      renderHeaders: true,
      renderFooters: true,
      renderFootnotes: true,
      ignoreLastRenderedPageBreak: false,
      experimental: true,
      useBase64URL: true,
    })
    if (run !== token || !host.value) return
    const pages = host.value.querySelectorAll('section.docx').length
    // 空文档 / 只有域代码的文档：不报错也不出页面，留白屏最糟
    if (pages === 0) {
      reportBroken(new Error(t('viewer.docxEmpty')))
      return
    }
    meta.value = t('viewer.pages', { n: pages })
  } catch (e) {
    reportBroken(e)
  }
}

/** 预览器内部抛错：统一退到「没有内置预览」卡片，保住两条出路。 */
function reportBroken(e: unknown) {
  toast.warn(normalizeError(e).message)
  kind.value = 'unsupported'
}

/** 预览里的代码块「复制」按钮：与编辑器预览同一套标记。 */
async function onHostClick(e: MouseEvent) {
  const btn = (e.target as HTMLElement).closest<HTMLButtonElement>('.nt-md-code-copy')
  if (!btn) return
  const code = btn.closest('.nt-md-code')?.querySelector('pre')?.textContent ?? ''
  if (!(await copyText(code))) return
  const original = btn.textContent ?? ''
  btn.textContent = t('markdown.copied')
  btn.disabled = true
  window.setTimeout(() => {
    btn.textContent = original || t('markdown.copyCode')
    btn.disabled = false
  }, 1200)
}

function openExternal() {
  void api.openPath(props.path)
}

onMounted(classify)
onBeforeUnmount(() => {
  token += 1
})

watch(() => props.path, () => {
  if (host.value) {
    host.value.innerHTML = ''
    host.value.className = 'nt-doc-host'
  }
  workbook = null
  zoom.value = 1
  kind.value = 'loading'
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
  gap: var(--nt-sp-2);
  flex: none;
  height: var(--nt-h-toolbar);
  padding: 0 var(--nt-sp-2);
  border-bottom: 1px solid var(--nt-border);
  background: var(--nt-chrome);
}

.nt-doc-spacer {
  flex: 1;
}

/* 头部读数：区域 / 行列数 / 页数。可读即可，不做胶囊——它没有动作 */
.nt-doc-meta {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-muted);
}

.nt-doc-zoom {
  display: flex;
  align-items: center;
  gap: 2px;
  flex: none;
}

.nt-doc-zoom-val {
  min-width: 46px;
  height: var(--nt-ctl-h-sm);
  padding: 0 var(--nt-sp-1);
  font-size: var(--nt-fs-hint);
  font-variant-numeric: tabular-nums;
  color: var(--nt-ink-2);
  background: transparent;
  border: 0;
  border-radius: var(--nt-radius-xs);
  cursor: pointer;
}

.nt-doc-zoom-val:hover {
  color: var(--nt-ink);
  background: var(--nt-surface-hover);
}

.nt-doc-host {
  flex: 1;
  min-height: 0;
  overflow: auto;
  background: var(--nt-surface);
  padding: var(--nt-sp-4);
}

/* Word：纸外底色比纸暗一档，滚动时能看出「这是一页纸」 */
.nt-doc-host.nt-docx-host {
  padding: var(--nt-sp-4);
  background: var(--nt-elev-1);
}

/* 表格：自带边框，不留内边距——吸顶行要贴住滚动容器的顶边 */
.nt-doc-host.nt-sheet-host {
  padding: 0;
  background: var(--nt-editor-bg);
}

.nt-doc-sheet {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

/* 工作表页签：Excel 里最常用的切换动作，放在底部贴边 */
.nt-doc-sheet-tabs {
  flex: none;
  display: flex;
  align-items: center;
  gap: 2px;
  padding: var(--nt-sp-1) var(--nt-sp-2);
  overflow-x: auto;
  background: var(--nt-chrome);
  border-top: 1px solid var(--nt-border);
}

.nt-sheet-tab {
  flex: none;
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  height: calc(var(--nt-ctl-h-sm) - 2px);
  padding: 0 var(--nt-sp-3);
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-2);
  background: transparent;
  border: 1px solid transparent;
  border-radius: var(--nt-radius-xs);
  cursor: pointer;
  transition:
    color var(--nt-dur-fast) var(--nt-ease),
    background-color var(--nt-dur-fast) var(--nt-ease),
    border-color var(--nt-dur-fast) var(--nt-ease);
}

.nt-sheet-tab:hover {
  color: var(--nt-ink);
  background: var(--nt-surface-hover);
}

.nt-sheet-tab.is-on {
  color: var(--nt-primary);
  background: var(--nt-primary-soft);
  border-color: var(--nt-primary-soft);
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

/* PDF：交给 WebView2 的阅读器，它自己带工具栏与滚动条 */
.nt-doc-pdf {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-1);
}

/* HTML：沙箱 iframe 铺满正文区。底色走编辑器面（随明暗），页面自己的背景色盖在上面 */
.nt-doc-html {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-1);
}

.nt-html-frame {
  flex: 1;
  min-height: 0;
  width: 100%;
  border: 0;
  border-radius: var(--nt-radius-sm);
  background: var(--nt-editor-bg);
}

.nt-doc-pdf embed {
  flex: 1;
  min-height: 0;
  width: 100%;
  border: 0;
  border-radius: var(--nt-radius-sm);
  background: var(--nt-elev-1);
}

/* 阅读器是系统组件：它内部的主题跟不了明暗，给一句兜底提示 */
.nt-doc-foot {
  flex: none;
  margin: 0;
  font-size: var(--nt-fs-2xs);
  color: var(--nt-ink-muted);
}

.nt-doc-image img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
}

/* 空态：给出路，不只是说「不行」 */
.nt-empty-actions {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: var(--nt-sp-2);
  margin-top: var(--nt-sp-2);
}

/* Markdown 预览：外壳自己不加内边距，交给 .nt-md-doc 的阅读栏控制 */
.nt-doc-host.nt-md-preview {
  padding: 0;
  background: var(--nt-editor-bg);
}

/* 表格预览：单元格描边走令牌，默认的 sheet_to_html 没有样式 */
.nt-doc-host :deep(table) {
  border-collapse: collapse;
  font-size: var(--nt-fs-sm);
}

.nt-doc-host :deep(td) {
  border: 1px solid var(--nt-border);
  padding: var(--nt-sp-1) var(--nt-sp-2);
  color: var(--nt-ink);
  white-space: nowrap;
}

/* docx-preview 自己带白色纸张样式：深色主题下把纸压到工作区色 */
.nt-doc-host :deep(.docx-wrapper) {
  background: var(--nt-elev-1);
  padding: var(--nt-sp-4) 0;
}

.nt-doc-host :deep(.docx-wrapper > section.docx) {
  box-shadow: var(--nt-card-shadow);
  margin-bottom: var(--nt-sp-4);
}
</style>
