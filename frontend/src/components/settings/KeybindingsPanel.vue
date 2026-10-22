// 设置面板：快捷键（查看默认键位 + 自定义覆盖）。
//
// 覆盖写进 config.keybindings（后端 settings KV），不进 Rust 侧——
// 键位是纯界面关注点，后端不该知道。
<template>
  <div class="nt-set">
    <section class="nt-set-section">
      <h2 class="nt-set-title">{{ t('settings.keybindings') }}</h2>
      <p class="nt-set-hint">{{ t('settings.keybindingHint') }}</p>

      <div class="nt-keys">
        <div v-for="cmd in rows" :key="cmd.id" class="nt-key-row">
          <span class="nt-key-label">{{ cmd.label }}</span>
          <button
            class="nt-key-btn"
            :class="{ 'is-editing': editing === cmd.id }"
            @click="startEdit(cmd.id)"
          >
            {{ editing === cmd.id ? t('settings.pressKey') : keyOf(cmd.id) }}
          </button>
          <ElButton
            v-if="settings.config.keybindings[cmd.id]"
            size="small"
            text
            @click="clearOverride(cmd.id)"
          >
            {{ t('common.clear') }}
          </ElButton>
        </div>
      </div>
    </section>
  </div>
</template>

<script lang="ts" setup>
import { onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton } from 'element-plus'
import { DEFAULT_KEYBINDINGS, useCommandStore } from '@/stores/commands'
import { useSettingsStore } from '@/stores/settings'
import { toAccelerator } from '@/composables/useGlobalKeys'
import { useToastStore } from '@/stores/toast'

const { t, te } = useI18n()
const commands = useCommandStore()
const settings = useSettingsStore()
const toast = useToastStore()

const editing = ref<string | null>(null)

/** 有默认键位的命令才列出来（免得列表里一半是空的）。 */
const rows = ref<Array<{ id: string; label: string }>>([])

onMounted(() => {
  rows.value = [...commands.registry.values()]
    .filter((c) => DEFAULT_KEYBINDINGS[c.id])
    .map((c) => ({
      id: c.id,
      label: te(c.labelKey) ? t(c.labelKey) : c.id,
    }))
    .sort((a, b) => a.label.localeCompare(b.label))
})

function keyOf(id: string): string {
  const accel = settings.config.keybindings[id] ?? DEFAULT_KEYBINDINGS[id]
  if (!accel) return '—'
  return accel.replace('Control', 'Ctrl').replace('+', ' + ')
}

function startEdit(id: string) {
  editing.value = id
}

function clearOverride(id: string) {
  const next = { ...settings.config.keybindings }
  delete next[id]
  settings.update({ keybindings: next })
}

/** 录制：捕获下一次按键作为新键位。 */
function onKey(e: KeyboardEvent) {
  if (!editing.value) return
  e.preventDefault()
  e.stopPropagation()
  if (e.key === 'Escape') {
    editing.value = null
    return
  }
  const accel = toAccelerator(e)

  // 冲突检测：同一组合键已分配给别的命令就拒绝，否则用户会「改了没反应」。
  const conflict = Object.entries(settings.config.keybindings).find(
    ([k, v]) => v === accel && k !== editing.value,
  )
  const defaultOwner = Object.entries(DEFAULT_KEYBINDINGS).find(
    ([k, v]) => v === accel && k !== editing.value,
  )
  if (conflict || defaultOwner) {
    const owner = (conflict ?? defaultOwner)![0]
    const label = rows.value.find((r) => r.id === owner)?.label ?? owner
    toast.warn(t('settings.keybindingConflict', { name: label }))
    return
  }

  settings.update({
    keybindings: { ...settings.config.keybindings, [editing.value!]: accel },
  })
  editing.value = null
}

onMounted(() => document.addEventListener('keydown', onKey, { capture: true }))
onUnmounted(() => document.removeEventListener('keydown', onKey, { capture: true }))
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

.nt-set-hint {
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-muted);
}

.nt-keys {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.nt-key-row {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
}

.nt-key-label {
  flex: 1;
  min-width: 0;
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink);
}

.nt-key-btn {
  min-width: 160px;
  height: var(--nt-ctl-h-sm);
  padding: 0 var(--nt-sp-3);
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius-sm);
  background: var(--nt-surface-2);
  font-family: var(--font-mono);
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-2);
  text-align: center;
  transition:
    border-color var(--nt-dur-fast) var(--nt-ease),
    background-color var(--nt-dur-fast) var(--nt-ease),
    color var(--nt-dur-fast) var(--nt-ease),
    transform var(--nt-dur-fast) var(--nt-ease);
}

.nt-key-btn:hover {
  border-color: var(--nt-control-border-hover);
  color: var(--nt-ink);
}

.nt-key-btn:active {
  background: var(--nt-surface-selected);
  transform: scale(0.97);
}

.nt-key-btn.is-editing {
  border-color: var(--nt-primary);
  color: var(--nt-primary);
  box-shadow: var(--nt-control-shadow-focus);
}
</style>
