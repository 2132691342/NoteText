import {
  defineConfig,
  presetUno,
  presetIcons,
  transformerDirectives,
  type UserConfig,
} from 'unocss'

/**
 * UnoCSS 配置：原子类是「布局 + 间距 + 排版」的唯一写法。
 *
 * 刻度全部接到 `--nt-*` 令牌（themes.css 是唯一色值与尺寸定义处）：
 * `gap-4` = `var(--nt-sp-4)`、`text-sm` = `var(--nt-fs-sm)`。
 * 所以布局与间距用原子类写，也不会绕开令牌。
 */
const sp = (n: number) => `var(--nt-sp-${n})`

export default defineConfig({
  presets: [presetUno(), presetIcons()],
  transformers: [transformerDirectives()],
  theme: {
    spacing: {
      0: '0',
      px: '1px',
      1: sp(1),
      2: sp(2),
      3: sp(3),
      4: sp(4),
      5: sp(5),
      6: sp(6),
      8: sp(8),
      10: sp(10),
      12: sp(12),
    },
    borderRadius: {
      none: '0',
      xs: 'var(--nt-radius-xs)',
      sm: 'var(--nt-radius-sm)',
      DEFAULT: 'var(--nt-radius)',
      lg: 'var(--nt-radius-lg)',
      xl: 'var(--nt-radius-xl)',
      full: 'var(--nt-radius-full)',
    },
    fontSize: {
      '2xs': ['var(--nt-fs-2xs)', '1.45'],
      xs: ['var(--nt-fs-xs)', '1.5'],
      sm: ['var(--nt-fs-sm)', '1.55'],
      md: ['var(--nt-fs-md)', '1.6'],
      lg: ['var(--nt-fs-lg)', '1.4'],
      xl: ['var(--nt-fs-xl)', '1.3'],
      '2xl': ['var(--nt-fs-2xl)', '1.2'],
    },
    colors: {
      bg: 'var(--nt-bg)',
      surface: 'var(--nt-surface)',
      surface2: 'var(--nt-surface-2)',
      hover: 'var(--nt-surface-hover)',
      selected: 'var(--nt-surface-selected)',
      border: 'var(--nt-border)',
      'border-strong': 'var(--nt-border-strong)',
      ink: 'var(--nt-ink)',
      ink2: 'var(--nt-ink-2)',
      muted: 'var(--nt-ink-muted)',
      primary: 'var(--nt-primary)',
      'primary-soft': 'var(--nt-primary-soft)',
      live: 'var(--nt-live)',
      success: 'var(--nt-success)',
      warning: 'var(--nt-warning)',
      danger: 'var(--nt-danger)',
    },
  },
  shortcuts: {
    // 布局高频组合
    'f-center': 'flex items-center justify-center',
    'f-between': 'flex items-center justify-between',
    'f-start': 'flex items-center',
    'f-col': 'flex flex-col',
    'f-1': 'flex-1 min-w-0',
    // 语义面
    'nt-panel': 'bg-surface border border-border rounded-lg',
    'nt-inset': 'bg-surface2 border border-border rounded',
    // 可点行（侧栏 / 列表）
    'nt-row':
      'flex items-center gap-3 px-3 rounded-sm cursor-pointer transition-all duration-150 hover:bg-[var(--nt-surface-hover)]',
    // Chrome 层（菜单/工具/标签/状态栏）
    'nt-chrome': 'bg-surface border-b border-border select-none',
  },
  content: {
    pipeline: {
      include: [/\.(vue|ts|html)($|\?)/],
    },
  },
} satisfies UserConfig)
