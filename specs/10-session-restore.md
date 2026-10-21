# 10 · 会话与恢复

## 定位

`src-tauri/src/service/session.rs` + `repo/session.rs`：窗口布局与标签元数据的
保存 / 恢复。**正文归前端编辑器，布局与元数据归 SQLite**。

## 设计

### 存什么

| 存 | 不存 |
|---|---|
| 标签的顺序、路径、标题、语言、编码、行尾、光标、滚动比例、脏标记、分屏 | 正文 |
| 窗口尺寸 / 位置 / 最大化态、分屏比例、侧栏开关与页签、活跃索引 | 编辑器实例状态（撤销栈等） |

有 path 的标签正文在磁盘上；空 path 的标签正文在 `drafts` 表（猝死兜底）。

### 保存

```
save_session(req)
  └─ 一个事务：
       ├─ sessions 行 upsert（id 固定 1）
       ├─ DELETE FROM tabs
       └─ 批量 INSERT tabs
```

**整块替换而不是增量 diff**：标签数量在个位数到几十之间，
一次 DELETE + 批量 INSERT 比算差集简单，且不会有「删了一半崩溃」的中间态
（整段在一个事务里）。

`tabs` 为空表示「全部关完」：此时也要写一条 sessions（记住窗口大小），
但 tabs 表清空——下次启动是干净的欢迎页。

### 恢复

```
restore_session() -> SessionRestoreVO { session, tabs, missing }
```

前端拿到后：

1. 有 path 的标签 → `read_file` 读正文 + 按 `cursor_line` 滚动选中；
2. 空 path 的标签 → `list_drafts` 按 `draft:<id>` 伪路径找回正文；
3. `missing` 非空 → 提示「N 个上次打开的文件已不存在」；
4. **恢复出来的标签一律按「未修改」处理**：上次若没保存，
   用户下次启动看到的是磁盘上的版本，标脏会误导。

### 布局补丁

`patch_session(active_index?, sidebar_open?, sidebar_tab?, split_ratio?)`
用于布局变化（侧栏开关 / 分屏），不必每次重传标签表。

**注意 `patch_session` 内部会 `load` 一次再全量 `save`**：
这是「整块替换」策略的代价——补一个字段也要重写 tabs。
标签几十条时这不是问题，但**如果将来标签上百，
该改成单字段 UPDATE**。

### 窗口状态记忆

`remember_window(window)` 由前端在 resize / 最大化时调：

- 只记「正常态」的尺寸：最大化时记的是最大化前的还原尺寸，
  否则恢复出来是个奇怪的大窗口；
- `window_maximized` 单独记，恢复时先设尺寸再最大化。

## 约束

- `restore_session` 失败不阻塞启动：用户看到的是欢迎页，
  而不是「应用起不来」；
- 保存失败（磁盘满 / DB 锁）时记 warn 后继续——
  拦着用户不让关比丢一次会话更糟；
- 标签的 `dirty` 存进库但**恢复时一律置 false**（见上）；
- 会话行的 `id` 固定为 1，`CHECK (id = 1)` 约束兜底
  （防将来误插第二行）。

## 取舍

**会话存 SQLite 而不是 JSON 文件**：书签 / 片段 / 宏都在库里，
会话放一起可以一起备份（导出备份含 session）。放文件要多一份格式与一套读写。

**正文不入库（草稿快照除外）**：存正文就要处理「数据库体积随文件增大」
「大文件拖慢恢复」两个问题，而收益只是「崩溃恢复」——
那个场景已经由草稿快照覆盖（新建未保存的标签）。

**草稿上限 2MB**：见 [specs/08](08-user-data.md)。
