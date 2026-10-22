# 数据模型

这一篇讲 NoteText 存什么、存在哪、恢复时长什么样。

## 两个根

| 根 | 谁写 | 内容 |
|---|---|---|
| 用户磁盘上的文件 | 用户 / 其他程序 | 正文、编码、行尾、mtime |
| `%APPDATA%\NoteText\` | NoteText | SQLite、日志、备份 |

```
%APPDATA%\NoteText\
├── notetext.db      SQLite（WAL）
├── config.json      预留（固定项目前都在 DB 里）
├── logs\
│   ├── app.log      全量
│   └── warn.log     只要 warn 与 error
└── backup\          导出的备份
```

`NOTETEXT_HOME` 环境变量可以覆盖整个根目录：测试与开发隔离用它，
不碰用户真实数据。

## 表结构

### sessions（会话与窗口布局）

| 字段 | 类型 | 说明 |
|---|---|---|
| id | INTEGER PK | **固定为 1**：单窗口应用只有一份「当前会话」。仍用表而不是单例配置，是为了将来多窗口时不用迁移 |
| active_index | INTEGER | 活跃标签索引 |
| sidebar_open | INTEGER | 侧栏是否展开 |
| sidebar_tab | TEXT | 侧栏当前页签 |
| window_width / window_height | INTEGER | 正常态尺寸（最大化时记的是还原尺寸） |
| window_x / window_y | INTEGER NULL | 窗口位置 |
| window_maximized | INTEGER | 是否最大化 |
| split_ratio | REAL | 分屏比例（0–1） |
| updated_at | INTEGER | 毫秒 |

### tabs（标签页）

| 字段 | 类型 | 说明 |
|---|---|---|
| id | INTEGER PK AUTOINCREMENT | |
| ord | INTEGER | 会话内顺序，前端按它排标签栏 |
| path | TEXT | 空 = 新建未保存的草稿标签 |
| title | TEXT | 显示名 |
| language / encoding / line_ending / has_bom | TEXT/INT | 打开时探测到的，保存时按它写回 |
| cursor_line / cursor_col | INTEGER | 光标位置 |
| scroll_ratio | REAL | 滚动比例（比绝对像素稳） |
| dirty / split | INTEGER | 脏标记、分屏是否开 |
| view_type | TEXT | code / markdown / richtext / hex / image / log |
| updated_at | INTEGER | 毫秒 |

**正文不在这一表**。空 path 的标签正文在 `drafts` 表（猝死兜底），
有 path 的标签正文在磁盘上。

### bookmarks（书签）

唯一索引 `(path, line)`：同行重复加书签时报「已存在」而不是插两行，
否则面板里会出现两条长得一样的条目，用户分不清删哪条。

| 字段 | 类型 | 说明 |
|---|---|---|
| id | INTEGER PK | |
| path / line | TEXT / INTEGER | 定位 |
| label | TEXT | 别名，空则显示 `文件名:行号` |
| created_at | INTEGER | 毫秒 |

### snippets（代码片段）

| 字段 | 类型 | 说明 |
|---|---|---|
| id | INTEGER PK | |
| name / language | TEXT | 名字与语言标签（`common` = 不限） |
| content | TEXT | 正文（可能几十 KB） |
| abbrev | TEXT | 触发缩写，输入后按 Tab 展开 |
| created_at / updated_at | INTEGER | 毫秒 |

列表查询不带 `content`：面板滚动时不至于把几十条正文全拉进内存再过 IPC。

### macros（宏）

| 字段 | 类型 | 说明 |
|---|---|---|
| id | TEXT PK | **UUID 字符串**：宏要导入导出，自增整数会撞车 |
| name | TEXT | 名字 |
| steps | TEXT | `Vec<MacroStep>` 的 JSON（text / key / pause 三种） |
| created_at / updated_at | INTEGER | 毫秒 |

权威源在前端（编辑器实例持有，回放要走命令表），这里只做薄持久层。

### drafts（草稿）

| 字段 | 类型 | 说明 |
|---|---|---|
| id | TEXT PK | UUID 字符串 |
| title / content | TEXT | 显示名与正文快照 |
| cursor_line / cursor_col | INTEGER | 光标 |
| updated_at | INTEGER | 毫秒 |

**上限 2MB**（`MAX_DRAFT_BYTES`）：超限不存，只记 warn。
存不上备份不该阻断关闭流程。

### recent_files（最近文件）

| 字段 | 类型 | 说明 |
|---|---|---|
| id | INTEGER PK | |
| path | TEXT UNIQUE | |
| pinned | INTEGER | 置顶项**不参与**「超上限淘汰」 |
| opened_at | INTEGER | 毫秒 |

淘汰规则：按 `opened_at` 淘汰最旧的非置顶项，直到数量 ≤ `recent_limit`。
置顶是用户明确说「这个我常用」，不该被自动清理挤掉。

### settings（界面偏好 KV）

| 字段 | 类型 | 说明 |
|---|---|---|
| key | TEXT PK | 固定一个键 `app_config` |
| value | TEXT | `AppConfig` 整块 JSON |
| updated_at | INTEGER | 毫秒 |

**为什么不拆成每项一行**：前端每次改动都整块回写，拆行后要算差集，
而配置总量几十项，整块 JSON 只有几 KB。

## 会话恢复长什么样

```jsonc
// GET restore_session
{
  "session": { "id": 1, "active_index": 2, "window_width": 1280, ... },
  "tabs": [
    { "path": "C:\\a.txt", "title": "a.txt", "cursor_line": 42, "dirty": false, ... },
    { "path": "",          "title": "未命名-1", "dirty": false, ... },   // 草稿标签
    { "path": "C:\\b.md",  "title": "b.md",  "view_type": "markdown", ... }
  ],
  "missing": ["C:\\gone.txt"]   // 路径已不存在的标签
}
```

前端拿到后：

1. 有 path 的标签 → `read_file` 读正文 + 按 `cursor_line` 滚动选中；
2. 空 path 的标签 → `list_drafts` 按 `draft:<id>` 伪路径找回正文；
3. `missing` 非空 → 提示「N 个上次打开的文件已不存在」；
4. **恢复出来的标签一律按「未修改」处理**：上次若没保存，用户下次启动看到的
   是磁盘上的版本，标脏会误导。

## 配置结构

```jsonc
{
  "theme": "system",              // light / dark / system
  "language": "zh-CN",
  "editor": { "font_size": 13, "tab_size": 4, "word_wrap": false, ... },
  "file":    { "default_encoding": "auto", "new_line_ending": "system", ... },
  "ui":      { "zoom_level": 100, "sidebar_width": 260, "close_to_tray": false, ... },
  "keybindings": { "file.save": "Control+Alt+S" }   // 只存覆盖，默认值在前端
}
```

`#[serde(default)]` 让旧配置读出来缺项自动补默认，不需要写迁移代码。
反序列化失败（旧版本字段类型变了）时回退默认值并记 warn——配置坏掉
不该让编辑器起不来。

