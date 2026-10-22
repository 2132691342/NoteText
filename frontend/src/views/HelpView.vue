// 帮助视图：渲染 assets/docs 里的内置手册。
//
// 手册是给用户的那一份（与 specs/ docs/ 的开发文档分开维护，AGENTS.md §2.6.1）。
// 内容通过 import.meta.glob 懒加载，改文档不用改代码。
<template>
  <div class="nt-help">
    <header class="nt-help-head" data-tauri-drag-region>
      <button class="nt-text-btn" @click="router.back()">← {{ t('help.title') }}</button>
      <span class="nt-help-current">{{ currentTitle }}</span>
    </header>
    <div class="nt-help-body">
      <nav class="nt-help-nav">
        <button
          v-for="doc in DOCS"
          :key="doc.name"
          class="nt-row-item nt-help-nav-item"
          :class="{ 'is-on': current === doc.name }"
          @click="router.replace(`/help/${doc.name}`)"
        >
          {{ doc.title }}
        </button>
      </nav>
      <article ref="host" class="nt-md nt-help-content" />
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()

const DOCS = [
  { name: 'getting-started', title: '快速上手' },
  { name: 'shortcuts', title: '快捷键' },
  { name: 'features', title: '功能' },
  { name: 'faq', title: '常见问题' },
] as const

const current = computed(() => (route.params.name as string) || 'getting-started')
const currentTitle = computed(() => DOCS.find((d) => d.name === current.value)?.title ?? '')

// 手册以原始文本打包：它们是 Markdown，不是 Vue 组件，走 ?raw 才不会在构建期失解。
const DOC_RAW = import.meta.glob('../../../assets/docs/*.md', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>

const host = ref<HTMLElement | null>(null)
/** 挂载后的 Markdown 子应用：只需要 unmount，不存整个 App 实例。 */
let mdApp: { unmount: () => void } | null = null

/** 用 mavon 的预览态渲染 Markdown：依赖里已经有一个 Markdown 实现，不再引第二个。 */
async function render() {
  if (!host.value) return
  const raw = DOC_RAW[`../../../assets/docs/${current.value}.md`]
  mdApp?.unmount()
  mdApp = null
  host.value.innerHTML = ''
  if (!raw) return
  const mod = (await import('mavon-editor')) as unknown as {
    default?: { mavonEditor?: unknown }
    mavonEditor?: unknown
  }
  const comp = (mod.mavonEditor ?? (mod.default as { mavonEditor?: unknown })?.mavonEditor) as unknown
  if (!comp) return
  const { createApp, h } = await import('vue')
  const app = createApp({
    render: () =>
      h(comp as never, {
        modelValue: raw,
        defaultOpen: 'preview',
        editable: false,
        toolbarsFlag: false,
        subfield: false,
        boxShadow: false,
      }),
  })
  app.mount(host.value)
  mdApp = app
}

onMounted(() => void nextTick(render))
watch(current, () => void nextTick(render))
</script>

<style scoped>
.nt-help {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--nt-bg);
}

.nt-help-head {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
  height: var(--nt-h-menu);
  padding: 0 var(--nt-sp-3);
  border-bottom: 1px solid var(--nt-border);
  -webkit-app-region: drag;
  flex: none;
}

.nt-help-current {
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-2);
}

.nt-help-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.nt-help-nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
  width: 160px;
  padding: var(--nt-sp-3) var(--nt-sp-2);
  border-right: 1px solid var(--nt-border);
  flex: none;
}

.nt-help-nav-item {
  height: var(--nt-ctl-h);
  font-size: var(--nt-fs-sm);
  text-align: left;
  color: var(--nt-ink-2);
}

.nt-help-content {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
}
</style>
