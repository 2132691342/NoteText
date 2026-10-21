# 11 · 视觉令牌与主题

## 定位

`frontend/src/themes.css` 是全项目**唯一**的色值与尺寸刻度定义处。
`ep-theme.css` 把 Element Plus 的变量接到 `--nt-*`，`uno.config.ts` 把原子类刻度接过去。
完整的设计语言（布局、五态、文案）见 [DESIGN.md](../DESIGN.md)，这里只讲令牌本身。

## 设计

### 表面六级阶梯

```
window（窗底） < workspace（区底） ≤ surface（表单行）
≤ raised（卡片 / 面板） < hover（悬停底） < selected（选中底）
```

| 阶梯 | 浅色 | 深色 |
|---|---|---|
| `--nt-elev-window` | `#f8fafc` | `#0b1220` |
| `--nt-elev-workspace` | `#ffffff` | `#131c31` |
| `--nt-elev-surface` | `#f8fafc` | `#1e293b` |
| `--nt-elev-raised` | `#ffffff` | `#1e293b` |
| `--nt-elev-hover` | `#eef2ff` | `#263449` |
| `--nt-elev-selected` | `#e0e7ff` | `#23305a` |

相邻两级只差 2%~5% 亮度——读起来是一片连续的工作区，而不是卡片叠卡片。

语义名（`--nt-bg` / `--nt-surface` / `--nt-surface-2` / `--nt-surface-hover` /
`--nt-surface-selected`）是指向阶梯的别名。**新代码一律写 `--nt-elev-*`**，
别名不许单独赋色值（否则会盖掉阶梯）。

### 控件令牌

表单控件另走一组 `--nt-control-*`：控件要有「可以点进去」的浮凸感，
与普通面的描边平铺区分开。

```
--nt-control-bg / bg-hover / bg-disabled
--nt-control-border / border-hover
--nt-control-shadow / shadow-focus
```

### 编辑器表面

`--nt-editor-*` 一组（bg / gutter-bg / gutter-ink / active-line / selection /
cursor）：代码区可独立压暗，不让界面 chrome 的亮色抢走代码的注意力。

### 尺寸刻度

| 组 | 变量 |
|---|---|
| 字号 | `--nt-fs-2xs` ~ `--nt-fs-3xl`（8 级） |
| 间距 | `--nt-sp-1` ~ `--nt-sp-12`（4 的倍数） |
| 圆角 | `--nt-radius-xs` / `sm` / `radius` / `lg` / `xl` / `full`（七档） |
| 控件高度 | `--nt-ctl-h-sm`(26) / `ctl-h`(34) / `ctl-h-lg`(38) / `ctl-h-xl`(44) / `ctl-icon`(32) |
| Chrome 高度 | `--nt-h-menu`(32) / `toolbar`(34) / `tab`(32) / `sidebar-header`(30) / `status`(26) |
| 图标瓦片 | `--nt-tile-sm`(30) / `tile`(40) / `tile-lg`(52) |
| 布局宽度 | `--nt-w-sidebar`(260) / `dialog`(560) 等 |

**全部乘 `--nt-vw-scale`**（`useViewportScale` 按「窗口宽 ÷ 1440」写入，
上下限 0.8~1.35）：分辨率不同的机器上整套刻度等比跟随，不改代码。
字号再乘 `--nt-fs-scale`（外观设置的字号缩放）。

**组件层写死裸像素只允许在令牌定义处**。

### 主题三档

`theme: light | dark | system`。`system` 时 `resolvedTheme` 跟随
`prefers-color-scheme`，并在系统切换时实时更新（不用重启）。

`[data-theme]` 写在 `<html>` 上，由 `stores/settings` 的 `applyTheme()` 维护。

## 约束

- **色值只在 `themes.css` 定义**：组件只允许 `var(--nt-*)`，不用 hex；
- **引用不存在的变量会静默失效**——那不是「样式不好看」，是**静默丢样式**，
  最危险。新增令牌时 grep 一遍引用处；
- 渐变只允许三处：图表面积（`--nt-ch-*`）、欢迎页光晕、用户自选的背景图；
- 分层靠「圆角 + 1px 中性描边 + 一层极轻阴影」（`--nt-card-shadow`）：
  禁彩色描边、禁 backdrop-blur 叠层；
- 胶囊只给真正的圆形（状态点 / 滚动条）；chip 与输入框走 `radius-sm`
  （小控件上套满圆角就是一根胶囊，反而看不出「可以点进去」的边界）；
- 强调色只有同一个靛蓝色相的五档，没有第二个色相；
- 数据可视化取 `--nt-ch-1..4`（靛蓝明度递减）。

## 取舍

**手写 EP 主题桥而不是引 `element-plus/theme-chalk` 的暗色变量**：
theme-chalk 的暗色是一整套独立的色值，与我们的六级阶梯对不上。
接过来要让「EP 的灰」和「我们的灰」在两套主题里各自对齐，
而写桥只需要 40 行。代价是 EP 升级时要核对一遍变量名有没有变。

**不引 Tailwind**：UnoCSS 的 presetUno 已覆盖所需原子类，
且刻度能接到 `--nt-*`。Tailwind 的默认色板会诱使组件写 `bg-gray-100`，
而那正是令牌纪律要防的。
