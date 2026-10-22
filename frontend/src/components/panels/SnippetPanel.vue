// 代码片段面板：按语言筛选，双击插入到光标处。
//
// 插入走编辑器命令（不直接操作 DOM）：命令层才知道光标在哪。
<template>
  <div class="nt-panel">
    <div class="nt-panel-head">
      <span>{{ t('sidebar.snippets') }}</span>
      <button class="nt-icon-btn" :title="t('common.search')" @click="showFilter = !showFilter">
        <NtIcon name="search" size="sm" />
      </button>
    </div>

    <div v-if="showFilter" class="nt-panel-filter">
      <ElInput v-model="keyword" size="small" :placeholder="t('common.search')" clearable />
      <ElSelect v-model="lang" size="small" clearable :placeholder="t('common.all')">
        <ElOption v-for="l in langs" :key="l" :label="l" :value="l" />
      </ElSelect>
    </div>

    <div v-if="items.length === 0" class="nt-empty">
      <p>{{ t('sidebar.noSnippets') }}</p>
      <p class="nt-empty-hint">{{ t('sidebar.snippetHint') }}</p>
    </div>
    <div v-else class="nt-panel-list">
      <button
        v-for="s in items"
        :key="s.id"
        class="nt-row-item nt-snippet-row"
        @dblclick="onInsert(s)"
        @contextmenu.prevent="onMenu($event, s)"
      >
        <span class="nt-snippet-main">
          <span class="nt-snippet-name">{{ s.name }}</span>
          <span class="nt-snippet-meta">
            <span v-if="s.abbrev" class="nt-snippet-abbrev">{{ s.abbrev }}</span>
            <span class="nt-snippet-lines">{{ t('snippet.lineCount', { n: s.lines }) }}</span>
          </span>
        </span>
      </button>
    </div>

    <Teleport to="body">
      <div v-if="menu" class="nt-ctx-menu" :style="{ left: `${menu.x}px`, top: `${menu.y}px` }" @click="menu = null">
        <button class="nt-ctx-item" @click="onInsertClicked">{{ t('common.apply') }}</button>
        <button class="nt-ctx-item" @click="onEdit">{{ t('common.rename') }}</button>
        <button class="nt-ctx-item" @click="onRemove">{{ t('common.delete') }}</button>
      </div>
    </Teleport>
  </div>
</template>

<script lang="ts" setup>
import { onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElInput, ElSelect, ElOption } from 'element-plus'
import NtIcon from '@/components/common/NtIcon.vue'
import { api, normalizeError } from '@/api'
import { useToastStore } from '@/stores/toast'
import type { SnippetVO } from '@/types/api'

const { t } = useI18n()
const toast = useToastStore()

const items = ref<SnippetVO[]>([])
const langs = ref<string[]>([])
const showFilter = ref(false)
const keyword = ref('')
const lang = ref<string | undefined>(undefined)
const menu = ref<{ x: number; y: number } | null>(null)
const menuItem = ref<SnippetVO | null>(null)

async function load() {
  try {
    items.value = await api.listSnippets(lang.value ?? undefined, keyword.value || undefined)
    langs.value = await api.snippetLanguages()
  } catch (e) {
    toast.bad(`${t('error.snippetFailed', { reason: normalizeError(e).message })}`)
  }
}

onMounted(load)
// 筛选变化即时重查：后端按语言 + 关键字过滤，前端不再做二次筛选。
watch([lang, keyword], () => void load())

function onInsert(s: SnippetVO) {
  document.dispatchEvent(
    new CustomEvent('snippet-insert', { detail: { text: s.content, name: s.name } }),
  )
}

function onMenu(e: MouseEvent, s: SnippetVO) {
  menuItem.value = s
  menu.value = { x: e.clientX, y: e.clientY }
}

function onInsertClicked() {
  const s = menuItem.value
  menu.value = null
  if (s) onInsert(s)
}

function onEdit() {
  const s = menuItem.value
  menu.value = null
  if (!s) return
  document.dispatchEvent(new CustomEvent('snippet-edit', { detail: { id: s.id } }))
}

async function onRemove() {
  const s = menuItem.value
  menu.value = null
  if (!s) return
  await api.removeSnippet(s.id)
  await load()
}

document.addEventListener('snippets-changed', () => void load())
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

.nt-panel-filter {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-1);
  padding: var(--nt-sp-2);
  border-bottom: 1px solid var(--nt-border);
  flex: none;
}

.nt-panel-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 0 var(--nt-sp-1) var(--nt-sp-2);
}

.nt-snippet-row {
  padding: var(--nt-sp-1) var(--nt-sp-2);
  width: 100%;
  text-align: left;
}

.nt-snippet-main {
  display: flex;
  flex-direction: column;
  min-width: 0;
  flex: 1;
}

.nt-snippet-name {
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink);
  font-weight: 500;
}

.nt-snippet-meta {
  display: flex;
  gap: var(--nt-sp-2);
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-muted);
}

.nt-snippet-abbrev {
  font-family: var(--font-mono);
  color: var(--nt-primary);
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
}
</style>
