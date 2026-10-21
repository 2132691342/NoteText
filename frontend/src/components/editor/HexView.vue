// Hex 视图主体：按 16 字节一行显示偏移 / 十六进制 / ASCII 三列。
//
// 与浮窗壳分开：内嵌进编辑区时不需要遮罩与焦点圈，
// 浮窗场景由 HexViewerDialog 包一层 NtModal。
//
// 二进制不走 invoke（字节数组会序列化成数字数组，几 MB 就堵死通道），
// 走 plugin-fs 前端直读——与后端 commands/file.rs 的注释一致。
//
// 只渲染可视区：100MB 的文件全量建 DOM 会直接把界面卡死，
// 所以按滚动位置切片，窗口外的行不建节点。
<template>
  <div class="nt-hex">
    <div class="nt-hex-head">
      <span class="nt-readout">{{ fileName }}</span>
      <span class="nt-readout">{{ total }} B</span>
      <div class="nt-hex-spacer"></div>
      <span v-if="loading" class="nt-readout">{{ t('common.loading') }}</span>
    </div>

    <div v-if="error" class="nt-empty">
      <p class="nt-hex-error">{{ error }}</p>
    </div>

    <div v-else class="nt-hex-body" ref="body" @scroll.passive="onScroll">
      <!-- 占位：撑出真实高度，让滚动条比例正确 -->
      <div class="nt-hex-spacer-box" :style="spacerStyle">
        <div class="nt-hex-window" :style="windowStyle">
          <div v-for="row in windowRows" :key="row.offset" class="nt-hex-row">
            <span class="nt-hex-off">{{ hexOff(row.offset) }}</span>
            <span class="nt-hex-bytes">
              <span
                v-for="(b, i) in row.bytes"
                :key="i"
                class="nt-hex-byte"
                :class="{ 'is-zero': b === 0 }"
              >{{ hexByte(b) }}</span>
              <span v-for="i in 16 - row.bytes.length" :key="`p${i}`" class="nt-hex-pad" />
            </span>
            <span class="nt-hex-ascii">{{ row.ascii }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { readFile } from '@tauri-apps/plugin-fs'
import { baseName } from '@/utils'

const props = defineProps<{ path: string }>()

const { t } = useI18n()

/** 行高固定：虚拟滚动要靠它算偏移，不能靠测量。 */
const ROW_H = 20
/** 每渲染批的行数：多渲染一屏，滚动时不闪白。 */
const OVERSCAN = 12

const bytes = ref<Uint8Array>(new Uint8Array())
const total = ref(0)
const loading = ref(true)
const error = ref('')
const scrollTop = ref(0)
const viewportH = ref(480)

const body = ref<HTMLElement | null>(null)

const fileName = computed(() => baseName(props.path))

const rowCount = computed(() => Math.ceil(total.value / 16))
const totalHeight = computed(() => rowCount.value * ROW_H)

const startRow = computed(() => Math.max(0, Math.floor(scrollTop.value / ROW_H) - OVERSCAN))
const visibleRows = computed(() => Math.ceil(viewportH.value / ROW_H) + OVERSCAN * 2)
const offsetY = computed(() => startRow.value * ROW_H)

// 样式走计算属性：模板里嵌反引号会让 Vue 的模板解析器把 ` 当成属性引号的边界。
const spacerStyle = computed(() => ({ height: totalHeight.value + 'px' }))
const windowStyle = computed(() => ({ transform: 'translateY(' + offsetY.value + 'px)' }))

/** 可视区的行：只建这几行的 DOM。 */
const windowRows = computed(() => {
  const out: Array<{ offset: number; bytes: number[]; ascii: string }> = []
  const end = Math.min(rowCount.value, startRow.value + visibleRows.value)
  for (let r = startRow.value; r < end; r++) {
    const off = r * 16
    const slice = Array.from(bytes.value.subarray(off, Math.min(off + 16, total.value)))
    out.push({
      offset: off,
      bytes: slice,
      ascii: slice.map((b) => (b >= 32 && b < 127 ? String.fromCharCode(b) : '.')).join(''),
    })
  }
  return out
})

function onScroll(e: Event) {
  const el = e.target as HTMLElement
  scrollTop.value = el.scrollTop
  viewportH.value = el.clientHeight
}

async function load() {
  loading.value = true
  error.value = ''
  try {
    const data = await readFile(props.path)
    bytes.value = data
    total.value = data.length
  } catch (e) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  viewportH.value = body.value?.clientHeight ?? 480
  void load()
})

watch(() => props.path, () => void load())

function hexOff(n: number): string {
  return n.toString(16).toUpperCase().padStart(8, '0')
}

function hexByte(b: number): string {
  return b.toString(16).toUpperCase().padStart(2, '0')
}
</script>

<style scoped>
.nt-hex {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
  min-height: 0;
  height: 100%;
}

.nt-hex-head {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-4);
  flex: none;
}

.nt-hex-spacer {
  flex: 1;
}

.nt-hex-body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  background: var(--nt-elev-1);
  border: 1px solid var(--nt-border);
  border-radius: var(--nt-radius-sm);
  font-family: var(--nt-font-editor);
  font-size: var(--nt-fs-sm);
}

.nt-hex-spacer-box {
  position: relative;
}

.nt-hex-window {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
}

.nt-hex-row {
  display: flex;
  align-items: center;
  height: 20px;
  gap: var(--nt-sp-4);
  padding: 0 var(--nt-sp-3);
  white-space: pre;
}

.nt-hex-off {
  color: var(--nt-ink-3);
  flex: none;
}

.nt-hex-bytes {
  display: flex;
  gap: 0.25em;
  flex: none;
}

.nt-hex-byte {
  color: var(--nt-ink);
}

/* 0x00 压暗：二进制文件里成片的 00 是噪音，不该抢视线 */
.nt-hex-byte.is-zero {
  color: var(--nt-ink-3);
  opacity: 0.5;
}

.nt-hex-pad {
  width: 2ch;
}

.nt-hex-ascii {
  color: var(--nt-ink-2);
}

.nt-hex-error {
  color: var(--nt-danger);
}
</style>
