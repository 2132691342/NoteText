# 19 · 大文件分块编辑、表格编辑与崩溃恢复

## 定位

三件互为补充的事，共同点是「不让用户的文件变成打不开、改不了、丢得掉」：

1. **大文件分块浏览 + 单块编辑**：GB 级文件不再被一刀切拒绝，秒开可浏览；
   需要改动时只解锁当前这一块，写回只替换那块字节；
2. **表格可编辑**：CSV / TSV 走文本网格；XLSX / ODS 走单元格补丁，
   **样式与图表原地保留**；两条链路都带表内查找替换；
3. **崩溃恢复**：异常退出后找回 30 秒前的未保存内容。

## 大文件分块（承接 [05 · 文件域](05-file-domain.md)）

### 分档

| 大小 | 行为 |
|---|---|
| ≤ 32MB | 全量读入，正常编辑（`MAX_FULL_READ`） |
| 32MB ~ 512MB | 弹窗让用户选：「普通打开（整篇编辑）」或「分块打开」 |
| 512MB ~ 64GB | 不再询问，直接分块（秒开，可逐块编辑） |
| > 64GB | 拒绝（索引扫描本身要过一遍文件，不承诺无限大） |

阈值常量：`service::file::MAX_FULL_READ` / `MAX_FORCE_FULL_READ` 与前端
`stores/editor.ts` 的 `FULL_READ_LIMIT` / `BIG_EDIT_LIMIT` 两两同值，改一边必须同步另一边。
分块上限在 `service::big_file::MAX_CHUNKED_FILE`。

### 单块写回（`big_file_write_block`）

写回只改一个块的字节区间，三道守护缺一不可：

| 守护 | 实现 | 缺了会怎样 |
|---|---|---|
| 乐观锁 | 重读磁盘那块字节，与 `old_text` 比对，不一致报 `1008` | 别人刚改过的内容被旧编辑盖掉，界面一句提示都没有 |
| 原子替换 | 临时文件写 `before + new + after`，再 rename | 写到一半失败留下半个文件 |
| 不整文入内存 | `copy_range` 用 256KB 定长缓冲搬字节 | GB 级文件写回时先吃掉一份完整副本 |

块内行尾跟随原块（原块里出现 CRLF 就写 CRLF），首块带 BOM 就继续带。
写完重建索引（块大小变了，旧偏移全作废）。

### 块索引（service/big_file.rs）

- **1MB 一块**，边界一律落在换行字节上：受支持编码的换行（0x0A）都不可能是
  多字节序列的后续字节，按它切块不会把字符切半。连续无换行的病态文件在
  1MB+4MB 处硬切（UTF-16 对齐到码元边界）。
- 索引记录每块 `{offset, byte_len, line_start}`；块内换行数在扫描期累计进
  `line_start`，闭块后不单独持有。
- 建索引 = 顺序 `BufReader` 扫一遍，每 8MB 回报一次进度
  （`bigfile:progress` 事件），命令层放 `spawn_blocking`，主线程不付这笔钱。
  用户可取消（`big_file_cancel`，取消位按路径登记）。
- **编码只看头部 64KB 采样**，整份文件按这一个编码逐块解码（与编辑器
  行为一致）；解码失败退有损，给替换符不给「块打不开」。
- 索引缓存在 `BigFileService` 内存表（path → 索引），标签关闭
  （`big_file_close`）时释放。读块前比对 mtime，变了自动重建并报 `stale`。

### 命令（6 条）

| 命令 | 职责 |
|---|---|
| `big_file_open` | 建索引，返回 `BigFileMetaVO`（块数 / 总行数 / 编码） |
| `big_file_block` | 读第 N 块，返回文本 + 全局起始行 + stale 标记 |
| `big_file_goto_line` | 全局行号 → 块下标（partition_point 二分） |
| `big_file_cancel` | 置取消位 |
| `big_file_close` | 释放缓存索引 |
| `big_file_write_block` | 单块写回（乐观锁 + 原子拼接） |

### 前端（ChunkedViewer.vue）

- 分块标签带 `chunked` 标记（运行时字段，不进会话契约）；EditorArea 优先路由
  到 ChunkedViewer，Minimap 与统计读不到注册表条目时自动降级。
- 块导航条：上一块 / 下一块 / 跳转行（行号先二分到块，再把光标落到块内对应行）。
- 光标换算成全局行号写回标签元数据：块内行 + 块起始行 - 1。
- 会话恢复的大文件标签不持久化 chunked 标记：挂载时 CodeEditor 读文件报
  `1004` 即转分块（自动降级），恢复链路不需要提前 stat。
