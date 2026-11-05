// 函数列表面板：从当前文件里正则提取符号（函数 / 类 / 结构体）。
// 为什么不引 tree-sitter：真正的 AST 解析器体积大且要 wasm，

<template>
  <div class="nt-panel">
    <div class="nt-panel-head">
      <span>{{ t('sidebar.functions') }}</span>
      <button class="nt-icon-btn" :title="t('common.refresh')" @click="scan">
        <NtIcon name="refresh" size="sm" />
      </button>
    </div>
    <div v-if="!editor.active" class="nt-empty">
      <p>{{ t('sidebar.noFunctions') }}</p>
    </div>
    <div v-else-if="symbols.length === 0" class="nt-empty">
      <p>{{ t('sidebar.noFunctions') }}</p>
    </div>
    <div v-else class="nt-panel-list">
      <button
        v-for="s in symbols"
        :key="`${s.line}:${s.name}`"
        class="nt-row-item nt-symbol-row"
        @click="onJump(s)"
      >
        <NtIcon :name="iconOf(s.kind)" class="nt-symbol-icon" />
        <span class="nt-symbol-name">{{ s.name }}</span>
        <span class="nt-readout nt-symbol-line">{{ s.line }}</span>
      </button>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import NtIcon from '@/components/common/NtIcon.vue'
import { useEditorStore } from '@/stores/editor'
import { getText, docVersionOf, bumpDocVersion } from '@/composables/useEditorRegistry'

const { t } = useI18n()
const editor = useEditorStore()

type Kind = 'function' | 'class' | 'struct' | 'interface' | 'const'
interface Symbol {
  name: string
  line: number
  kind: Kind
}

/**
 * 符号表。
 *
 * **不能写成 computed(() => extract(getText(id)))**：那会让每次按键都把整篇
 * 正文 `toString()` 一遍再跑一遍正则，几百 KB 的文件上打字就卡。
 * 这里由正文版本号驱动的防抖刷新填充，打字期间不重扫。
 */
const symbols = ref<Symbol[]>([])

/** 正文版本号：只用来触发「停手后重扫」的 watch。 */
const version = computed(() => (editor.active ? docVersionOf(editor.active.id) : 0))

/** 重扫防抖：用户停手 260ms 后扫一次。 */
let scanTimer = 0
function scheduleScan() {
  if (scanTimer) window.clearTimeout(scanTimer)
  scanTimer = window.setTimeout(() => {
    scanTimer = 0
    rescan()
  }, 260)
}

function rescan() {
  const tab = editor.active
  if (!tab) {
    symbols.value = []
    return
  }
  const text = getText(tab.id)
  symbols.value = text ? extract(text, tab.language) : []
}

function iconOf(kind: Kind): string {
  switch (kind) {
    case 'class':
      return 'tag'
    case 'struct':
      return 'braces'
    case 'interface':
      return 'list'
    case 'const':
      return 'hash'
    default:
      return 'braces'
  }
}

function onJump(s: Symbol) {
  document.dispatchEvent(
    new CustomEvent('editor-goto-line', {
      detail: { tabId: editor.activeId, line: s.line },
    }),
  )
}

/** 手动刷新：立刻重扫（按钮点下去就该有反馈）。 */
function scan() {
  if (scanTimer) window.clearTimeout(scanTimer)
  scanTimer = 0
  if (editor.activeId != null) bumpDocVersion(editor.activeId)
  rescan()
}

onMounted(scan)
onBeforeUnmount(() => {
  if (scanTimer) window.clearTimeout(scanTimer)
})

/**
 * 每种语言的符号正则。只认「行首缩进 + 关键字」的形态：
 * 宁可漏（漏了用户自己搜）也不要满屏误报。
 */
function extract(text: string, lang: string): Symbol[] {
  const lines = text.split('\n')
  const out: Symbol[] = []
  const patterns: Array<[RegExp, Kind]> = patternsFor(lang)

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i]
    // 超过 20000 行就不扫：列表渲染与正则都要时间，大文件用查找代替。
    if (i > 20000) break
    for (const [re, kind] of patterns) {
      const m = re.exec(line)
      if (m?.[1]) {
        out.push({ name: m[1], line: i + 1, kind })
        break
      }
    }
  }
  return out
}

