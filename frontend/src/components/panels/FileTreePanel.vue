// 文件树面板：懒展开的目录树。
// 展开状态在 store 里（切标签不丢），子项缓存按目录。

<template>
  <div class="nt-tree" @contextmenu.prevent="onBlankMenu">
    <div v-if="!tree.root" class="nt-empty">
      <p>{{ t('sidebar.openFolderHint') }}</p>
      <ElButton size="small" @click="commands.run('file.openFolder')">
        {{ t('menu.openFolder') }}
      </ElButton>
    </div>
    <template v-else>
      <!-- 根目录本身是第一层：与资源管理器一致，树从「skill」开始而不是它的内容。
           key 绑根路径：换文件夹时重挂载，新树默认展开。 -->
      <FileTreeNode
        :key="tree.root"
        :entry="rootEntry"
        :depth="0"
        :initially-expanded="true"
        @contextmenu="onNodeMenu"
      />
    </template>
  </div>

  <!-- 右键菜单：与编辑器 / 标签栏共用 ContextMenu -->
  <ContextMenu
    v-if="menu"
    :x="menu.x"
    :y="menu.y"
    :items="treeMenuItems"
    @close="menu = null"
  />

  <!-- 树内拖拽：源节点与目标文件夹之间画一条反馈，落点由 drop 定 -->
  <div
    v-if="dropTarget"
    class="nt-tree-drop-hint"
    :style="{ top: `${dropTarget.y}px`, left: `${dropTarget.x}px` }"
  >
    {{ t('tree.dropHint', { name: dropTarget.name }) }}
  </div>
</template>

<script lang="ts" setup>
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ElButton } from 'element-plus'
import { useFileTreeStore } from '@/stores/fileTree'
import { useEditorStore } from '@/stores/editor'
import { useCommandStore } from '@/stores/commands'
import { useDialogStore } from '@/stores/dialogs'
import { useToastStore } from '@/stores/toast'
import { api, normalizeError } from '@/api'
import { copyText, dirName, baseName, isArchivePath, isSplitFirstPart } from '@/utils'
import ContextMenu from '@/components/common/ContextMenu.vue'
import FileTreeNode from '@/components/panels/FileTreeNode.vue'
import type { MenuItem } from '@/components/common/contextMenu'
import type { DirEntry, FileOpVO } from '@/types/api'

const { t } = useI18n()
const tree = useFileTreeStore()
const editor = useEditorStore()
const commands = useCommandStore()
const dialogs = useDialogStore()
const toast = useToastStore()

const menu = ref<{ x: number; y: number } | null>(null)
const menuEntry = ref<DirEntry | null>(null)

/** 根节点：把打开的文件夹本身合成一个 DirEntry，树从它开始。 */
const rootEntry = computed<DirEntry>(() => ({
  name: baseName(tree.root),
  path: tree.root,
  is_dir: true,
  size: 0,
  modified_at: 0,
  read_only: false,
}))

/** 节点右键冒泡上来的意图：统一在这里开菜单。 */
function onNodeMenu(payload: { x: number; y: number; entry: DirEntry }) {
  menuEntry.value = payload.entry
  menu.value = { x: payload.x, y: payload.y }
}

/** 一组文件操作的结果统一报一次：失败项逐条说，成功项只说条数。 */
function report(res: FileOpVO, onOk?: () => void) {
  const bad = res.results.filter((r) => !r.ok)
  if (res.succeeded > 0) {
    toast.ok(t('toast.opDone', { n: res.succeeded }))
    onOk?.()
  }
  if (bad.length > 0) toast.bad(bad.map((r) => r.message).join('\n'))
}

function onBlankMenu(e: MouseEvent) {
  if (!tree.root) return
  menuEntry.value = null
  menu.value = { x: e.clientX, y: e.clientY }
}

