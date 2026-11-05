# 05 · 文件域

## 定位

`src-tauri/src/service/file.rs`（读写 / 目录树 / 原子写 / 监控）+
`src-tauri/src/service/encoding.rs`（编码探测 / 解码 / 编码 / BOM）。

这是编辑器最核心的一层：**用户的所有内容都从这里过，出错就是数据丢失**。

## 设计

### 读取

```
read(path, mode)
  ├─ stat：不存在 → FILE_NOT_FOUND；超过上限 → FILE_TOO_LARGE（提示用 head）
  ├─ 按 mode 取字节（Full / Head / Range）
  ├─ encoding::detect(&bytes)      BOM → UTF-16 无 BOM → UTF-8 → 候选编码打分
  ├─ encoding::strip_bom           记录 has_bom，剥掉再解
  ├─ encoding::decode
  └─ normalize_newlines            行尾归一成 LF 供编辑器算行列
```

**为什么行尾在探测之后归一**：`detect_line_ending` 要看原始字节判 CRLF/LF，
而归一后的文本才交给编辑器算行列（混用行尾会让列号随行尾漂移）。

**大文件分模式**：`MAX_FULL_READ = 64MB`，`MAX_FORCE_FULL_READ = 512MB`，前端
`FULL_READ_LIMIT` / `BIG_EDIT_LIMIT` 同值。超了不静默全量读入（会把 WebView 卡死）。

| 文件大小 | 打开方式 | 能力 |
|---|---|---|
| ≤ 64MB | CodeMirror 全功能编辑器 | 编辑 / 保存 / 语法高亮 / 查找替换 / 分屏 / 小地图 |
| 64–512MB | 弹窗选「普通打开（整篇编辑）」或「分块打开」 | 普通打开即整篇可编辑；分块为 1MB 块导航 |
| > 512MB | 自动进分块视图（`service/big_file.rs`） | 块导航 / 跳行 / 单块解锁编辑；最高 64GB |

**表格是另一条链路**：CSV / TSV / JSONL 不受这个分档约束——
它们走记录对齐索引 + 分页取数（specs/19），1000 万行也秒开。

Head 模式保留给截断重载等内部场景；分块模式的块索引、逐块解码与单块写回见 **specs/19**。

| 保护 | 落点 |
|---|---|
| 标记截断 | `tab.truncated`，视图给出明确的只读标识 |
| 强制只读 | `tab.read_only = true`，编辑器不接收输入 |
| 拦截写回 | `save_tab` 见到 `truncated` 直接拒绝并提示，不让半个文件覆盖原文件 |

分块（`tab.chunked`）是另一回事：它**允许逐块编辑**，每个写回都带字节级乐观锁
（`big_file_write_block`），不经过整篇写盘链路。

**为什么 32–512MB 还给选择**：这个区间的文件「整篇编辑」仍有价值（配置导出、
数据清洗），代价只是打开慢几秒；512MB 以上整篇进 CodeMirror 已经不可接受，
替用户决定转分块。
### 大文件不卡的三条硬约束

大文件「能打开」只是第一步，「打开后还能用」靠这三条：

| 位置 | 禁止的写法 | 正确写法 |
|---|---|---|
| 状态栏行数 | `doc.toString().split('
').length`（每次按键复制全文） | `doc.lines`（O(1) 计数） |
| 详细统计（字符 / 词 / 非空行） | 每次按键都过 IPC 算一遍 | 停手 600ms 后由 `nt-detail-stats` 事件问一次 |
| 小地图 | `doc.toString().split('
')`（每次滚动复制全文） | `doc.line(i)` 按需抽样读行 |

这三处都曾是「打开过大文件之后整机发卡」的直接来源：一篇 7MB 的 JSON
每敲一个字就要复制 7MB，滚动一次小地图又要复制一次。三类消费方现在都
只做与文件体积无关的工作——行数走 `doc.lines`，统计攒到停手后，小地图按
`doc.line(i)` 抽样。

**分块视图的三条保护**：`tab.chunked` 为真时整篇写回被拒绝（会把块内容当成全文覆盖），
自动保存跳过它，状态栏标「分块只读」。用户要落盘只能走「编辑本块」→ `big_file_write_block`，
写回前比对磁盘字节，前后段流式拼接。

### 写入

```
write(path, text, encoding?, line_ending?, bom?)
  ├─ 路径校验：空 → PATH_EMPTY；父目录不存在 → FILE_NOT_FOUND（不顺带创建）
  ├─ 行尾在文本层归一（多字节编码里 CR/LF 序列不能按字节判）
  ├─ encoding::encode
  ├─ 补 BOM（用户显式选了才补）
  ├─ atomic_write：同目录临时文件 + rename
  └─ RecentRepo::touch 记账
```

**原子写**：`.{name}.notetext-tmp` + `fs::rename`。
直接 `fs::write` 截断原文件再写，崩在中间就是数据丢失。
必须同目录：跨卷（C: → D:）的 rename 会退化成复制，原子性就没了。
rename 失败时清掉临时文件，否则同目录会留下一堆 `.xxx.notetext-tmp`。

