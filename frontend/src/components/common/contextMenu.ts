// 右键菜单的条目契约：与 ContextMenu.vue 一一对应。
// 类型单独成文件的原因：SFC 的 `<script setup>` 不能导出类型，

/** 一个菜单条目。`sep` 为真时是分隔线，其余字段忽略。 */
export interface MenuItem {
  /** 文案。 */
  label?: string
  /** 分隔线。 */
  sep?: boolean
  /** 禁用（置灰不可点）。 */
  disabled?: boolean
  /** 危险项（危险色文字，仍是中性底）。 */
  danger?: boolean
  /** 快捷键提示，只显示不绑定。 */
  hint?: string
  /** 点击回调。 */
  run?: () => void
}