/** 右键菜单条目：按被点中的节点算（空白处只给目录级动作）。 */
const treeMenuItems = computed<MenuItem[]>(() => {
  const entry = menuEntry.value
  const isDir = entry?.is_dir ?? false
  const targetDir = isDir ? entry!.path : entry ? dirName(entry.path) : tree.root
  // 压缩包节点：菜单换成归档动作（打开 / 解压 / 测试），普通压缩不再给
  const archiveEntry = entry && !isDir && isArchivePath(entry.path) ? entry : null
  // 分卷首卷（.001）：给「合并分卷」
  const splitEntry = entry && !isDir && isSplitFirstPart(entry.path) ? entry : null
  return [
    {
      label: t('menu.open'),
      disabled: !entry,
      run: async () => {
        if (!entry) return
        if (entry.is_dir) {
          await tree.refresh(entry.path)
          expandDir(entry.path)
        } else {
          await editor.openPath(entry.path)
        }
      },
    },
    { sep: true },
    {
      label: t('tree.newFile'),
      // 空白处右键 = 在根目录建；建在文件下没意义，只有目录（含根）可用。
      disabled: !!entry && !isDir,
      run: () => void startMenuEdit('new-file'),
    },
    {
      label: t('tree.newFolder'),
      disabled: !!entry && !isDir,
      run: () => void startMenuEdit('new-dir'),
    },
    { sep: true },
    {
      label: t('menu.cut'),
      hint: 'Ctrl+X',
      disabled: !entry,
      run: () => {
        if (entry) tree.setClipboard([entry.path], true)
      },
    },
    {
      label: t('menu.copy'),
      hint: 'Ctrl+C',
      disabled: !entry,
      run: () => {
        if (entry) tree.setClipboard([entry.path], false)
      },
    },
    {
      label: t('tree.pasteTo', { name: entry ? entry.name : baseName(tree.root) }),
      disabled: !tree.clipboard,
      run: () => void paste(targetDir),
    },
    {
      label: t('tree.duplicate'),
      disabled: !entry,
      run: () => void paste(targetDir, true),
    },
    {
      label: t('common.rename'),
      disabled: !entry,
      run: () => void startMenuEdit('rename'),
    },
    { sep: true },
    // 归档动作：压缩包给打开 / 解压 / 测试，分卷首卷给合并，其余给压缩 / 分卷
    ...(archiveEntry
      ? [
          {
            label: t('tree.openArchive'),
            run: () => {
              menu.value = null
              dialogs.open('archiveBrowser', '', { path: archiveEntry.path })
            },
          },
          {
            label: t('tree.extractTo'),
            run: () => {
              menu.value = null
              dialogs.open('extractArchive', '', { path: archiveEntry.path })
            },
          },
          {
            label: t('tree.extractHere', { name: archiveEntry.name }),
            run: () => void extractHere(archiveEntry.path),
          },
          {
            label: t('tree.testArchive'),
            run: () => void testArchive(archiveEntry.path),
          },
        ]
      : []),
    ...(splitEntry
      ? [
          {
            label: t('tree.joinParts'),
            run: () => {
              menu.value = null
              dialogs.open('splitJoin', '', { mode: 'join', path: splitEntry.path })
            },
          },
        ]
      : []),
    ...(entry && !archiveEntry && !splitEntry
      ? [
          {
            label: t('tree.compress'),
            run: () => {
              menu.value = null
              dialogs.open('createArchive', '', { sources: [entry.path] })
            },
          },
        ]
      : []),
    ...(!entry && tree.root
      ? [
          {
            label: t('tree.compress'),
            run: () => {
              menu.value = null
              dialogs.open('createArchive', '', { sources: [] })
            },
          },
          {
            label: t('tree.extractArchive'),
            run: () => {
              menu.value = null
              dialogs.open('extractArchive', '', {})
            },
          },
        ]
      : []),
    ...(!archiveEntry && !splitEntry && entry && !isDir
      ? [
          {
            label: t('tree.splitFile'),
            run: () => {
              menu.value = null
              dialogs.open('splitJoin', '', { mode: 'split', path: entry.path })
            },
          },
        ]
      : []),
    { sep: true },
    {
      label: t('tab.copyPath'),
      disabled: !entry,
      run: async () => {
        if (!entry) return
        await copyText(entry.path)
        toast.ok(t('common.copied'))
      },
    },
    {
      label: t('tab.revealInExplorer'),
      disabled: !entry,
      run: () => {
        if (entry) void api.openPath(entry.is_dir ? entry.path : dirName(entry.path))
      },
    },
    { sep: true },
    {
      label: t('common.refresh'),
      run: () => {
        if (tree.root) void tree.refresh(tree.root)
      },
    },
    {
      label: t('tree.delete'),
      danger: true,
      disabled: !entry,
      run: () => void removeEntry(),
    },
  ]
})