**保存按「打开时的编码 + 行尾 + BOM」写回**：用户没主动改编码时，
一次保存不该把整个文件从 GBK 转成 UTF-8。这是编辑器最招人骂的一类 bug——
同事那边 diff 出几千行。

### 编码探测

```
detect(bytes) -> (encoding, low_confidence)
  1) BOM 是硬证据：EF BB BF → utf-8-bom；FF FE → utf-16le；FE FF → utf-16be
  2) 空文件 → utf-8（不是 latin 兜底）
  3) UTF-16 无 BOM：NUL 占比 ≥ 20% 且几乎全落同一奇偶位 → utf-16le / utf-16be
  4) UTF-8（末尾截断也算）→ utf-8
  5) 候选编码按解码损失打分：gb18030 → big5 → euc-kr → shift_jis → windows-1252
  6) 损失全不为 0 → 取最小者 + low_confidence = true
```

**第 3 步必须早于第 4 步**：`A\0B\0` 是合法 UTF-8，顺序颠倒会把 UTF-16 判成 UTF-8。

**第 4 步容忍截断**：`Head` 模式必然切在多字节字符中间，
`from_utf8` 的 `error_len() == None`（输入意外结束）仍按 UTF-8 处理——
否则一份 UTF-8 大文件会被截断处的半个字符带偏成 GBK，整篇乱码。

**损失打分（`loss`）**：替换符 ×100、控制字符 ×20、私用区 ×40、罕见区汉字 ×1。
候选顺序即平手时的优先级（中文用户量最大的现实顺序）。

**`low_confidence` 时前端必须提示「不确定」**：`FileContentVO.truncated` 是
「被截断**或**置信度低」的或语义，状态栏的编码名后带 `?`。
让用户手选比报错好——报错等于「打不开」，而大多数情况用户知道这是什么编码。

### 码表来自 `encoding_rs`

| 编码 | 实现 |
|---|---|
| utf-8 / utf-8-bom | `encoding_rs`（严格校验，失败交给上层回退） |
| utf-16le / utf-16be | 解码走 `encoding_rs`；编码自己按码元写（`Encoding::encode` 是表单语义，Unicode 一律吐 UTF-8） |
| gb18030（兼 GBK / GB2312） | `encoding_rs::GB18030` |
| big5 / shift_jis / euc-jp / euc-kr | `encoding_rs` 对应码表 |
| windows-1252 / iso-8859-1 / koi8-r | `encoding_rs` 对应码表 |

**为什么用完整码表而不是手写子集**：手写表只能覆盖标点与少量常用字，
中文正文必然解成 U+FFFD——「打开就乱码」的根因常是「表里没字」，不是探测错。
代价是二进制体积约 +200KB，换来 12 种编码全量正确。

**表里没有的字符解成 U+FFFD / 编成 `?`**：宁可显式问号，不要静默丢字。
`had_errors` 时记一条 warn 日志，便于排查「为什么存回去丢了几个字」。

### 表格预览（CSV / TSV）

```
read_table_preview(path, encoding?) -> TablePreviewVO
  ├─ 读前 8MB（TABLE_HEAD_BYTES），超出部分只统计行数
  ├─ detect + decode：编码判定与文本链路同一条，不让渲染层自己猜
  ├─ 截断时丢掉最后一行（半条记录比少一行更误导）
  ├─ sniff_delimiter：前 10 行里 , / \t / ; / | 出现最多的那个
  ├─ parse_delimited：引号包裹、"" 转义、字段内分隔符与换行
  └─ total_rows：尾部按换行统计（最多再扫 128MB，超了记 0 = 未知）
```

**为什么表格预览走后端切行**：交给前端的表格库要先 `read_binary`（20MB 上限）、
把 base64 解成字节再按 UTF-8 猜编码。
大表被上限挡掉、GBK 表整列乱码——用户看到「文件太大打不开」与
「中文全是乱码」两个症状，根因是同一条链路。

`rows` 上限 5000 行、前端再按 2000 行渲染：再多也没人逐行看，
但 `total_rows` 仍给出来，用户知道表有多大。

### 目录树

```
read_dir(dir)
  ├─ 不是目录 → FILE_NOT_FOUND
  ├─ 应用 ignore_patterns（按名字精确匹配）
  ├─ 权限不足的项跳过（不是整棵目录读失败）
  └─ 排序：目录在前，名称不区分大小写
```

**一层不递归**：不递归算总量，用户展开 `.git` 才知道代价就晚了。

**目录项不嗅探二进制**：`read_dir` 里对每个文件开一次句柄读前 8KB，
在「目录里放了几百 MB 的大文件」时会让整棵树变慢。`DirEntry::is_binary`
恒为 false，真正的判定在打开具体文件时由 `stat_file` 做一次。

**外部变更只刷变化路径的父目录**：`fs:changed` 带来的是具体路径，
`refreshPaths` 按父目录分组重读，不再整棵工作目录递归一遍。
原先的无条件递归会把「保存一次」变成「几十个目录重新 read_dir」。

**忽略规则来自配置**：`FileConfig.ignore_patterns` 默认
`node_modules` / `.git` / `target` / `dist`。匹配按名字精确相等，
不解析通配符——常见忽略目标都是固定的构建产物名，精确匹配可预期；
按扩展名过滤的场景交给查找工具的 glob。