## 备份

导出含 `bookmarks` / `snippets` / `macros` / `drafts` / `recent_files` / `session`，
**不含 `settings`**：那是本机偏好，换机器不一定要带。

导入只覆盖「备份里有的表」，其余不动——用户在本机新加的书签不该被一份旧备份抹掉。

## 迁移策略

- `execute_batch` 里 `CREATE TABLE IF NOT EXISTS`：幂等，已存在的表跳过；
- `PRAGMA user_version` 记版本，下一个迁移写成 `MIGRATION_V2` 并在 `migrate()` 里
  加一段 `if v < 2`，顺序执行，跳版本（v0 → v2）时两段都会跑；
- 新增字段只增不删、零值兜底；废弃字段留在表里（改语义靠代码兼容），不主动 DROP。

## 取舍

| 选择 | 优势 | 代价 |
|---|---|---|
| SQLite + WAL，rusqlite `bundled` | 自带 SQLite C 源码，用户机器不用装、无 CGO 依赖链；WAL 让读写不互斥 | 二进制体积多几百 KB |
| 配置整体序列化成一行 JSON | 加字段不用写迁移、不用按项建列 | 不能按单个配置项做 SQL 查询（当前没有这个需求） |
| 会话写入用全量替换而非 diff | 标签数只在个位数到十位数，`DELETE` + 一批 `INSERT` 比算 diff 简单，也不会留下中间态 | 每次关窗重写全部标签行（行数少，代价可忽略） |
| 导出备份不含 `settings` | 换机器不必带上本机偏好 | 用户在另一台机器要重新设一次外观 |
| 导入只覆盖备份里有的表 | 本机新攒的数据不被旧备份抹掉 | 导入语义不是「完全还原」，需要在文档里说清 |
| 废弃字段留在表里不主动 DROP | 老版本数据可读，不必写破坏性迁移 | 表里会留下不再使用的列 |
