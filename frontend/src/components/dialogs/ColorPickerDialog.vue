// 取色器浮窗：屏幕取色 + HEX / RGB / HSL 三种表示。
// 用 EyeDropper API（Chromium 内核的 WebView2 支持）；不支持时

<template>
  <NtModal :title="t('viewer.colorPicker')" width="420" @close="dialogs.close()">
    <div class="nt-cp">
      <div class="nt-cp-preview" :style="{ background: hex }" />

      <div class="nt-field">
        <span class="nt-label">{{ t('viewer.hex') }}</span>
        <ElInput v-model="hex" size="small" spellcheck="false" @change="onHex" />
        <ElButton size="small" @click="copyValue(hex)">{{ t('common.copy') }}</ElButton>
      </div>

      <div class="nt-field">
        <span class="nt-label">{{ t('viewer.rgb') }}</span>
        <span class="nt-readout nt-cp-val">{{ rgbText }}</span>
        <ElButton size="small" @click="copyValue(rgbText)">{{ t('common.copy') }}</ElButton>
      </div>

      <div class="nt-field">
        <span class="nt-label">{{ t('viewer.hsl') }}</span>
        <span class="nt-readout nt-cp-val">{{ hslText }}</span>
        <ElButton size="small" @click="copyValue(hslText)">{{ t('common.copy') }}</ElButton>
      </div>

      <div class="nt-cp-actions">
        <ElButton size="small" type="primary" :loading="picking" @click="pick">
          {{ t('viewer.pick') }}
        </ElButton>
        <span v-if="!supported" class="nt-cp-hint">{{ t('viewer.pickUnsupported') }}</span>
      </div>
    </div>
  </NtModal>
</template>

<script lang="ts" setup>
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton, ElInput } from 'element-plus'
import NtModal from '@/components/common/NtModal.vue'
import { useDialogStore } from '@/stores/dialogs'
import { useToastStore } from '@/stores/toast'
import { copyText } from '@/utils'

const { t } = useI18n()
const dialogs = useDialogStore()
const toast = useToastStore()

/** EyeDropper 只在 Chromium 系可用；类型不在标准 lib.dom 里，自己声明最小形状。 */
interface EyeDropperLike {
  open: () => Promise<{ sRGBHex: string }>
}
const supported = typeof window !== 'undefined' && 'EyeDropper' in window

const hex = ref('#5b5fe8')
const picking = ref(false)

const rgb = computed(() => hexToRgb(hex.value))

const rgbText = computed(() => (rgb.value ? `rgb(${rgb.value.r}, ${rgb.value.g}, ${rgb.value.b})` : '—'))

const hslText = computed(() => {
  if (!rgb.value) return '—'
  const h = rgbToHsl(rgb.value.r, rgb.value.g, rgb.value.b)
  return `hsl(${h.h}, ${h.s}%, ${h.l}%)`
})

/** 输入框改 HEX：不合法的串不接收（保留上一个有效值，界面不跳）。 */
function onHex() {
  const v = hex.value.trim()
  const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(v)
  if (!m) {
    toast.warn(t('viewer.badHex'))
    return
  }
  hex.value = normalizeHex(m[1])
}

async function pick() {
  if (!supported) return
  picking.value = true
  try {
    const dropper = new (window as unknown as { EyeDropper: new () => EyeDropperLike }).EyeDropper()
    const res = await dropper.open()
    hex.value = normalizeHex(res.sRGBHex.replace('#', ''))
  } catch {
    /* 用户按 Esc 取消：不是错误，不提示 */
  } finally {
    picking.value = false
  }
}

async function copyValue(v: string) {
  if (!v || v === '—') return
  await copyText(v)
  toast.ok(t('viewer.copiedColor', { value: v }))
}

function normalizeHex(h: string): string {
  const s = h.replace('#', '')
  const full = s.length === 3 ? s.split('').map((c) => c + c).join('') : s
  return `#${full.toLowerCase()}`
}

function hexToRgb(hex: string): { r: number; g: number; b: number } | null {
  const m = /^#([0-9a-f]{6})$/i.exec(hex)
  if (!m) return null
  const n = parseInt(m[1], 16)
  return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 }
}

function rgbToHsl(r: number, g: number, b: number): { h: number; s: number; l: number } {
  const rn = r / 255
  const gn = g / 255
  const bn = b / 255
  const max = Math.max(rn, gn, bn)
  const min = Math.min(rn, gn, bn)
  const l = (max + min) / 2
  if (max === min) return { h: 0, s: 0, l: Math.round(l * 100) }
  const d = max - min
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min)
  let h: number
  if (max === rn) h = ((gn - bn) / d + (gn < bn ? 6 : 0)) / 6
  else if (max === gn) h = ((bn - rn) / d + 2) / 6
  else h = ((rn - gn) / d + 4) / 6
  return { h: Math.round(h * 360), s: Math.round(s * 100), l: Math.round(l * 100) }
}
</script>

<style scoped>
.nt-cp {
  display: flex;
  flex-direction: column;
  gap: var(--nt-sp-3);
}

.nt-cp-preview {
  height: 72px;
  border-radius: var(--nt-radius);
  border: 1px solid var(--nt-border);
  transition: background-color var(--nt-dur) var(--nt-ease);
}

.nt-cp-val {
  flex: 1;
  min-width: 0;
  font-family: var(--font-mono);
  font-size: var(--nt-fs-sm);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.nt-cp-actions {
  display: flex;
  align-items: center;
  gap: var(--nt-sp-3);
}

.nt-cp-hint {
  font-size: var(--nt-fs-hint);
  color: var(--nt-ink-muted);
}
</style>