### 文件监控

```
watch_dir(dir, on_change)
  ├─ 一个 RecommendedWatcher 复用（每次新建会让 inotify 句柄无限增长）
  ├─ **非递归（NonRecursive）**：只监听这一层
  ├─ 只报 is_modify / is_create / is_remove（访问时间之类的噪音丢掉）
  └─ 变更经 commands 层注入的回调转发 → emit('fs:changed', paths)
```

**为什么不递归**：递归监听一棵大树（`node_modules` / 构建缓存 / 几十万文件的
数据目录）会让 notify 为每个子目录注册一个系统句柄，Windows 上就是逐个
`OpenDirectory`。按需监听：根目录先监听，子目录在展开时逐个加监听，
折叠时 `unwatch` 释放（前端 `fileTree.toggle` 负责）。

**监控失败不阻断启动**：编辑器核心功能不依赖它，失败只让侧栏的监控面板不可用。

## 约束

- **文本与二进制都走 invoke**：文本出参是 `String`；二进制的 `Vec<u8>` 会
  序列化成 `number[]`（体积约 4 倍），所以 `read_binary` 出参是 base64，
  并带 20MB 上限。图片 / Hex 都用它——不给渲染层 fs 插件权限；
- **CSV / TSV 不走 `read_binary`**：表格预览走 `read_table_preview`，
  按前 8MB 切行，几十 MB 的导出表也能开，且编码由后端判定；
- 空文件按 UTF-8，`has_bom = false`，`line_ending = "lf"`；
- `stat_file` 的 `is_binary` 判据是「前 8KB 含 NUL」，但带 BOM 的 UTF-16
  直接判文本（`FF FE` / `FE FF` 开头）——否则满篇 NUL 会让它被当成二进制；
  无 BOM 的 UTF-16 仍会被误判，这条概率低，代价是用户手动切编码。

## 取舍

**不引 chardet 之类的统计探测库**：码表用 `encoding_rs`，判定仍自己写——
「候选编码各解一遍、数损失」比统计模型**可解释**（能说清为什么这么判），
在几行的配置片段上也不至于像统计库那样凭噪声下结论。
代价是候选清单固定为五种，冷门编码判不出——那种情况置 `low_confidence` 让用户手选。

**大文件另给一条链路，而不是把上限调大**：

| 选择 | 优势 | 代价 |
|---|---|---|
| 64MB 分界 + 分页视图 | 常见日志（几十 MB）仍可编辑；更大的文件内存占用恒定，能跳行能查找 | 两套渲染路径；>64MB 不可编辑，要改只能「另存为副本」 |
| 表格另走记录对齐索引 | 1000 万行的 CSV 也能秒开、改单格、保存 | 表格只能整表重写（不能改一半留一半），XLSX 另有单元格补丁链路 |
| 行首字节偏移索引 | 翻页只读目标区间，任意行号 O(1) 定位 | 首次打开要全文件扫一遍（几百 MB 约 1~2 秒），索引占内存约 8 字节/行 |
| 整块查找而非逐行查找 | 20MB CSV 上一次全量查找从 ~490ms 降到 ~330ms（debug） | 命中要换算行号，块边界那行要留到下一轮 |
| 朴素子串匹配（不引 regex） | 热路径零依赖开销，ASCII 走字节级无分配比较 | 不支持正则；整词判定按 `is_alphanumeric` 简化 |

## 文件操作（`src-tauri/src/service/file_ops.rs`）

供给文件树右键菜单与树内拖拽。与 `batch` 域的分工：那边是「扫一批按规则改名」，
这边是「用户对一个节点下了一个明确指令」。

```
validate_target(from, to)         全部写操作的唯一校验入口
  ├─ 空 / 非绝对 / 含 ..    → 拒绝（判据依赖隐式工作目录，不可复现）
  ├─ 源不存在              → 拒绝（记 1005）
  ├─ 目标与源相同           → 拒绝（大小写不敏感比较）
  ├─ 目标已存在            → 拒绝（不覆盖）
  └─ 目录进自己的子目录     → 拒绝（树会成环，文件树读到尽头）
```

**逐条记账不整体中止**（`remove` / `move_to` / `copy_to`）：用户勾了 10 个文件，
3 个被占用，该删的 7 个要删掉。整体回滚会让一次误选把「已经删掉 7 个」
变成「一个都没删」，用户以为没删就又点一次。

**`auto_suffix`**：移动 / 复制时同名默认拒绝；勾选后落到 `name (2).ext`
起的序号名。**先算出不重名的目标名，再走统一校验**——反过来的话，校验一定
会以「目标已存在」把这选项拦下，勾了等于没勾。

**`is_within` 对不存在的目标也能判**：逐级往上找最近的已存在祖先再 canonicalize，
把不存在的尾巴接回去。直接拿字面路径比会让 Windows 上 `\?\C:\...` 与 `C:\...`
前缀不等，判成越界。

**复制不引 walkdir crate**：递归复制二十行能写完（AGENTS.md §1.1）。
