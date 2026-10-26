// 宏面板：列出已录制的宏，点一下回放到当前光标处。
// 宏是「一串编辑动作」，回放必须驱动编辑器命令，所以这里只取视图不自己改正文。
:<template>
  <div class="nt-panel">
    <div class="nt-panel-head">
      <span>{{ t('sidebar.macros') }}</span>
      <button class="nt-icon-btn" :title="t('menu.macroRecord')" @click="commands.run('macro.record')">
        <NtIcon name="plus" size="sm" />
      </button>
    </div>

    <div v-if="items.length === 0" class="nt-empty">
      <p>{{ t('sidebar.noMacros') }}</p>
      <p class="nt-empty-hint">{{ t('sidebar.macroHint') }}</p>
    </div>

    <div v-else class="nt-panel-list">
      <div v-for="m in items" :key="m.id" class="nt-macro-row">
        <button
          class="nt-macro-main"
          :title="t('menu.macroPlay')"
          :disabled="!hasEditor"
          @click="play(m)"
        >
          <NtIcon name="play" size="sm" class="nt-macro-icon" />
          <span class="nt-macro-name">{{ m.name }}</span>
          <span class="nt-macro-meta">{{ t('sidebar.macroSteps', { n: m.steps.length }) }}</span>
        </button>
        <button class="nt-icon-btn" :title="t('common.rename')" @click="rename(m)">
          <NtIcon name="braces" size="sm" />
        </button>
        <button class="nt-icon-btn" :title="t('common.delete')" @click="remove(m)">
          <NtIcon name="trash" size="sm" />
        </button>
      </div>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import NtIcon from '@/components/common/NtIcon.vue'
import { api, normalizeError } from '@/api'
import { useEditorStore } from '@/stores/editor'
import { useCommandStore } from '@/stores/commands'
import { useToastStore } from '@/stores/toast'
import { get } from '@/composables/useEditorRegistry'
import { playSteps } from '@/composables/useEditorMacro'
import { askText } from '@/components/common/askText'
import type { MacroVO } from '@/types/api'

const { t } = useI18n()
const editor = useEditorStore()
const commands = useCommandStore()
const toast = useToastStore()

const items = ref<MacroVO[]>([])
/** 没有视图就没法回放：按钮置灰，而不是点了什么也不发生。 */
const hasEditor = computed(() => editor.activeId != null && !!get(editor.activeId))

async function load() {
  try {
    items.value = (await api.listMacros()) ?? []
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}

async function play(m: MacroVO) {
  const id = editor.activeId
  const view = id != null ? get(id) : undefined
  if (!view) {
    toast.warn(t('tool.clipNoEditor'))
    return
  }
  await playSteps(view, m.steps, (cmdId) => commands.run(cmdId))
  toast.ok(t('toast.macroPlayed', { name: m.name }))
}

async function rename(m: MacroVO) {
  const name = await askText({ title: t('common.rename'), value: m.name, placeholder: m.name })
  const next = name?.trim()
  if (!next || next === m.name) return
  try {
    await api.saveMacro({ id: m.id, name: next, steps: m.steps })
    await load()
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}

async function remove(m: MacroVO) {
  const ok = await api.showConfirm(t('common.delete'), t('sidebar.macroDeleteBody', { name: m.name }))
  if (!ok) return
  try {
    await api.removeMacro(m.id)
    await load()
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}

/** 录制结束会派发 macros-changed：面板跟着重查，新宏立刻可见。 */
function onChanged() {
  void load()
}
onMounted(() => {
  void load()
  document.addEventListener('macros-changed', onChanged)
})
onBeforeUnmount(() => document.removeEventListener('macros-changed', onChanged))
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

.nt-macro-row {
  display: flex;
  align-items: center;
  gap: 2px;
  border-radius: var(--nt-radius-xs);
  transition: background-color var(--nt-dur-fast) var(--nt-ease);
}

.nt-macro-row:hover {
  background: var(--nt-surface-hover);
}

/* 行主体是一颗按钮：点哪都能回放，右侧两个图标只做管理动作 */
.nt-macro-main {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  flex: 1;
  min-width: 0;
  padding: var(--nt-sp-1) var(--nt-sp-2);
  text-align: left;
  color: var(--nt-ink-2);
  background: transparent;
  border: 0;
  cursor: pointer;
}

.nt-macro-main:disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

.nt-macro-icon {
  flex: none;
  color: var(--nt-primary);
}

.nt-macro-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink);
}

.nt-macro-meta {
  flex: none;
  margin-left: auto;
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-muted);
  font-variant-numeric: tabular-nums;
}

.nt-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--nt-sp-2);
  padding: var(--nt-sp-6) var(--nt-sp-3);
  text-align: center;
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-muted);
}

.nt-empty-hint {
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-muted);
}
</style>
