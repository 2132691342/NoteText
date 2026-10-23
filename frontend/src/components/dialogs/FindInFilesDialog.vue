// 目录查找浮窗：在整个文件夹里搜，结果按文件分组。
// 与「本文件查找」分开：目录查找要遍历文件、要进度、结果可能上万条，
<template>
  <NtModal :title="t('dialog.findInFiles')" width="760" @close="emit('close')">
    <div class="nt-fif">
      <div class="nt-field">
        <span class="nt-label">{{ t('dialog.directory') }}</span>
        <ElInput v-model="dir" size="small" readonly @click="pick" />
        <ElButton size="small" @click="pick">{{ t('common.browse') }}</ElButton>
      </div>
      <div class="nt-field">
        <span class="nt-label">{{ t('dialog.find') }}</span>
        <ElInput v-model="query" size="small" spellcheck="false" @keydown.enter="run" />
      </div>
      <div class="nt-field">
        <span class="nt-label">{{ t('dialog.includePattern') }}</span>
        <ElInput v-model="include" size="small" placeholder="*.ts" spellcheck="false" />
        <span class="nt-label">{{ t('dialog.excludePattern') }}</span>
        <ElInput v-model="exclude" size="small" placeholder="node_modules" spellcheck="false" />
      </div>
      <div class="nt-fif-flags">
        <ElCheckbox v-model="matchCase">{{ t('dialog.matchCase') }}</ElCheckbox>
        <ElCheckbox v-model="regex">{{ t('dialog.regex') }}</ElCheckbox>
        <div class="nt-fif-flags-right">
          <span v-if="running" class="nt-readout">{{ t('toast.scanning', { n: scanned }) }}</span>
          <ElButton v-if="running" size="small" @click="stop">{{ t('dialog.stop') }}</ElButton>
          <ElButton v-else size="small" type="primary" @click="run">{{ t('dialog.search') }}</ElButton>
        </div>
      </div>

      <div class="nt-fif-results">
        <div class="nt-tool-col-head">
          <span>{{ t('dialog.results') }}</span>
          <span class="nt-readout">{{ hits.length }}</span>
        </div>
        <div class="nt-fif-results-body">
          <div v-if="!hits.length && !running" class="nt-empty-sm">{{ t('dialog.searchHint') }}</div>
          <button
            v-for="(h, i) in hits"
            :key="i"
            class="nt-find-result"
            @click="openAt(h)"
          >
            <span class="nt-find-result-path">{{ h.rel }}</span>
            <span class="nt-find-result-line" v-html="highlight(h)" />
          </button>
        </div>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElCheckbox, ElInput } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { api } from '@/api'
import { useFileTreeStore } from '@/stores/fileTree'
import { useEditorStore } from '@/stores/editor'
import { useToastStore } from '@/stores/toast'
import { useFindResultsStore } from '@/stores/findResults'
import { globMatch } from '@/utils'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const tree = useFileTreeStore()
const editor = useEditorStore()
const toast = useToastStore()

const dir = ref(tree.root)
const query = ref('')
const include = ref('*')
const exclude = ref('')
const matchCase = ref(false)
const regex = ref(false)
const running = ref(false)
const scanned = ref(0)
const hits = ref<Array<{ path: string; rel: string; line: string; lineNo: number }>>([])
const findResults = useFindResultsStore()

let cancelled = false

onMounted(() => {
  if (!dir.value) dir.value = ''
})

onBeforeUnmount(() => {
  cancelled = true
})

async function pick() {
  const p = await api.openFolderDialog(dir.value || undefined)
  if (p) dir.value = p
}

/** 停止：协作式取消，正在读的那个文件读完就退。 */
function stop() {
  cancelled = true
  running.value = false
}

/**
 * 遍历 + 搜索。
 *
 * 并发上限 4：全并发会把 IO 打满，而编辑器还要响应用户操作。
 * 每 200 个文件让出一帧，进度数字才更新得了。
 */
