// 菜单栏：一级菜单 + 下拉。
//
// 数据来自命令表（stores/commands），不在这里写死功能——
// 新增命令只改命令注册处，菜单自动长出来。
<template>
  <div class="nt-menubar">
    <span class="nt-menubar-brand">NoteText</span>
    <div
      v-for="group in GROUPS"
      :key="group.id"
      class="nt-menu-wrap"
      @mouseenter="onHover(group.id)"
    >
      <button
        class="nt-menu-item"
        :class="{ 'is-open': open === group.id }"
        @click="open = open === group.id ? null : group.id"
      >
        {{ t(`menu.${group.id}`) }}
      </button>
      <Teleport to="body">
        <div
          v-if="open === group.id"
          class="nt-menu-dropdown"
          :style="dropdownStyle"
          @click="open = null"
        >
          <template v-for="(item, i) in group.items" :key="item.id">
            <div v-if="item.sep" :key="`sep-${i}`" class="nt-menu-sep" />
            <button
              v-else
              class="nt-menu-entry"
              :disabled="item.enabled && !item.enabled()"
              @click="commands.run(item.id)"
            >
              <span class="nt-menu-entry-label">{{ t(item.labelKey) }}</span>
              <span class="nt-menu-entry-key">{{ keyOf(item) }}</span>
            </button>
          </template>
        </div>
      </Teleport>
    </div>
    <div class="nt-menubar-spacer" />
    <button class="nt-menu-item" @click="router.push('/help')">{{ t('menu.help') }}</button>
  </div>
</template>

<script lang="ts" setup>
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useCommandStore, DEFAULT_KEYBINDINGS, type Command } from '@/stores/commands'
import { useSettingsStore } from '@/stores/settings'

const { t } = useI18n()
const router = useRouter()
const commands = useCommandStore()
const settings = useSettingsStore()

const open = ref<string | null>(null)
const anchorX = ref(0)

/** 菜单分组与顺序：与工具栏 / 快捷键的分组一致。 */
const GROUPS = computed(() => {
  const byGroup = (g: Command['group']) =>
    commands.byGroup(g).map((c) => ({ ...c, sep: false as const }))
  return [
    { id: 'file', items: byGroup('file') },
    { id: 'edit', items: byGroup('edit') },
    { id: 'view', items: byGroup('view') },
    { id: 'tools', items: byGroup('tools') },
  ]
})

/** 下拉定位：贴在触发按钮下方。用 Teleport 到 body 才能越过编辑器的
 *  overflow 容器，所以坐标要按视口算。 */
const dropdownStyle = computed(() => ({
  left: `${anchorX.value}px`,
  top: `calc(var(--nt-h-menu) + 4px)`,
}))

function onHover(id: string) {
  // 已打开时滑过即切换：桌面菜单的常规行为。
  if (open.value !== null && open.value !== id) {
    open.value = id
    const el = document.querySelector(`.nt-menu-wrap:nth-child(${index(id)})`)
    anchorX.value = el?.getBoundingClientRect().left ?? 0
  }
}

function index(id: string) {
  return GROUPS.value.findIndex((g) => g.id === id) + 2
}

function keyOf(cmd: { id: string }): string {
  const override = settings.config.keybindings[cmd.id]
  const accel = override ?? DEFAULT_KEYBINDINGS[cmd.id]
  if (!accel) return ''
  return accel.replace('Control', 'Ctrl').replace('+', ' + ')
}

function onDocClick() {
  open.value = null
}

onMounted(() => document.addEventListener('click', onDocClick))
onUnmounted(() => document.removeEventListener('click', onDocClick))
</script>

<style scoped>
.nt-menubar-brand {
  font-size: var(--nt-fs-label);
  font-weight: 600;
  color: var(--nt-ink);
  margin-right: var(--nt-sp-2);
  padding-left: var(--nt-sp-2);
}

.nt-menubar-spacer {
  flex: 1;
}

.nt-menu-wrap {
  position: relative;
  display: inline-flex;
}

.nt-menu-dropdown {
  position: fixed;
  z-index: var(--nt-z-pop);
  min-width: 220px;
  padding: var(--nt-sp-1);
  background: var(--nt-surface);
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius);
  box-shadow: var(--nt-card-shadow);
}

.nt-menu-entry {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--nt-sp-6);
  width: 100%;
  height: calc(var(--nt-ctl-h) - 4px);
  padding: 0 var(--nt-sp-2);
  border-radius: var(--nt-radius-xs);
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink);
  text-align: left;
}

.nt-menu-entry:hover:not(:disabled) {
  background: var(--nt-surface-selected);
  color: var(--nt-primary);
}

.nt-menu-entry:disabled {
  color: var(--nt-ink-muted);
  cursor: default;
}

.nt-menu-entry-key {
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-muted);
  flex: none;
}

.nt-menu-sep {
  height: 1px;
  margin: var(--nt-sp-1) var(--nt-sp-2);
  background: var(--nt-border);
}
</style>