function patternsFor(lang: string): Array<[RegExp, Kind]> {
  const common: Array<[RegExp, Kind]> = [
    [/^\s*(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_$][\w$]*)/, 'function'],
    [/^\s*(?:export\s+)?class\s+([A-Za-z_$][\w$]*)/, 'class'],
  ]
  switch (lang) {
    case 'rust':
      return [
        [/^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z_]\w*)/, 'function'],
        [/^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+([A-Za-z_]\w*)/, 'struct'],
        [/^\s*(?:pub(?:\([^)]*\))?\s+)?trait\s+([A-Za-z_]\w*)/, 'interface'],
        [/^\s*(?:pub(?:\([^)]*\))?\s+)?enum\s+([A-Za-z_]\w*)/, 'struct'],
        [/^\s*(?:pub(?:\([^)]*\))?\s+)?const\s+([A-Z_]\w*)/, 'const'],
      ]
    case 'go':
      return [
        [/^\s*func\s+(?:\([^)]*\)\s*)?([A-Za-z_]\w*)/, 'function'],
        [/^\s*type\s+([A-Za-z_]\w*)\s+struct/, 'struct'],
        [/^\s*type\s+([A-Za-z_]\w*)\s+interface/, 'interface'],
      ]
    case 'python':
      return [
        [/^\s*(?:async\s+)?def\s+([A-Za-z_]\w*)/, 'function'],
        [/^\s*class\s+([A-Za-z_]\w*)/, 'class'],
      ]
    case 'java':
    case 'kotlin':
    case 'csharp':
      return [
        [/^\s*(?:public|private|protected|internal|static|final|abstract|\s)*\s*(?:class|interface)\s+([A-Za-z_]\w*)/, 'class'],
        [/^\s*(?:public|private|protected|internal|static|\s)*\s*[\w<>[\],\s]+\s+([A-Za-z_]\w*)\s*\(/, 'function'],
      ]
    case 'c':
    case 'cpp':
      return [
        [/^\s*(?:static\s+)?[\w:<>~\*&\s]+\s+([A-Za-z_]\w*)\s*\(/, 'function'],
        [/^\s*(?:struct|class)\s+([A-Za-z_]\w*)/, 'struct'],
      ]
    case 'typescript':
    case 'javascript':
      return [
        ...common,
        [/^\s*(?:export\s+)?interface\s+([A-Za-z_$][\w$]*)/, 'interface'],
        [/^\s*(?:export\s+)?(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?\(/, 'function'],
      ]
    case 'vue':
      return common
    default:
      return common
  }
}

// 切文件立刻重扫；同一个文件里打字则等停手。
watch(() => editor.activeId, () => scan())
watch(version, () => scheduleScan())
</script>

<style scoped>
.nt-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.nt-panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--nt-sp-1) var(--nt-sp-2) var(--nt-sp-1) var(--nt-sp-3);
  font-size: var(--nt-fs-label);
  font-weight: 600;
  color: var(--nt-ink);
  flex: none;
}

.nt-panel-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 0 var(--nt-sp-1) var(--nt-sp-2);
}

.nt-symbol-row {
  gap: var(--nt-sp-2);
  padding: var(--nt-sp-1) var(--nt-sp-2);
  width: 100%;
  text-align: left;
}

.nt-symbol-icon {
  color: var(--nt-primary);
  flex: none;
}

.nt-symbol-name {
  font-size: var(--nt-fs-sm);
  font-family: var(--font-mono);
  color: var(--nt-ink);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
  min-width: 0;
}

.nt-symbol-line {
  font-size: var(--nt-fs-hint);
  flex: none;
}

.nt-empty {
  padding: var(--nt-sp-6) var(--nt-sp-3);
  text-align: center;
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-muted);
}
</style>
