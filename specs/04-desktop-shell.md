# 04 · 桌面壳

## 定位

Tauri 只负责「窗口 + 系统能力」，业务全部走 invoke。
这个分工让前端在浏览器里也能开发调试（`npm run dev` 显示空态而不是崩）。

## 窗口

- **无边框 + 前端自绘标题栏**（`tauri.conf.json` 的 `decorations: false`）：
  视觉统一，且标题栏中区能放「当前文件名」——这是编辑器最常看的信息；
- 最小 800×600，默认 1280×800，`center: true`；
- 拖拽区用 `data-tauri-drag-region` 声明；标题栏可拖，
  菜单栏不可拖（合并会让整个菜单条变拖拽区，点菜单要先抢回来）。

### 窗控全走后端

自绘标题栏后，最小化 / 最大化 / 关闭都不再是系统装饰：

| 按钮 | 命令 |
|---|---|
| ─ | `window_minimize` |
| □ / ❐ | `window_toggle_maximize`（按 `window_is_maximized` 切图标） |
| ✕ | `window_close`（只广播事件，不直接关） |

## 关闭与退出（必须显式区分）

这是壳层最容易出错的地方。核心约束：

**Tauri 的 `CloseRequested` 里 `api.prevent_close()` 一调，这次关闭就被拦下。
前端不置放行位，窗口会「点了 X 没反应」。**

| 来源 | 条件 | `should_prevent` | 结果 |
|---|---|---|---|
| 用户点 X / Alt+F4 | 已确认退出 | false | 真正退出 |
| 用户点 X / Alt+F4 | 关闭到托盘开、窗口可见 | true + hide | 收进托盘 |
| 用户点 X / Alt+F4 | 其余 | true + 广播 `app:before-close` | 等前端裁决 |
| 托盘「退出」 | 先置确认位 | false | 真正退出 |

前端链路（`composables/useQuitGuard.ts`）：

```
点 X → invoke('window_close')
     → 后端广播 app:before-close
     → 前端：存会话 + 存草稿
     → 有未保存？弹原生确认
     →   取消 → invoke('cancel_quit')，窗口保持打开
     →   确认 → 逐个保存 → invoke('confirm_quit') → invoke('force_quit')
     → force_quit 先置 QUIT_CONFIRMED 再 app.exit(0)
```

**`force_quit` 必须先置位再 exit**：`exit` 内部会触发 `CloseRequested`，
不置位会被自己拦回去，进程退不掉。

**收托盘前先提示**：窗口「凭空消失」最让用户困惑。
托盘没就绪时不拦截——藏进一个还不存在的托盘，用户再也找不回来。

## 托盘

`tauri.conf.json` 的 `trayIcon` + `shell/tray.rs` 建菜单。

| 菜单项 | 行为 |
|---|---|
| 显示主窗口 | `WindowShow + WindowUnminimise + SetFocus` |
| 新建文档 | 同上 + 广播 `app:new-document` |
| 退出 | `close_policy::force_quit` |

约束：

- 「显示窗口」必须 `Show + Unminimise + SetFocus` **三件都做**：
  隐藏与最小化是两种状态，只做一半唤不回；
- 菜单回调统一 `catch_unwind`：托盘是进程级组件，
  回调里炸一个会把编辑器一起带走；
- 左键单击 = 唤起窗口，右键 = 菜单。

## 单实例

`tauri-plugin-single-instance`：内部用本地 socket + 互斥体，
比自己写命名管道可靠。这里只负责「收到 argv 后干什么」：

```
二次启动 → forward(app, argv)
  ├─ 唤起窗口（show + unminimize + focus）
  └─ 第一个看起来像路径的参数 → emit('app:open-file', path)
```

**不带文件也唤起**：只想再开一个窗口时，双击图标像没反应会很困惑。

首次启动时命令行里的文件路径由 `app::lifecycle` 在 ready 之后发——
立即发会丢（Vue 还没挂载，事件没人听）。前端用 `composables/useShellIntent.ts`
的意图队列兜底：事件早于视图挂载也存得下，视图挂载时消费一次。

## 生命周期

| 钩子 | 职责 |
|---|---|
| `setup` | 存 AppHandle → 装托盘 → 广播 `app:ready` → 同步关闭策略 |
| `on_window_event` | `CloseRequested`：按关闭策略收托盘或放行 |
| `RunEvent::Exit` | 落一条日志 |

**启动失败必须让用户看见**：路径解析或 DB 失败时，
窗口还没起来，只能走 `platform::dialog_win` 的 Win32 `MessageBoxW`。
静默退出会让用户以为「双击没反应」。

## 文件关联

`tauri.conf.json` 的 `bundle.fileAssociations`：

| 扩展名 | |
|---|---|
| txt md log json yml yaml toml ini csv xml | `role: "Viewer"` |

**`Viewer` 而不是 `Editor`**：出现在「打开方式」列表里，
不抢占用户已有的默认程序。抢占默认是安装器最招人骂的行为之一。

## 取舍

- **无边框 + 自绘标题栏**：视觉统一、标题栏能放文件名；
  代价是要处理拖拽区、最大化态、双击标题栏（本期没做双击最大化，
  因为编辑区里双击是选词，误触率高）。
- **不做开机自启**：编辑器打开频率低，自启收益小于
  「登录后看到一个不需要的窗口」的烦人程度。
- **不做多窗口**：单窗口 + 多标签覆盖 99% 场景；
  多窗口要处理「同一文件两个标签」「哪个窗口接收单实例转交」，
  复杂度不成比例。
