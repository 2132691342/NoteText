// 帮助视图：渲染 assets/docs 里的内置手册。
//
// 手册是给用户的那一份（与 specs/ docs/ 的开发文档分开维护，AGENTS.md §2.6.1）。
// 内容通过 import.meta.glob 懒加载，改文档不用改代码。
<template>
  <div class="nt-help">
    <header class="nt-help-head">
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
      <article class="nt-md nt-help-content">
        <component :is="currentComp" v-if="currentComp" />
      </article>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, defineAsyncComponent } from 'vue'
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

// 手册按需加载：帮助页不该把四篇文档全打进首屏。
const currentComp = computed(() =>
  defineAsyncComponent(() => import(`../../assets/docs/${current.value}.md`)),
)
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
