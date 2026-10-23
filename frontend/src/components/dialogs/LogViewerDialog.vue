// 日志查看器浮窗：按级别着色 + 过滤。
// 场景：用户报告「应用出了问题」，第一句话总是「日志在哪」。

<template>
  <NtModal :title="t('viewer.logViewer')" width="860" @close="dialogs.close()">
    <div class="nt-log">
      <div class="nt-log-bar">
        <ElRadioGroup v-model="filter" size="small">
          <ElRadioButton value="all">{{ t('viewer.logAll') }}</ElRadioButton>
          <ElRadioButton value="error">{{ t('viewer.logErrors') }}</ElRadioButton>
          <ElRadioButton value="warn">{{ t('viewer.logWarns') }}</ElRadioButton>
        </ElRadioGroup>
        <div class="nt-log-spacer" />
        <ElButton size="small" @click="openLogDir">{{ t('settings.openDir') }}</ElButton>
        <ElButton size="small" :loading="loading" @click="load">{{ t('common.refresh') }}</ElButton>
      </div>

      <div class="nt-log-body">
        <div v-if="!lines.length && !loading" class="nt-empty-sm">{{ t('viewer.logEmpty') }}</div>
        <div v-for="(l, i) in lines" :key="i" class="nt-log-line" :class="`is-${l.level}`">
          <span class="nt-log-time">{{ l.time }}</span>
          <span class="nt-log-level">{{ l.level.toUpperCase() }}</span>
          <span class="nt-log-text">{{ l.text }}</span>
        </div>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElRadioButton, ElRadioGroup } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { useDialogStore } from '@/stores/dialogs'
import { useToastStore } from '@/stores/toast'
import { api, normalizeError } from '@/api'

const { t } = useI18n()
const dialogs = useDialogStore()
const toast = useToastStore()

type Level = 'error' | 'warn' | 'info'
interface LogLine {
  time: string
  level: Level
  text: string
}

/** 上限：日志可能几十 MB，全渲染会把界面卡死。只取最后 N 行。 */
const MAX_LINES = 2000

const all = ref<LogLine[]>([])
const loading = ref(false)
const filter = ref<'all' | 'error' | 'warn'>('all')

const lines = computed(() => {
  if (filter.value === 'all') return all.value
  return all.value.filter((l) => l.level === filter.value)
})

onMounted(() => void load())

/** 读 app.log：后端落盘格式是 `LEVEL 时间 target 消息`，按行解析。 */
async function load() {
  loading.value = true
  try {
    const info = await api.appInfo()
    const sep = info.log_dir.includes('\\') ? '\\' : '/'
    const path = `${info.log_dir}${sep}app.log`
    const b64 = await api.readBinary(path)
    const text = atob(b64)
    const parsed = parseLog(text)
    // 只要最后 MAX_LINES 行：最新的问题在末尾。
    all.value = parsed.slice(-MAX_LINES)
  } catch (e) {
    // 日志读不到不当错误弹：可能是首次启动还没写日志。
    toast.warn(normalizeError(e).message)
  } finally {
    loading.value = false
  }
}

/**
 * 解析 tracing 文本格式。
 *
 * 形如 `2026-10-22T14:30:01.123Z ERROR notetext::file: 写文件失败`。
 * 解析不出来的行按 info 收进去：丢掉一整行比把它标错级别更糟
 * （用户要找的就是那一行）。
 */
function parseLog(text: string): LogLine[] {
  const out: LogLine[] = []
  for (const raw of text.split('\n')) {
    const line = raw.trimEnd()
    if (!line) continue
    const m = /^(\S+)\s+(ERROR|WARN|INFO|DEBUG|TRACE)\s+(\S+)\s*(.*)$/.exec(line)
    if (m) {
      const lvl = m[2].toLowerCase()
      out.push({
        time: m[1].slice(11, 19),
        level: (lvl === 'error' || lvl === 'warn' ? lvl : 'info') as Level,
        text: m[4],
      })
    } else {
      out.push({ time: '', level: 'info', text: line })
    }
  }
  return out
}

function openLogDir() {
  void api.appInfo().then((info) => void api.openPath(info.log_dir))
}
</script>

<style scoped>
.nt-log {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
  min-height: 0;
}

.nt-log-bar {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  flex: none;
}

.nt-log-spacer {
  flex: 1;
}

.nt-log-body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  font-family: var(--font-mono);
  font-size: var(--nt-fs-sm);
  line-height: 1.6;
}

.nt-log-line {
  display: flex;
  gap: var(--nt-sp-2);
  padding: 1px var(--nt-sp-2);
  border-radius: var(--nt-radius-xs);
  white-space: pre-wrap;
  word-break: break-all;
}

.nt-log-line:hover {
  background: var(--nt-surface-hover);
}

.nt-log-time {
  flex: none;
  color: var(--nt-ink-muted);
  font-variant-numeric: tabular-nums;
}

.nt-log-level {
  flex: none;
  width: 46px;
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-muted);
}

.nt-log-line.is-error .nt-log-level {
  color: var(--nt-danger);
}

.nt-log-line.is-error .nt-log-text {
  color: var(--nt-danger);
}

.nt-log-line.is-warn .nt-log-level {
  color: var(--nt-warning);
}
</style>