/** 解压到旁边：目标 = 压缩包同级 + 同名子目录，冲突保留两者（rename 策略）。 */
async function extractHere(archivePath: string) {
  menu.value = null
  try {
    await api.archive.extract({
      archive: archivePath,
      dest: dirName(archivePath),
      entries: null,
      password: null,
      overwrite: 'rename',
      create_subdir: true,
    })
    toast.ok(t('archive.queued'))
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}

/** 测试完整性：提交队列，结果由任务面板与 toast 反馈。 */
async function testArchive(archivePath: string) {
  menu.value = null
  try {
    await api.archive.test({ archive: archivePath, password: null })
    toast.ok(t('archive.queued'))
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
}

/**
 * 菜单项触发的行内编辑：菜单是全局浮层，拿不到树节点的 setup 作用域，
 * 所以这里用事件把意图传下去，节点自己在监听里开输入框。
 * 空白处触发的动作落根目录（menuEntry 为 null 时 path 取 tree.root）。
 */
function startMenuEdit(mode: 'rename' | 'new-file' | 'new-dir') {
  menu.value = null
  const path = menuEntry.value?.path ?? tree.root
  if (!path) return
  window.dispatchEvent(new CustomEvent('nt-tree-edit', { detail: { path, mode } }))
}

/** 让某个目录节点展开：菜单触发，节点在监听里翻自己的展开态。 */
function expandDir(path: string) {
  window.dispatchEvent(new CustomEvent('nt-tree-expand', { detail: { path } }))
}

/** 粘贴：剪切 = 移动，复制 = 复制，「创建副本」= 复制到同目录并加序号。 */
async function paste(destDir: string, autoSuffix = false) {
  const cb = tree.clipboard
  if (!cb || !destDir) return
  try {
    const res = cb.cut
      ? await api.fileOps.move(cb.paths, destDir, autoSuffix)
      : await api.fileOps.copy(cb.paths, destDir, autoSuffix)
    report(res, () => {
      if (cb.cut) for (const p of cb.paths) editor.renamePath(p, '')
      tree.setClipboard([], false)
    })
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
  if (tree.root) await tree.refresh(tree.root)
}

/** 删除：目录递归删，删前问一次。 */
async function removeEntry() {
  const entry = menuEntry.value
  if (!entry) return
  menu.value = null
  const ok = await api.showConfirm(
    t('tree.deleteConfirmTitle'),
    t(entry.is_dir ? 'tree.deleteFolderBody' : 'tree.deleteFileBody', { name: entry.name }),
  )
  if (!ok) return
  try {
    const res = await api.fileOps.remove([entry.path])
    report(res, () => {
      editor.renamePath(entry.path, '')
      tree.markDirty(entry.path, false)
    })
  } catch (e) {
    toast.bad(normalizeError(e).message)
  }
  if (tree.root) await tree.refresh(tree.root)
}

/** 拖拽时的落点提示：跟随目标目录行。 */
const dropTarget = computed(() => {
  const path = tree.dropTarget
  if (!path) return null
  const el = document.querySelector<HTMLElement>(`.nt-tree-row[data-path="${cssEscape(path)}"]`)
  if (!el) return null
  const r = el.getBoundingClientRect()
  return { x: r.right - 12, y: r.bottom + 4, name: baseName(path) }
})

function cssEscape(s: string): string {
  return s.replace(/["\\]/g, '\\$&')
}
</script>

<style scoped>
.nt-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--nt-sp-3);
  padding: var(--nt-sp-6) var(--nt-sp-3);
  text-align: center;
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink-muted);
}

/* 拖拽落点：目标行整体给一层描边，用户看得见「会掉到哪」 */
.nt-tree-row.is-drop {
  background: var(--nt-primary-soft);
  box-shadow: inset 0 0 0 1px var(--nt-primary);
}

/* 剪切待粘贴态：半透明，与资源管理器一致 */
.nt-tree-row.is-cut {
  opacity: 0.45;
}

.nt-tree-input {
  flex: 1;
  min-width: 0;
  height: calc(var(--nt-ctl-h-sm) - 4px);
  padding: 0 var(--nt-sp-1);
  font-size: var(--nt-fs-sm);
  color: var(--nt-ink);
  background: var(--nt-control-bg);
  border: 1px solid var(--nt-primary);
  border-radius: var(--nt-radius-xs);
}

.nt-tree-drop-hint {
  position: fixed;
  z-index: var(--nt-z-pop);
  padding: var(--nt-sp-1) var(--nt-sp-2);
  font-size: var(--nt-fs-hint);
  color: var(--nt-primary);
  background: var(--nt-elev-raised);
  border: 1px solid var(--nt-primary);
  border-radius: var(--nt-radius-xs);
  box-shadow: var(--nt-card-shadow);
  pointer-events: none;
}
</style>
