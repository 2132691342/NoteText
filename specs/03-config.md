# 03 · 配置系统

## 定位

界面偏好的唯一真相源是 SQLite `settings` 表的一个 KV 键
（`app_config` → `AppConfig` 整块 JSON）。`stores/settings` 是它的镜像 + 派生量。

## 设计

### 为什么整块存一个键

| 方案 | 代价 |
|---|---|
| 每项一行 KV | 前端每次改动要算差集，后端要支持部分更新 |
| 整块 JSON（选用） | 配置总量几十项，整块 JSON 只有几 KB；前端防抖回写即可 |

配置量大几十倍时才需要拆，现在没有。

### 分层

```
config.json（预留，固定项）      启动期读，DB 之前就要有的东西
  └─ 目前没有内容：日志级别之类也在 DB 里，DB 打不开时本来就要 fail-fast

settings 表 app_config 键       界面偏好，改一项整块回写
```

### 派生量不落库

| 派生量 | 怎么算 |
|---|---|
| `resolvedTheme` | `theme === "system"` 时跟随 `prefers-color-scheme`，实时更新 |
| `--nt-fs-scale` | `ui.zoom_level / 100`，只写 CSS 变量 |
| `--nt-vw-scale` | 窗口宽 ÷ 1440，上下限 0.8~1.35 |

这三个都只写 CSS 变量，不进配置——进配置就要考虑「用户改了这个，
那个要不要跟着变」，而派生量的定义就是「跟着变」。

### 回写策略

```ts
function update(patch: Partial<AppConfig>) {
  config.value = { ...config.value, ...patch }   // 立即生效（响应式）
  applyTheme(); applyZoom()                       // 立即落地（CSS 变量）
  scheduleSave()                                  // 防抖 400ms 回写
}
```

**立即生效 + 防抖回写**：拖动字号滑块时不会每像素都过一次 IPC，
而界面已经跟着变了。`save()` 失败时弹提示，不静默——
用户改了设置却发现没保存，比「保存失败」更烦。

### 快捷键覆盖

`config.keybindings` 只存**覆盖**（`{ command_id: "Control+Alt+S" }），
默认值在前端 `stores/commands.ts` 的 `DEFAULT_KEYBINDINGS`。

后端不该知道键位：它是纯界面关注点。这样默认键位表可以在前端自由调整
（加命令、改默认），不需要动数据库 schema。

## 约束

- 读配置失败（首次启动 / 浏览器预览）时用默认值，**不当错误弹窗**：
  用户看到的是「设置是默认的」，而不是「应用坏了」；
- 反序列化失败（旧版本字段类型变了）时回退默认值并记 warn；
- 「恢复默认」只重置配置，不动 `keybindings` 里用户自己改的部分——
  那一份是用户显式改的，恢复默认时该保留。

## 取舍

**配置不分层存**（固定项 / 界面偏好）在 NoteText 里没有收益：
DB 打不开时本来就要 fail-fast，不存在「DB 没起来但日志要先配好」的场景。
真要分（比如日志级别要在 DB 之前生效），再拆 `config.json`。
