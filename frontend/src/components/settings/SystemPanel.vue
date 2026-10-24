// 设置面板：系统文件关联（文本 / 压缩包两组开关 + 默认状态徽标）。
// Windows 10 起应用不能直接抢默认：这里注册「打开方式」与默认应用候选，
<template>
  <div class="nt-set">
    <section class="nt-set-section">
      <h2 class="nt-set-title">{{ t('settings.systemTitle') }}</h2>
      <p class="nt-sys-hint">{{ t('settings.systemHint') }}</p>

      <div v-for="g in cards" :key="g.id" class="nt-sys-card">
        <div class="nt-sys-card-head">
          <span class="nt-sys-name">{{ t(g.labelKey) }}</span>
          <span v-if="defaultCount(g) > 0" class="nt-sys-default-count">
            {{ t('settings.systemDefaultCount', { n: defaultCount(g) }) }}
          </span>
          <ElSwitch
            :model-value="g.registered"
            :loading="g.busy"
            :disabled="busy || !g.loaded"
            @change="(v) => toggle(g, Boolean(v))"
          />
        </div>
        <div class="nt-sys-exts">
          <span
            v-for="e in g.extensions"
            :key="e.extension"
            class="nt-sys-ext"
            :class="extClass(e)"
          >
            .{{ e.extension }}
            <i class="nt-sys-ext-badge">{{ extBadge(e) }}</i>
          </span>
        </div>
      </div>

      <div class="nt-sys-actions">
        <ElButton size="small" :disabled="busy" @click="openSettings">
          {{ t('settings.systemOpenSettings') }}
        </ElButton>
      </div>
    </section>
  </div>
</template>

<script lang="ts" setup>
import { computed, onMounted, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElSwitch } from 'element-plus'
import { api } from '@/api'
import { useToastStore } from '@/stores/toast'
import type { FileAssocExtVO } from '@/types/api'
import { humanError } from '@/utils/errors'

const { t } = useI18n()
const toast = useToastStore()

interface GroupCard {
  id: 'text' | 'archive'
  labelKey: string
  loaded: boolean
  busy: boolean
  registered: boolean
  extensions: FileAssocExtVO[]
}

const groups = reactive<GroupCard[]>([
  { id: 'text', labelKey: 'settings.systemTextGroup', loaded: false, busy: false, registered: false, extensions: [] },
  { id: 'archive', labelKey: 'settings.systemArchiveGroup', loaded: false, busy: false, registered: false, extensions: [] },
])
const busy = ref(false)

const cards = computed(() => groups)

function fill(vo: Awaited<ReturnType<typeof api.fileAssocStatus>>) {
  for (const g of groups) {
    const src = vo.groups.find((x) => x.group === g.id)
    g.extensions = src?.extensions ?? []
    g.registered = g.extensions.some((e) => e.in_open_with)
    g.loaded = true
  }
}

async function refresh() {
  fill(await api.fileAssocStatus())
}

onMounted(() => {
  void refresh().catch((e) => toast.bad(failText(e)))
})

function defaultCount(g: GroupCard): number {
  return g.extensions.filter((e) => e.is_default).length
}

function extClass(e: FileAssocExtVO): string {
  if (e.is_default) return 'is-default'
  if (e.in_open_with) return 'is-open-with'
  return 'is-none'
}

function extBadge(e: FileAssocExtVO): string {
  if (e.is_default) return t('settings.systemBadgeDefault')
  if (e.in_open_with) return t('settings.systemBadgeOpenWith')
  return t('settings.systemBadgeNone')
}

async function toggle(g: GroupCard, enable: boolean) {
  g.busy = true
  busy.value = true
  try {
    await api.setFileAssocGroup(g.id, enable)
    await refresh()
    if (enable) toast.ok(t('toast.systemAssocOn'))
    else toast.ok(t('toast.systemAssocOff'))
  } catch (e) {
    // 失败回读真实状态，开关停在磁盘上的事实而不是用户的期望
    await refresh()
    toast.bad(failText(e))
  } finally {
    g.busy = false
    busy.value = false
  }
}

async function openSettings() {
  try {
    await api.openDefaultAppsSettings()
  } catch (e) {
    toast.bad(failText(e))
  }
}

/** 失败提示：humanError 已按码本地化并带下一步动作。 */
function failText(e: unknown): string {
  const { message, step } = humanError(e)
  return step ? `${message}（${step}）` : message
}
</script>

<style scoped>
.nt-set {
  max-width: 640px;
}

.nt-set-section {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
}

.nt-set-title {
  font-size: var(--nt-fs-md);
  font-weight: 600;
  color: var(--nt-ink);
  padding-bottom: var(--nt-sp-2);
  border-bottom: 1px solid var(--nt-border);
}

.nt-sys-hint {
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-2);
  line-height: 1.6;
}

.nt-sys-card {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-2);
  padding: var(--nt-sp-3);
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius-md);
  background: var(--nt-elev-surface);
}

.nt-sys-card-head {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
}

.nt-sys-name {
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink);
  font-weight: 500;
}

/* 只读读数：换字色不换几何（AGENTS §2.10.1） */
.nt-sys-default-count {
  flex: 1;
  font-size: var(--nt-fs-hint);
  color: var(--nt-primary);
}

.nt-sys-exts {
  display: flex;
  flex-wrap: wrap;
  gap: var(--nt-sp-2);
}

.nt-sys-ext {
  display: inline-flex;
  align-items: center;
  gap: var(--nt-sp-1);
  height: var(--nt-ctl-h-sm);
  padding: 0 var(--nt-sp-2);
  border-radius: var(--nt-radius-sm);
  border: 1px solid var(--nt-border);
  background: var(--nt-elev-raised);
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-2);
  font-family: var(--nt-font-editor);
}

.nt-sys-ext-badge {
  font-style: normal;
  font-size: var(--nt-fs-hint);
  transition: color var(--nt-dur-fast) var(--nt-ease);
}

.is-open-with .nt-sys-ext-badge {
  color: var(--nt-ink-3, var(--nt-ink-2));
}

/* 默认态只抬字色：徽标不是按钮，不能做出可点的样子 */
.is-default {
  border-color: var(--nt-border);
}

.is-default .nt-sys-ext-badge {
  color: var(--nt-primary);
  font-weight: 500;
}

.nt-sys-actions {
  display: flex;
  gap: var(--nt-sp-3);
}
</style>
