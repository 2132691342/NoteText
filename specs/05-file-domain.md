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
  ├─ encoding::detect(&bytes)      BOM 优先 → 合法 UTF-8 → GBK/Big5/SJIS 特征 → latin 兜底
  ├─ encoding::strip_bom           记录 has_bom，剥掉再解
  ├─ encoding::decode
  └─ normalize_newlines            行尾归一成 LF 供编辑器算行列
```

**为什么行尾在探测之后归一**：`detect_line_ending` 要看原始字节判 CRLF/LF，
而归一后的文本才交给编辑器算行列（混用行尾会让列号随行尾漂移）。

**大文件分模式**：`MAX_FULL_READ = 32MB`。超了不静默全量读入
（会把 WebView 卡死），前端在 `open_path` 时先 `stat_file` 看大小，
超限自动改用 `ReadMode::Head`（只读前 32MB），**不弹选择框**。

降级后的标签有三重保护：

| 保护 | 落点 |
|---|---|
| 标记截断 | `tab.truncated = true`，状态栏显示「已截断」并带 hover 提示 |
| 强制只读 | `tab.read_only = true`，编辑器不接收输入 |
| 拦截写回 | `save_tab` 见到 `truncated` 直接拒绝并提示，不让半个文件覆盖原文件 |

**为什么不让用户选**：超限时用户想要的是「打开看内容」，不是理解三种读取模式的差别。
自动降级一步到位，而截断状态在状态栏可见——真想完整编辑会用外部程序。

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
  3) 合法 UTF-8 → utf-8
  4) GB18030 特征（双字节首 0x81–0xFE、次字节合法区间）→ gb18030
  5) Big5 特征 → big5
  6) Shift_JIS 特征（半角片假名占比）→ shift_jis
  7) 兜底 windows-1252 + low_confidence = true
```

**`low_confidence` 时前端必须提示「不确定」**：`FileContentVO.truncated` 是
「被截断**或**置信度低」的或语义，状态栏的编码名后带 `?`。
让用户手选比报错好——报错等于「打不开」，而大多数情况用户知道这是什么编码。

### 编码表是手写的子集

| 编码 | 实现 |
|---|---|
| utf-8 / utf-8-bom | 标准库 |
| utf-16le / utf-16be | 手写 `encode_utf16` / `decode_utf16` |
| windows-1252 / iso-8859-1 | 单字节表（32 个高位 + 0xA0–0xFF 直映） |
| gb18030 / big5 / shift_jis | 双字节表（常用区子集） |
| euc-jp / euc-kr / koi8-r | 只做 ASCII 直通，高位字节落 Latin-1 区（U+0080–U+00FF） |

**表里没有的字符解成 U+FFFD / 编成 `?`**：宁可显式问号，不要静默丢字。

**要扩展编码清单时优先换 `encoding_rs`**，别继续往表里加——
半套码表比没有更容易让人误判。这是写进 [AGENTS.md §1.1](../AGENTS.md) 的约定。

### 目录树

```
read_dir(dir)
  ├─ 不是目录 → FILE_NOT_FOUND
  ├─ 应用 ignore_patterns（按名字精确匹配）
  ├─ 权限不足的项跳过（不是整棵目录读失败）
  └─ 排序：目录在前，名称不区分大小写
```

**一层不递归**：不递归算总量，用户展开 `.git` 才知道代价就晚了。

**忽略规则来自配置**：`FileConfig.ignore_patterns` 默认
`node_modules` / `.git` / `target` / `dist`。匹配按名字精确相等，
不解析通配符——常见忽略目标都是固定的构建产物名，精确匹配可预期；
按扩展名过滤的场景交给查找工具的 glob。

### 文件监控

```
watch_dir(dir, on_change)
  ├─ 一个 RecommendedWatcher 复用（每次新建会让 inotify 句柄无限增长）
  ├─ 只报 is_modify / is_create / is_remove（访问时间之类的噪音丢掉）
  └─ 变更经 commands 层注入的回调转发 → emit('fs:changed', paths)
```

**监控失败不阻断启动**：编辑器核心功能不依赖它，失败只让侧栏的监控面板不可用。

## 约束

- **文本与二进制都走 invoke**：文本出参是 `String`；二进制的 `Vec<u8>` 会
  序列化成 `number[]`（体积约 4 倍），所以 `read_binary` 出参是 base64，
  并带 20MB 上限。图片 / PDF / docx / Hex 都用它——不给渲染层 fs 插件权限；
- 空文件按 UTF-8，`has_bom = false`，`line_ending = "lf"`；
- `stat_file` 的 `is_binary` 判据是「前 8KB 含 NUL」：UTF-16 文本也含 NUL，
  但编辑器用 UTF-16 打开的场景远少于真二进制文件，误判代价（用户手动切编码）可接受。

## 取舍

**不引 chardet 之类的统计探测库**：常见中文编码的判定用「高位字节序列合法」
+「是否含典型中文字符」已够用，且**可解释**（能告诉用户为什么这么判）。
统计库在短文本上误判率高，而编辑器面对的常是几行的配置片段。
代价是冷门编码判不出——那种情况置 `low_confidence` 让用户手选。

## 文件操作（`src-tauri/src/service/file_ops.rs`）

供给文件树右键菜单与树内拖拽。与 `batch` 域的分工：那边是「扫一批按规则改名」，
这边是「用户对一个节点下了一个明确指令」。

```
validate_target(from, to)         三条硬约束的统一入口
  ├─ 目标已存在            → 拒绝（不覆盖）
  ├─ 目录进自己的子目录     → 拒绝（树会成环，文件树读到尽头）
  └─ 路径非绝对 / 含 ..     → 拒绝（判据依赖隐式工作目录，不可复现）
```

**逐条记账不整体中止**（`remove` / `move_to` / `copy_to`）：用户勾了 10 个文件，
3 个被占用，该删的 7 个要删掉。整体回滚会让一次误选把「已经删掉 7 个」
变成「一个都没删」，用户以为没删就又点一次。

**`auto_suffix`**：移动 / 复制时同名默认拒绝；勾选后落到 `name (2).ext`
起的序号名。校验要在算序号名**之前**做——先校验会把 auto_suffix 这条路堵死。

**`is_within` 对不存在的目标也能判**：逐级往上找最近的已存在祖先再 canonicalize，
把不存在的尾巴接回去。直接拿字面路径比会让 Windows 上 `\?\C:\...` 与 `C:\...`
前缀不等，判成越界。

**复制不引 walkdir crate**：递归复制二十行能写完（AGENTS.md §1.1）。
