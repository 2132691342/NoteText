// 外部变更：文件被别的程序改了怎么办。
//
// 三种情况分开处理（这是编辑器最容易被骂的地方）：
// - 标签干净 → 静默重载（用户没在编辑，别打断他）；
// - 标签脏 → 提示「已修改，重新载入会丢本地修改」，让用户选；
// - 文件被删 / 移走 → 只提示，不动编辑器内容（用户可能正想复制出去）。
import { onMounted, onUnmounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { useEditorStore } from '@/stores/editor'
import { useFileTreeStore } from '@/stores/fileTree'
import { useI18n } from 'vue-i18n'
import { useToastStore } from '@/stores/toast'
import { isDesktop } from '@/api'
import { baseName } from '@/utils'

export function useExternalChange() {
  const { t } = useI18n()
  const editor = useEditorStore()
  const tree = useFileTreeStore()
  const toast = useToastStore()

  if (!isDesktop) return

  let unlisten: (() => void) | null = null

  onMounted(async () => {
    unlisten = await listen<string[]>('fs:changed', (ev) => {
      for (const path of ev.payload) {
        handlePath(path)
      }
      // 文件树刷新：外部新建 / 删除文件要反映出来。
      if (tree.root) void tree.refresh(tree.root)
    })
  })

  onUnmounted(() => unlisten?.())

  function handlePath(path: string) {
    const tab = editor.findByPath(path)
    if (tab) {
      if (tab.dirty) {
        toast.bad(t('toast.fileChanged', { name: baseName(path) }))
      } else {
        void editor.reload(tab.id)
        toast.info(t('toast.fileChanged', { name: baseName(path) }))
      }
      return
    }
    // 不是打开的标签：只刷树，不打扰用户。
  }
}
