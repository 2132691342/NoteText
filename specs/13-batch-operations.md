# 13 · 批量操作

## 定位

`src-tauri/src/service/batch.rs` + `src-tauri/src/domain/batch.rs`：
目录级查找替换与批量重命名。

与工具箱（09）的区别：工具箱是「粘一段文本 → 出一个结果」的纯计算；
批量操作**会改用户磁盘上的文件**，所以它的每一条设计都围绕「不能改错」。

---

## 设计

### 两段式是硬约束

```
scan_find  /  plan_rename    只算不写，出预览
       ↓  用户在界面上勾选
apply_replace / apply_rename  才落盘
```

**没有一步到位的接口**。批量改磁盘没有撤销键——用户勾错了、模式写错了，
文件就没了。所以后端只提供两段，不提供「扫描并应用」。

前端同样强制：批量查找替换的「应用」按钮在扫描出结果前不可用，
且只作用于勾选的文件；批量重命名的冲突项标红且不可勾选。

### 应用阶段不重算

重命名应用时前端把**预览时生成的映射**原样回传（`RenamePair`），
后端不再根据模式重算。

理由：预览与落地必须是同一份计划。重算一次就可能算出不同结果
（比如预览后用户又新建了同名文件），用户看到的和改掉的对不上。

### 失败不整体回滚，但必须记账

```rust
pub struct BatchReplaceVO {
    pub changed: usize,
    pub replacements: usize,
    pub failures: Vec<BatchFailure>,   // 逐条：哪个文件、什么原因
}
```

批量操作里「部分成功」比「全失败」有用得多——用户改 200 个文件，
不该因为第 201 个读不了就前功尽弃。但失败必须逐条报出来，
否则用户以为全改了。

### 冲突在预览阶段就挡住

`RenamePlanItem` 有两个标记位：

| 标记 | 含义 | 应用时 |
|---|---|---|
| `conflict` | 目标已存在 | 跳过，不可勾选 |
| `unchanged` | 名字没变（查找串没命中） | 跳过 |

覆盖别人的文件不可接受。`apply_rename` 还会**再预检一次**，
任一目标存在就整体拒绝——中途遇到冲突会导致「改了一半」，
那种半截状态比整体失败难恢复。

### 两条护栏复用文件域

- 超过 `MAX_FULL_READ`（32MB）的文件跳过，不静默读进内存；
- 含 NUL 的疑似二进制文件跳过，替换会破坏结构。

跳过的文件计入 `skipped`，不静默吞掉。

### glob 只支持四种形状

`*xxx` / `xxx*` / `*xxx*` / 精确名。目录级查找里用户真正会写的就这几种，
上完整 glob 库不值当。

### 正则模式是整行替换（已知限制）

迷你正则引擎（见 09）只回答「是否命中」与捕获组，
不支持按匹配段替换，所以正则模式下命中的**整行**被替换成目标串。

这是明确的能力边界，前端在正则模式下提示用户。
要完整语义需换 `regex` crate（代价：二进制 +1MB，见 AGENTS §1.1）。

---

## 契约

| 命令 | 入参 | 出参 |
|---|---|---|
| `batch_scan_find` | `BatchFindReq` | `BatchFindVO` |
| `batch_apply_replace` | `BatchReplaceReq` | `BatchReplaceVO` |
| `batch_plan_rename` | `BatchRenameReq` | `BatchRenameVO` |
| `batch_apply_rename` | `RenameApplyReq` | `BatchReplaceVO` |

`BatchFindVO.hits` 每条带 `lines`（行号，最多 50 条）与 `sample`（首条命中正文）：
列表里放几千行没有意义，用户要的是「哪几个文件、大概是什么」。

`BatchRenameReq.mode` 三种：

| 模式 | 行为 |
|---|---|
| `replace` | 在文件名（不含扩展名）里查找替换 |
| `sequence` | 加序号，`replace` 里的 `{n}` 是序号占位，`prefix` 决定前后 |
| `case` | 大小写变换（`upper` / `lower`） |

---

## 实现要点

### 收集文件：显式栈而非递归

```rust
let mut stack = vec![dir.to_path_buf()];
while let Some(cur) = stack.pop() { /* read_dir，目录入栈 */ }
```

递归写法在深层目录上有栈溢出风险，而用户的目录树深度不可控。

### 正则逐行匹配：一次编译

`regex_lines_match` 一次性编译模式、对多行复用编译产物。
逐行调 `regex_test` 等于每行重编一次——对一万行的文件就是一万次编译。

`ToolsService::new()` 会开一个内存数据库，所以 batch 侧用
`OnceLock` 共享一个实例，而不是每次匹配 new 一个。

### 写盘走原子写

`service::file::atomic_write`（同目录临时文件 + rename）。
批量操作同样不能留下半个文件。

---

## 取舍

| 决定 | 换来了什么 | 代价 |
|---|---|---|
| 两段式（scan → apply） | 用户在写之前看见要改什么；勾错了只是没勾 | 多一次交互 |
| 应用不重算 | 预览与落地产出一致 | 前端要回传完整映射 |
| 失败不回滚 | 200 个文件不会因为第 201 个失败而前功尽弃 | 可能出现部分成功，要靠 `failures` 告知 |
| 冲突整体拒绝 | 不会出现「改了一半」的半截状态 | 一个冲突就全停，用户要手动挪开那个文件 |
| glob 只支持三种形状 | 不引 glob 库 | 复杂模式（如 `**/*.rs`）不支持 |
| 正则整行替换 | 不引 regex crate | 正则模式能力受限 |