- **编辑默认关闭**：「编辑本块」解锁当前块（修改 compartment 重配），
  区块变脏时标 `blockDirty`；切块 / 退只读前若未保存，先弹确认。
  Ctrl+S 只保存当前块，与整篇保存完全分开。
- 截断标签（`truncated`，只读了前段）一律只读：保存会把剩余部分删掉，这条护栏沿用 specs/05。
- 外部变更：分块标签不做全量重读，读块时后端按 mtime 报 stale 提示；
  写入时后端做字节级乐观锁，过期报 `1008` 并自动重载该块。

### 取舍

| 选择 | 优势 | 代价 |
|---|---|---|
| std `File + Seek` 分块读，不引 mmap crate | 零新依赖；按需读 1MB 与 mmap 等价 | 每块一次系统调用（微秒级，不可感） |
| 块边界只对齐换行，不对齐编码序列 | 算法一句话说清 | UTF-16 硬切处可能出现一个替换符（病态文件才触发） |
| 只解锁单块编辑，不做全篇编辑 | 写回只替换一个字节区间，数据风险面最小 | 跨块改写要一块一块改（大文件本来不该全篇改） |
| 整文件单一编码 | 与编辑器行为一致，块间不会换编码 | 头部 ASCII 尾部 GBK 的混合文件显示为替换符（强制编码重开可解） |

## 表格两条链路

| 格式 | 编辑器 | 保存路径 |
|---|---|---|
| CSV / TSV ≤ 32MB | `SheetEditor.vue` 文本网格 | 序列化成文本 → `write_file`（原子写） |
| CSV / TSV > 32MB | `DocPreview.vue` 只读切行预览 | 不开放 |
| XLSX / XLSM / ODS | `WorkbookEditor.vue` 单元格网格 | `save_workbook` 单元格补丁 |
| XLS（未实现/旧格式） | `DocPreview.vue` 只读预览 | 不开放 |

两条链路的网格都带**表内查找 / 替换**：

- CSV / TSV：前端已全量在内存，扫描不付 IPC 代价；替换只改命中格。
- XLSX / ODS：走 `find_in_workbook`（含跨表定位），替换只对当前表生效——
  跨表无差别替换容易改错，让用户切到那张表再替换一次更安全；公式格跳过。

### XLSX 写回：为什么是「原地改 XML」

整本反序列化再重新生成工作簿，会把解析链不认识的部分全丢掉：样式表、主题、条件格式、
图表、图片、数据校验。用户看到的是「改一个格，颜色全没了」。

所以 `service/sheet.rs` 走外科手术（契约在 `domain/sheet.rs`，入口在 `commands/sheet.rs`）：

```
读：xl/workbook.xml + rels → 表名与部件路径 → sharedStrings.xml → 逐表解析 <c>
写：解压 → 只替换目标表里被点到的那几个 <c> → 其余部件字节级原样搬回
```

| 守护 | 实现 |
|---|---|
| 样式不丢 | 改值时保留原 `<c>` 上的 `s=`（样式索引）；`styles.xml` 整块原样搬运 |
| 未编辑的表不动 | 只重写命中的 `sheetN.xml`，其余工作表字节级原样 |
| 共享串追加 | 新文本追加进 `sharedStrings.xml` 并重写该部件；已有下标不变 |
| 备份 | 写前把原文件复制成 `.bak`，用户可回退 |

守护测试：`sheet_edit_preserves_styles_and_finds_across_sheets`
（手搭一个带 `s=` 的 xlsx，改完断言 `styles.xml` 与未编辑工作表字节级不变）。

## CSV / TSV 可编辑（SheetEditor.vue）

### 范围

- **CSV / TSV ≤ 32MB**：可编辑网格（全量读入内存）。
- CSV / TSV > 32MB：只读预览（DocPreview 切行链路，不受编辑限制）。
- XLSX / ODS 走另一条链路（见上一节），不在这里处理。

### 设计

- 解析 / 序列化在前端，与后端 `parse_delimited` 同一套规则：引号包裹、
  `""` 转义、字段内换行；分隔符嗅探（前 10 行计数，候选 `,` `\t` `;` `|`）。
- **写回最小引号化**：只有含分隔符 / 引号 / 换行的字段才包引号——除有意编辑外
  往返字节一致。
- 虚拟滚动（复用 `useVirtualList`，26px 定高行）：30 万行的 CSV 只渲染窗口内
  行；行号数组只在行数变化时重建，每次单元格编辑不触发滚动归零。
- 编辑交互：点击进入单元格输入，Enter 提交、Tab 提交并右移、Esc 取消；
  工具栏加行 / 删行 / 加列 / 删列。脏标记在提交时算一次，不逐键算。
