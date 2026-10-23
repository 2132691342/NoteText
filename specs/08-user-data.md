# 08 · 用户数据

## 定位

书签 / 片段 / 宏 / 草稿 / 最近文件五个表，由 `service/userdata.rs` 统一编排，
`repo/` 各自持久化。它们的共同点是「跨会话的小数据」，
分开建表而不是塞一张 KV 表——查询维度不同（书签按文件、片段按语言、
宏按名字），KV 化会把排序与筛选全推到前端。

## 设计

### 书签

```
add(path, line, label?)       同行已存在 → 更新别名（幂等，不插两行）
toggle(path, line)            有则删、无则加（F2 的行为）
list()                        附带该行正文预览
list_by_file(path)            编辑器打开文件时画行号槽
clear(path?) / clear_all()
```

**唯一索引 `(path, line)`**：同行重复加时报「已存在」而不是插两行，
否则面板里会出现两条长得一样的条目，用户分不清删哪条。

**后端返回该行正文**（`BookmarkVO.line_text`）：面板不自己读文件——
省一次 IO，也避免大文件卡面板。读不到给空串，不让列表加载失败。

### 代码片段

```
list(language?, keyword?)     后端按语言 + 关键字过滤
save(id?, name, language, content, abbrev)
find_by_abbrev(abbrev)        编辑器里输入缩写 + Tab 展开
remove(id)
languages()                   去重后的语言清单（面板筛选下拉）
```

**列表查询不带 `content`**：片段正文可能几十 KB，
面板滚动时不至于把几十条全拉进内存再过 IPC。`SnippetVO.lines` 给规模。

**名字与内容不能为空**：面板按名字排序，空名字会排最前且看不出是什么。

### 宏

```
list() / get(id) / save(id?, name, steps) / remove(id) / import_all(macros)
```

**`id` 是 UUID 字符串**（`pkg::id::uuid_v4()`）：宏要导入导出，
自增整数在导入时会与本地已存在的撞车。

**权威源在前端**：编辑器实例持有宏，回放要走命令表。
后端只做「跨设备共享 / 备份」的薄持久层，前端启动不依赖它。
`steps` 是 `Vec<MacroStep>` 的 JSON（text / key / pause 三种）。

**空步骤不保存**：一个空宏回放什么都不会发生，只会占列表。

### 草稿

```
list() / get(id) / save(path, title, content) -> bool / remove(id)
```

**只存「关闭时的快照」**：正文实时权威在前端编辑器，
过 IPC 同步每个按键会把主线程与 WebView 之间的通道占满。

**上限 2MB**（`MAX_DRAFT_BYTES`）：超限不存，只记 warn。
存不上备份不该阻断关闭流程，所以返回 `bool` 而不是错误。

**`path` 是 `draft:<id>` 伪路径**：与 `session.tabs` 里空 path 的标签对应，
恢复时前端按它对上。

### 最近文件

```
touch(path)                   置顶已存在的项，不存在则插入，opened_at 推到当前
trim(limit)                   淘汰到上限以内
set_pinned(path, pinned) / remove(path) / clear()
```

**淘汰规则**：按 `opened_at` 淘汰最旧的**非置顶**项，直到数量 ≤ `recent_limit`。
`limit = 0` 表示「不记最近文件」：清空但保留置顶项。

**置顶永不淘汰**：用户明确说「这个我常用」，不该被自动清理挤掉。

### 备份

导出含五个表 + session，**不含 settings**（那是本机偏好，换机器不一定要带）。
导入只覆盖「备份里有的表」，其余不动——用户在本机新加的书签
不该被一份旧备份抹掉。

## 约束

- 五个数据源共用「列表 + 增删 + 备份」的形态，所以放一个 service 而不是五个：
  它们只是「某张表」的编排，不值得各自开目录（[AGENTS.md §2.1](../AGENTS.md) 的开包门槛）；
- 出参里的切片构造时初始化：空列表必须序列化成 `[]` 而不是 `null`
  （守护见 [12](12-testing.md) 的 `empty_lists_serialize_to_array_not_null`）；
- 时间一律毫秒整数，`now_ms()` 在 `app::bootstrap` 里统一定义。

## 取舍

**草稿上限 2MB 而不是更大**：大草稿本就不该开在新建标签里
（用户该直接打开一个文件）。硬塞会把数据库撑大并拖慢会话恢复。
代价是用户在新标签里写了个大文档没保存，关掉就没了——
那种情况状态栏会提示「草稿过大未备份」。

**最近文件不放目录**：只放文件。目录列表的维护要额外一张表，
而「打开文件夹」对话框自己会记住上次位置（`FileConfig.last_folder`），
收益不成比例。
