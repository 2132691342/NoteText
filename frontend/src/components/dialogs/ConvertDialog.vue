// 格式转换浮窗：JSON / YAML / TOML / XML 互转。
// 为什么在前端做而不是后端：这四种格式在配置文件场景里都是小文档

<template>
  <NtModal :title="t('convert.title')" width="720" @close="dialogs.close()">
    <div class="nt-conv">
      <div class="nt-field nt-conv-bar">
        <span class="nt-label">{{ t('convert.from') }}</span>
        <ElSelect v-model="from" size="small" class="nt-conv-sel">
          <ElOption v-for="f in FORMATS" :key="f.id" :value="f.id" :label="f.label" />
        </ElSelect>
        <ElButton size="small" class="nt-conv-swap" @click="swap">{{ t('convert.swap') }}</ElButton>
        <span class="nt-label">{{ t('convert.to') }}</span>
        <ElSelect v-model="to" size="small" class="nt-conv-sel">
          <ElOption v-for="f in FORMATS" :key="f.id" :value="f.id" :label="f.label" />
        </ElSelect>
        <div class="nt-conv-spacer" />
        <ElButton size="small" :loading="loading" type="primary" @click="run">
          {{ t('convert.convert') }}
        </ElButton>
      </div>

      <div class="nt-conv-cols">
        <div class="nt-conv-col">
          <div class="nt-tool-col-head">
            <span>{{ t('convert.input') }}</span>
            <button class="nt-text-btn nt-conv-load" @click="loadFromEditor">
              {{ t('convert.loadFromEditor') }}
            </button>
          </div>
          <textarea
            v-model="input"
            class="nt-tool-io"
            spellcheck="false"
            :placeholder="t('convert.emptyInput')"
          />
        </div>
        <div class="nt-conv-col">
          <div class="nt-tool-col-head">
            <span>{{ t('convert.output') }}</span>
            <button class="nt-text-btn" @click="copyOut">{{ t('convert.copyResult') }}</button>
          </div>
          <pre class="nt-tool-io nt-conv-out">{{ output }}</pre>
        </div>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElOption, ElSelect } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { useDialogStore } from '@/stores/dialogs'
import { useToastStore } from '@/stores/toast'
import { getText, focusedTabId } from '@/composables/useEditorRegistry'
import { copyText } from '@/utils'

const { t } = useI18n()
const dialogs = useDialogStore()
const toast = useToastStore()

type Format = 'json' | 'yaml' | 'toml' | 'xml'

const FORMATS = computed(() => [
  { id: 'json' as Format, label: t('convert.json') },
  { id: 'yaml' as Format, label: t('convert.yaml') },
  { id: 'toml' as Format, label: t('convert.toml') },
  { id: 'xml' as Format, label: t('convert.xml') },
])

const from = ref<Format>('json')
const to = ref<Format>('yaml')
const input = ref('')
const output = ref('')
const loading = ref(false)

/** 从当前标签载入正文：转配置文件的场景里「正在编辑的那个」最常见。 */
function loadFromEditor() {
  const id = focusedTabId()
  const text = id != null ? getText(id) : ''
  if (!text) {
    toast.warn(t('convert.emptyInput'))
    return
  }
  input.value = text
}

function swap() {
  const f = from.value
  from.value = to.value
  to.value = f
  // 输出搬到输入：用户连着转两次（A→B 再 B→C）时不用手动复制。
  if (output.value) {
    input.value = output.value
    output.value = ''
  }
}

async function copyOut() {
  if (!output.value) return
  await copyText(output.value)
  toast.ok(t('tool.cleared'))
}

async function run() {
  if (!input.value.trim()) {
    toast.warn(t('convert.emptyInput'))
    return
  }
  loading.value = true
  try {
    const data = await parse(input.value, from.value)
    output.value = await stringify(data, to.value)
  } catch (e) {
    output.value = ''
    toast.bad(t('convert.parseFailed', { reason: (e as Error).message }))
  } finally {
    loading.value = false
  }
}

// ---- 四种格式的解析 / 序列化 ----

/** 解析成普通 JS 对象：XML 的标签名即键，属性以 @ 前缀挂在节点上。 */
async function parse(text: string, fmt: Format): Promise<unknown> {
  if (fmt === 'json') return JSON.parse(text)
  if (fmt === 'yaml') {
    const { load } = await import('js-yaml')
    return load(text)
  }
  if (fmt === 'toml') {
    const TOML = await import('@iarna/toml')
    return TOML.parse(text)
  }
  const { XMLParser } = await import('fast-xml-parser')
  const parser = new XMLParser({
    ignoreAttributes: false,
    attributeNamePrefix: '@_',
    // 数字 / 布尔自动转型：配置里 <port>8080</port> 该是数字不是字符串。
    parseTagValue: true,
    parseAttributeValue: true,
    trimValues: true,
  })
  return parser.parse(text)
}

/** 序列化：统一 2 空格缩进，便于 diff。 */
async function stringify(data: unknown, fmt: Format): Promise<string> {
  if (fmt === 'json') return JSON.stringify(data, null, 2)
  if (fmt === 'yaml') {
    const { dump } = await import('js-yaml')
    return dump(data, { indent: 2, lineWidth: 120, noRefs: true })
  }
  if (fmt === 'toml') {
    const TOML = await import('@iarna/toml')
    return TOML.stringify(data as never)
  }
  const { XMLBuilder } = await import('fast-xml-parser')
  return new XMLBuilder({
    ignoreAttributes: false,
    attributeNamePrefix: '@_',
    format: true,
    indentBy: '  ',
  }).build(data as never)
}
</script>

<style scoped>
.nt-conv {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
  min-height: 0;
}

.nt-conv-bar {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-2);
  flex: none;
}

.nt-conv-sel {
  width: 110px;
  flex: none;
}

.nt-conv-swap {
  flex: none;
}

.nt-conv-spacer {
  flex: 1;
}

.nt-conv-cols {
  display: flex;
  gap: var(--nt-sp-3);
  flex: 1;
  min-height: 0;
}

.nt-conv-col {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-2);
}

.nt-conv-load {
  height: auto;
  font-size: var(--nt-fs-hint);
}

.nt-conv-out {
  margin: 0;
  flex: 1;
  min-height: 0;
}
</style>