async function run() {
  if (!dir.value || !query.value) {
    toast.warn(t('toast.needDirAndKeyword'))
    return
  }
  running.value = true
  cancelled = false
  hits.value = []
  scanned.value = 0

  const re = buildRegex()
  if (!re) {
    running.value = false
    return
  }

  const stack = [dir.value]
  const MAX_RESULTS = 5000

  while (stack.length && !cancelled && hits.value.length < MAX_RESULTS) {
    const cur = stack.pop()!
    let entries
    try {
      entries = await api.readDir(cur)
    } catch {
      continue
    }
    for (const e of entries) {
      if (cancelled) break
      if (e.is_dir) {
        if (!shouldSkipDir(e.name)) stack.push(e.path)
        continue
      }
      if (!shouldInclude(e.name)) continue
      if (e.size > 8 * 1024 * 1024) continue // 超大文件跳过：读它会把界面卡死
      try {
        const c = await api.readFile(e.path)
        const lines = c.text.split('\n')
        for (let i = 0; i < lines.length; i += 1) {
          re.lastIndex = 0
          if (re.test(lines[i])) {
            hits.value.push({
              path: e.path,
              rel: e.path.slice(dir.value.length + 1),
              line: lines[i].slice(0, 200),
              lineNo: i + 1,
            })
            if (hits.value.length >= MAX_RESULTS) break
          }
        }
      } catch {
        /* 读不了的文件跳过（权限 / 二进制） */
      }
      scanned.value += 1
      if (scanned.value % 200 === 0) {
        await new Promise((r) => setTimeout(r, 0))
      }
    }
  }

  running.value = false
  if (hits.value.length >= MAX_RESULTS) {
    toast.warn(t('toast.tooManyResults', { n: MAX_RESULTS }))
  }

  // 同步一份到侧栏面板：浮窗关掉后结果还在，用户能反复点开。
  pushToPanel()
}

/** 把命中按文件归并后推给查找结果面板。 */
function pushToPanel() {
  const byPath = new Map<string, { path: string; count: number; lines: number[]; sample: string }>()
  for (const h of hits.value) {
    const cur = byPath.get(h.path)
    if (cur) {
      cur.count += 1
      if (cur.lines.length < 50) cur.lines.push(h.lineNo)
    } else {
      byPath.set(h.path, {
        path: h.path,
        count: 1,
        lines: [h.lineNo],
        sample: h.line.trim().slice(0, 120),
      })
    }
  }
  const groups = [...byPath.values()]
  findResults.set(
    groups.map((g) => ({ path: g.path, count: g.count, lines: g.lines, sample: g.sample })),
    hits.value.length,
  )
}

function shouldSkipDir(name: string): boolean {
  return name === '.git' || name === 'node_modules' || name === 'target' || name === 'dist'
}

function shouldInclude(name: string): boolean {
  if (include.value && include.value !== '*' && !globMatch(include.value, name)) return false
  if (exclude.value && globMatch(exclude.value, name)) return false
  return true
}

function buildRegex(): RegExp | null {
  const q = query.value
  if (!q) return null
  try {
    const src = regex.value ? q : q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
    return new RegExp(src, matchCase.value ? 'g' : 'gi')
  } catch {
    toast.warn(t('error.regexInvalid', { reason: '' }))
    return null
  }
}

function openAt(h: { path: string; lineNo: number }) {
  void editor.openPath(h.path).then((tab) => {
    if (!tab) return
    document.dispatchEvent(
      new CustomEvent('editor-goto-line', { detail: { tabId: tab.id, line: h.lineNo } }),
    )
  })
  emit('close')
}

function highlight(h: { line: string }): string {
  const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  const idx = h.line.toLowerCase().indexOf(query.value.toLowerCase())
  if (idx < 0 || !query.value) return esc(h.line)
  return (
    esc(h.line.slice(0, idx)) +
    `<mark>${esc(h.line.slice(idx, idx + query.value.length))}</mark>` +
    esc(h.line.slice(idx + query.value.length))
  )
}
</script>

<style scoped>
.nt-fif {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
  height: 70vh;
  min-height: 0;
}

.nt-fif-flags {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-4);
}

.nt-fif-flags-right {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
}

.nt-fif-results {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-2);
  flex: 1;
  min-height: 0;
}

.nt-fif-results-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius-sm);
  background: var(--nt-surface-2);
}

.nt-empty-sm {
  padding: var(--nt-sp-4);
  text-align: center;
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-muted);
}
</style>
