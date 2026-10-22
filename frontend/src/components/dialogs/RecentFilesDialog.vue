// 最近文件浮窗：后端一直有记录，之前没有入口。
//
// 只做三件事：列出来、点开、钉住。删除与清空放在行内右键——
// 最近文件是「顺手的入口」，不该让用户为它学一套编辑操作。
<template>
  <NtModal :title="t('menu.recentFiles')" width="640" @close="emit('close')">
    <div v-if="loading" class="nt-empty">
      <span class="nt-spinner" />
      <span>{{ t('common.loading') }}</span>
    </div>

    <div v-else-if="items.length === 0" class="nt-empty">
      <NtIcon name="clock" size="lg" class="nt-empty-icon" />
      <p>{{ t('welcome.noRecent') }}</p>
    </div>

    <div v-else class="nt-recent-list">
      <button
        v-for="f in items"
        :key="f.path"
        class="nt-row-item nt-recent-row"
        :class="{ 'is-disabled': !f.exists }"
        :title="f.path"
        @click="onOpen(f)"
        @contextmenu.prevent="onMenu($event, f)"
      >
        <NtIcon :name="f.pinned ? 'pin' : 'file'" size="sm" class="nt-recent-icon" />
        <span class="nt-recent-main">
          <span class="nt-recent-name">{{ f.file_name }}</span>
          <span class="nt-recent-dir">{{ ellipsisMiddle(f.dir, 60) }}</span>
        </span>
        <span class="nt-recent-time nt-readout">{{ formatTime(f.opened_at) }}</span>
      </button>
    </div>

    <template #footer>
      <ElButton size="small" :disabled="items.length === 0" @click="onClear">
        {{ t('menu.clearRecent') }}
      </ElButton>
      <ElButton size="small" type="primary" @click="emit('close')">
        {{ t('common.close') }}
      </ElButton>
    </template>
  </NtModal>

  <Teleport to="body">
    <div
      v-if="menu"
      class="nt-ctx-menu"
      :style="{ left: `${menu.x}px`, top: `${menu.y}px` }"
      @click="menu = null"
    >
      <button class="nt-ctx-item" @click="onPin">{{ t('menu.pinRecent') }}</button>
      <button class="nt-ctx-item" @click="onReveal">{{ t('tab.revealInExplorer') }}</button>
      <div class="nt-menu-sep" />
      <button class="nt-ctx-item" @click="onRemove">{{ t('common.removeFromList') }}</button>
    </div>
  </Teleport>
</template>

<script lang="ts" setup>
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import NtIcon from '@/components/common/NtIcon.vue'
import { api, normalizeError } from '@/api'
import { useEditorStore } from '@/stores/editor'
import { useToastStore } from '@/stores/toast'
import { dirName, ellipsisMiddle, formatTime } from '@/utils'
import type { RecentFileVO } from '@/types/api'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const editor = useEditorStore()
const toast = useToastStore()

const items = ref<RecentFileVO[]>([])
const loading = ref(true)
const menu = ref<{ x: number; y: number } | null>(null)
const menuItem = ref<RecentFileVO | null>(null)

async function load() {
  loading.value = true
  try {
    // 空数组兜底：后端不给 null，但前端按「可能是 null」接（AGENTS.md §2.3.2）。
    items.value = (await api.recentFiles()) ?? []
  } catch (e) {
    toast.bad(normalizeError(e).message)
  } finally {
    loading.value = false
  }
}

onMounted(load)

async function onOpen(f: RecentFileVO) {
  if (!f.exists) {
    toast.warn(t('sidebar.fileMissing', { name: f.file_name }))
    return
  }
  emit('close')
  await editor.openPath(f.path)
}

function onMenu(e: MouseEvent, f: RecentFileVO) {
  menuItem.value = f
  menu.value = { x: e.clientX, y: e.clientY }
}

async function onPin() {
  const f = menuItem.value
  menu.value = null
  if (!f) return
  await api.setRecentPinned(f.path, !f.pinned)
  await load()
}

function onReveal() {
  const f = menuItem.value
  menu.value = null
  if (!f) return
  void api.openPath(dirName(f.path))
}

async function onRemove() {
  const f = menuItem.value
  menu.value = null
  if (!f) return
  await api.removeRecent(f.path)
  await load()
}

async function onClear() {
  const n = await api.clearRecent()
  toast.ok(t('sidebar.recentCleared', { n }))
  await load()
}
</script>

<style scoped>
.nt-recent-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 52vh;
  overflow-y: auto;
}

.nt-recent-row {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
  width: 100%;
  padding: var(--nt-sp-2) var(--nt-sp-3);
  border-radius: var(--nt-radius-sm);
  text-align: left;
}

.nt-recent-icon {
  color: var(--nt-ink-muted);
}

.nt-recent-main {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
  flex: 1;
}

.nt-recent-name {
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.nt-recent-dir {
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.nt-recent-time {
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-muted);
  flex: none;
}

.nt-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--nt-sp-3);
  padding: var(--nt-sp-10) var(--nt-sp-4);
  color: var(--nt-ink-muted);
  font-size: var(--nt-fs-sm);
}

.nt-empty-icon {
  color: var(--nt-ink-muted);
}

.nt-spinner {
  width: 16px;
  height: 16px;
  border: 2px solid var(--nt-border-strong);
  border-top-color: var(--nt-primary);
  border-radius: var(--nt-radius-full);
  animation: nt-spin 720ms linear infinite;
}
</style>
