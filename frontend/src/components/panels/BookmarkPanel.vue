// 书签面板：按文件分组的书签列表，点击跳到该行。
// 后端已把「该行正文」一起返回（BookmarkVO.line_text），
<template>
  <div class="nt-panel">
    <div class="nt-panel-head">
      <span>{{ t('sidebar.bookmarks') }}</span>
      <button class="nt-icon-btn" :title="t('common.clear')" @click="onClearAll">
        <NtIcon name="trash" />
      </button>
    </div>
    <div v-if="items.length === 0" class="nt-empty">
      <p>{{ t('sidebar.noBookmarks') }}</p>
      <p class="nt-empty-hint">{{ t('sidebar.bookmarkHint') }}</p>
    </div>
    <div v-else class="nt-panel-list">
      <button
        v-for="b in items"
        :key="b.id"
        class="nt-row-item nt-bookmark-row"
        @click="onJump(b)"
        @contextmenu.prevent="onMenu($event, b)"
      >
        <NtIcon size="sm" name="bookmark" class="nt-bookmark-icon" />
        <span class="nt-bookmark-main">
          <span class="nt-bookmark-file">{{ b.file_name }}:{{ b.line }}</span>
          <span class="nt-bookmark-text">{{ b.label || b.line_text }}</span>
        </span>
      </button>
    </div>

    <Teleport to="body">
      <div v-if="menu" class="nt-ctx-menu" :style="{ left: `${menu.x}px`, top: `${menu.y}px` }" @click="menu = null">
        <button class="nt-ctx-item" @click="onRename">{{ t('common.rename') }}</button>
        <button class="nt-ctx-item" @click="onClearFile">{{ t('common.clear') }}</button>
        <button class="nt-ctx-item" @click="onRemove">{{ t('common.delete') }}</button>
      </div>
    </Teleport>
  </div>
</template>

<script lang="ts" setup>
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import NtIcon from '@/components/common/NtIcon.vue'
import { api, normalizeError } from '@/api'
import { useEditorStore } from '@/stores/editor'
import { useToastStore } from '@/stores/toast'
import { askText } from '@/components/common/askText'
import type { BookmarkVO } from '@/types/api'

const { t } = useI18n()
const editor = useEditorStore()
const toast = useToastStore()

const items = ref<BookmarkVO[]>([])
const menu = ref<{ x: number; y: number } | null>(null)
const menuItem = ref<BookmarkVO | null>(null)

async function load() {
  try {
    items.value = await api.listBookmarks()
  } catch (e) {
    toast.bad(`${t('error.bookmarkFailed', { reason: normalizeError(e).message })}`)
  }
}

onMounted(load)

/** 跳到书签：文件没打开就先打开，再滚到该行。 */
async function onJump(b: BookmarkVO) {
  let tab = editor.findByPath(b.path)
  if (!tab) tab = (await editor.openPath(b.path)) ?? undefined
  if (!tab) return
  editor.activate(tab.id)
  // 由编辑器组件监听这个事件完成滚动与选中。
  document.dispatchEvent(
    new CustomEvent('editor-goto-line', { detail: { tabId: tab.id, line: b.line } }),
  )
}

function onMenu(e: MouseEvent, b: BookmarkVO) {
  menuItem.value = b
  menu.value = { x: e.clientX, y: e.clientY }
}

async function onRemove() {
  const b = menuItem.value
  menu.value = null
  if (!b) return
  await api.removeBookmark(b.id)
  await load()
}

async function onClearFile() {
  const b = menuItem.value
  menu.value = null
  if (!b) return
  await api.clearBookmarks(b.path)
  await load()
}

async function onClearAll() {
  await api.clearBookmarks()
  await load()
}

/**
 * 改别名：`add_bookmark` 对 (path, line) 是 upsert，
 * 带上同一个行号再写一次 label 就是「重命名」，不需要额外命令。
 */
async function onRename() {
  const b = menuItem.value
  menu.value = null
  if (!b) return
  const label = await askText({
    title: t('common.rename'),
    value: b.label,
    placeholder: b.line_text,
  })
  if (label == null) return
  try {
    await api.addBookmark({ path: b.path, line: b.line, label: label.trim() })
    await load()
    document.dispatchEvent(new CustomEvent('bookmarks-changed'))
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}

// 其他面板改完书签后广播刷新。挂载时登记、卸载时摘掉：否则来回切页签会越积越多。
function onChanged() {
  void load()
}
onMounted(() => document.addEventListener('bookmarks-changed', onChanged))
onBeforeUnmount(() => document.removeEventListener('bookmarks-changed', onChanged))
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

.nt-bookmark-row {
  gap: var(--nt-sp-2);
  padding: var(--nt-sp-1) var(--nt-sp-2);
  width: 100%;
  text-align: left;
}

.nt-bookmark-icon {
  color: var(--nt-warning);
  flex: none;
}

.nt-bookmark-main {
  display: flex;
  flex-direction: column;
  min-width: 0;
  flex: 1;
}

.nt-bookmark-file {
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink);
  font-weight: 500;
}

.nt-bookmark-text {
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-muted);
  font-family: var(--font-mono);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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
