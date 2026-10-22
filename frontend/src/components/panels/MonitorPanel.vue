// 文件监控面板：tail -f 当前文件，外部追加即时显示。
//
// 轮询而不是 notify：notify 的事件不含「追加了多少行」，而 tail -f
// 只要增量。每 800ms 读一次行区间，比全量重读省一个量级。
<template>
  <div class="nt-panel">
    <div class="nt-panel-head">
      <span>{{ t('sidebar.monitor') }}</span>
      <button
        class="nt-icon-btn"
        :class="{ 'is-on': watching }"
        :title="watching ? t('dialog.stop') : t('sidebar.monitor')"
        @click="toggleWatch"
      >
        <NtIcon :name="watching ? 'square' : 'activity'" />
      </button>
    </div>

    <div v-if="!editor.active?.path" class="nt-empty">
      <p>{{ t('sidebar.notMonitoring') }}</p>
      <p class="nt-empty-hint">{{ t('sidebar.monitorHint') }}</p>
    </div>
    <div v-else class="nt-monitor">
      <div class="nt-monitor-head">
        <span class="nt-monitor-file" :title="editor.active.path">
          {{ editor.active.title }}
        </span>
        <span class="nt-readout">{{ t('monitor.lineCount', { n: tail.length }) }}</span>
      </div>
      <div ref="scroll" class="nt-monitor-body">
        <div
          v-for="(l, i) in tail"
          :key="`${startLine + i}`"
          class="nt-log-line"
          :class="levelClass(l)"
        >
          {{ l }}
        </div>
      </div>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import NtIcon from '@/components/common/NtIcon.vue'
import { api, normalizeError } from '@/api'
import { useEditorStore } from '@/stores/editor'
import { useToastStore } from '@/stores/toast'

const { t } = useI18n()
const editor = useEditorStore()
const toast = useToastStore()

/** 保留的尾部行数：再多 DOM 会拖慢滚动。 */
const MAX_LINES = 2000
const POLL_MS = 800

const tail = ref<string[]>([])
const startLine = ref(1)
const watching = ref(false)
const scroll = ref<HTMLElement | null>(null)
let timer: number | null = null

const path = computed(() => editor.active?.path ?? '')

function levelClass(line: string): string {
  if (/\b(ERROR|FATAL|ERR)\b/i.test(line)) return 'is-error'
  if (/\b(WARN|WARNING)\b/i.test(line)) return 'is-warn'
  return ''
}

/** 读一段行区间：从上次读到的位置接着读，只取增量。 */
async function poll() {
  const p = path.value
  if (!p || !watching.value) return
  try {
    const from = startLine.value
    const c = await api.readFile(p, { mode: 'range', start_line: from, lines: 500 })
    const lines = c.text ? c.text.replace(/\n$/, '').split('\n') : []
    if (lines.length) {
      tail.value = [...tail.value, ...lines].slice(-MAX_LINES)
      startLine.value = from + lines.length
      // 贴底：用户在看最新输出，不该被新行顶回上面。
      requestAnimationFrame(() => {
        if (scroll.value) scroll.value.scrollTop = scroll.value.scrollHeight
      })
    }
  } catch (e) {
    toast.bad(`${t('error.readFailed', { reason: normalizeError(e).message })}`)
    stop()
  }
}

function start() {
  if (!path.value) {
    toast.warn(t('sidebar.monitorHint'))
    return
  }
  watching.value = true
  tail.value = []
  startLine.value = 1
  void poll()
  timer = window.setInterval(() => void poll(), POLL_MS)
}

function stop() {
  watching.value = false
  if (timer !== null) {
    window.clearInterval(timer)
    timer = null
  }
}

function toggleWatch() {
  if (watching.value) stop()
  else start()
}

onMounted(() => {
  // 面板一打开就开始：用户点它就是想看日志。
  if (path.value) start()
})

onBeforeUnmount(stop)

// 切换标签：监控目标跟着变，从头读新文件。
watch(path, () => {
  if (watching.value) start()
})
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

.nt-empty {
  padding: var(--nt-sp-6) var(--nt-sp-3);
  text-align: center;
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-muted);
}

.nt-empty-hint {
  margin-top: var(--nt-sp-2);
  font-size: var(--nt-fs-hint);
}

.nt-monitor {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

.nt-monitor-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--nt-sp-2);
  padding: var(--nt-sp-1) var(--nt-sp-3);
  border-bottom: 1px solid var(--nt-border);
  font-size: var(--nt-fs-hint);
  flex: none;
}

.nt-monitor-file {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--nt-ink-2);
}

.nt-monitor-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: var(--nt-sp-1) 0;
}
</style>