- **保存走通用链路**：registry 新增「文本适配器」（`registerTextProvider`），
  SheetEditor 把序列化文本暴露成 `getText()`，保存 / 另存为 / 退出确认 /
  崩溃快照全部复用 CodeMirror 的既有链路，后端零改动（`write_file` +
  原子写 + 自写窗口护栏已就绪）。
- 行尾整理（去行尾空白 / 补末行换行）按用户配置照常生效；预览开关
  （`showing_preview`）在网格与只读预览之间切换，与 Markdown 三态同一套语义。

### 取舍

| 选择 | 优势 | 代价 |
|---|---|---|
| 前端解析序列化，不引 CSV 库 | 与后端同规则、零依赖、序列化可控 | 引号边界要两处同步维护 |
| 均匀列宽（180px） | 布局一行代码 | 长字段靠悬停滚动看全 |
| 适配器接入保存链路，不造第二套保存 | Ctrl+S / 退出确认 / 快照免费获得 | registry 多一种条目形态 |
| 查找 / 替换在内存里扫 | 零 IPC 代价，改完立即看得到 | 30 万行的全表扫描在主线程（约 10ms 级，可接受） |

## 崩溃恢复

### 机制

| 环节 | 实现 |
|---|---|
| 标记 | `AppConfig.last_exit_clean`：启动（`app/lifecycle.rs on_ready`）置 false，干净退出（`mark_clean_exit` 命令，前端在 `forceQuit` 前调）置 true |
| 快照 | `useCrashGuard`：每 30 秒把**脏标签**正文静默快照进草稿库（`saveDraft`），内容没变不重写；>2MB 跳过（草稿库上限同值）；分块 / 截断标签跳过（没有完整正文） |
| 识别 | 启动后 `last_exit_clean === false` 且草稿非空 → askPick 提示「打开草稿箱 / 忽略」 |
| 恢复 | 草稿箱（DraftsDialog）逐条决定；快照只进编辑器不落盘，**文件永远不被自动覆盖** |

### 与「源文件旁交换文件」方案的取舍

快照进草稿库而非源文件旁的交换文件：不污染用户目录、天然享受草稿箱 UI 与
备份导出。代价是恢复要用户手动保存一次——这是刻意的：替用户覆盖文件
本身就是一次数据风险。

### 取舍

| 选择 | 优势 | 代价 |
|---|---|---|
| 30 秒间隔快照 | 崩溃窗口最多丢 30 秒 | 每 30 秒一次草稿表写入（脏标签通常 0~3 个，毫秒级） |
| 快照进草稿库 | 零新存储形态，草稿箱 UI 直接复用 | 恢复后要手动保存一次 |

## 约束

- 大文件阈值常量前后端两两同值（32MB / 512MB），改一边必须同步另一边；
  分块上限 `MAX_CHUNKED_FILE`（64GB）只在后端。
- 崩溃快照上限 2MB 与 `repo/draft.rs::MAX_DRAFT_BYTES` 同值。
- CSV 编辑上限 32MB（全量读入），不与分块模式混用。
- 单表读取上限 20 万单元格（`MAX_CELLS_PER_SHEET`）；5 万条查找结果封顶。
- XLSX 写回只改单元格值与公式；合并单元格 / 列宽 / 行高不在编辑范围内
  （读出来供展示，写回时原样保留）。
- 错误码 `1007 BIG_INDEX_CANCELLED` / `1008 FILE_STALE`：`pkg/error.rs`、
  `assets/specs/15-error-codes.md`、前端 `errCode` / `next` 三处同步。
- 崩溃恢复标记在 `forceQuit` **之前**落库：退出流程中断在标记之前，
  下次启动会多问一次恢复——宁可多问，不可丢内容。

## 关键流程

```
打开 >512MB 文件
  openPath → 大小 > 512MB → openChunked → ChunkedViewer 挂载
    → big_file_open（spawn_blocking 扫描，进度事件，可取消）
    → big_file_block(0) → CM 只读视图渲染
跳转行 N
  big_file_goto_line(N) → 二分到块 → loadBlock(idx) → 光标落块内对应行
改一个块
  点「编辑本块」→ 解锁只读 → Ctrl+S 或「保存本块」
    → big_file_write_block（乐观锁比对 → before+new+after → rename）
    → 重建索引 → 块数 / 总行数刷新
CSV 保存
  单元格提交 → rows 更新 → Ctrl+S → saveTabById
    → textForSave → getText（适配器）→ trim 整理 → write_file（原子写）
XLSX 保存
  单元格提交 → pending 累积 → 点「保存工作簿」
    → save_workbook（补丁目标 sheet XML → 重建 zip → .bak 备份 → rename）
崩溃恢复
  异常终止（last_exit_clean 停在 false）
  → 下次启动 → 恢复会话 → 检测 false + 草稿非空 → askPick → 草稿箱
```
