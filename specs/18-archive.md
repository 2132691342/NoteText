# 18 · 归档 / 压缩

## 定位

压缩子系统：**格式真相在 `domain/archive_format.rs`，引擎在
`service/archive/`，任务队列负责进度与取消**。目标是对齐主流压缩工具的用户
路径——创建 / 打开浏览 / 解压 / 测试 / 转换 / 条目增删 / 分卷合并——
而不是对齐它们的全部格式清单；本期支持 10 种格式：

| 格式 | 容器 | 加密 | 引擎 |
|---|---|---|---|
| 7z | ✓ | AES-256 + 可加密文件名 | `sevenz-rust`（LZMA2） |
| zip | ✓ | AES-256 | `zip`（Deflate / BZip2 / Zstd / Store） |
| tar / tgz | ✓ | — | `tar` + `flate2` |
| gz / bz2 / zst / xz / lz4 / br | 单文件 | — | `flate2` / `bzip2` / `zstd` / `xz2` / `lz4_flex` / `brotli` |

不引入外部进程：拉起外部压缩器 exe 的方案在桌面应用里要处理路径与
版本漂移，Rust 原生库编译期锁行为。代价是 7z 只能写 LZMA2（`sevenz-rust`
不提供其余编码器），rar / lzh 这类无开放实现的格式读不了。

## 分层

```
commands/archive.rs   invoke 入口：同步三件（open / preview / formats）+ 长任务八件（提交队列）
service/archive/
├── mod.rs            ArchiveService：编排入口，事件 sink 注入点
├── queue.rs          TaskManager：并发 2、250ms 心跳、暂停 / 取消、事件推发
├── ctx.rs            TaskCtx：进度 / 当前文件 / 速度 / 取消检查，引擎唯一可触的状态
├── create.rs         7z / zip / tar 系 / 单文件压缩系写入器
├── extract.rs        解压（条目过滤、覆盖三策略、防穿越）
├── list.rs           打开归档：条目表 + 魔数探测 + tar 流识别
├── update.rs         追加 / 删除条目（重建式，仅 7z / zip）
├── convert.rs        格式转换（解到内存条目再按目标格式写）
├── split.rs          文件分卷 / 合并
├── preview.rs        条目预览（文本 / 图片二选一）
├── test_archive.rs   完整性测试
└── util.rs           格式探测 / glob 排除 / 条目名净化 / 库错误 → AppError
domain/archive*.rs    REQ / VO / 格式枚举 / 任务快照（一个聚合一段，DO/REQ/VO 同居）
```

依赖方向照旧（AGENTS §2.2）：引擎不碰 Tauri。事件经 `TaskManager.attach_sink`
注入的回调发出，`lib.rs` 的 setup 把它接到 `AppHandle::emit`——
所以全部链路都能在 `cargo test` 里不拉起 WebView 地跑。

## 任务队列

| 维度 | 取值 | 理由 |
|---|---|---|
| 并发上限 | 2 | 压缩是 CPU 密集活，全并行让每个都变慢，磁盘也抖 |
| 进度推送 | 250ms 心跳 + 状态变化即推 | 更快前端渲染跟不上，更慢进度条一顿一顿 |
| 状态机 | queued → running ⇄ paused → done / failed / cancelled | 暂停恢复在 ctx 检查点落地，不中断线程 |
| 取消 | ctx 检查点轮询 | 取消不是杀线程：写一半的文件由引擎自己收尾删除 |
| 收尾 | 终态时 progress 收敛到 1，速度定格 | 前端不必处理「完成后还在转圈」 |

长任务**只回任务 id**；`CreateArchiveVO` 等结构化结果作为
`archive:task-finished` 的 `result` 负载到达。同步命令只有三个：
`open_archive` / `preview_archive_entry` / `get_archive_formats`——
打开与预览要么快、要么需要立刻把 9002 递回给用户输密码，进队列反而绕。

### 暂停 / 取消的检查点粒度

`TaskCtx.check()` 在**每个条目 / 每块缓冲**之间被调用（创建逐条目、解压逐条目、
单文件流按 256KB 块）。检查点足够密，取消延迟 ≤ 一块；又不至于每字节检查，
开销可忽略。暂停即 `check()` 里等一个 Condvar。

## 关键设计

### 格式探测：魔数优先，扩展名兜底

`util::detect_format` 先读前 512 字节走 `ArchiveFormat::from_magic`，
命中压缩流（gz / zst / bz2 / xz）时再探 512 字节里有没有 `ustar` 标记——
**`.tar.gz` 必须识别成 tgz 容器而不是 gz 单文件**，否则用户打开看到的是
一个叫 `archive.tar` 的条目而不是目录树。魔数认不出再退扩展名。
tar 本身魔数在偏移 257，单列判断。

### 条目名净化（解压安全底线）

`sanitize_entry_name` 拒绝绝对路径、盘符、`..` 段——zip 系目录穿越
（Zip Slip）是压缩工具最经典的漏洞，归档里来的一律当不可信输入。
守护测试：`archive_extract_rejects_path_traversal`（手工构造带恶意条目名的 zip）。

### 密码三态

| 场景 | 码 | 前端动作 |
|---|---|---|
| 没给密码，包有加密内容 | 9002 | 浏览浮窗内出现密码行 |
| 给了密码但不对 | 9003 | 同上，提示检查大小写 |
| 打开了但写密码错 | 9003 | 同上 |

分流在 `util::err_sevenz` / `err_zip` / `password_error`：库错误类型 +
「是否已给过密码」共同决定码。**打开失败不关浮窗**——用户输完密码原窗重试，
不用从头再点一遍。

### 覆盖三策略

解压时目标文件已存在：`overwrite`（直接盖）/ `skip`（跳过并计数）/
`rename`（落成 `name (1).ext`，`unique_path` 生成）。结果里的
`skipped` 只数 skip 命中的条目——「部分完成」必须可见。

### 追加 / 删除是重建式

`update.rs` 不做原地改写：读出全部条目 → 加 / 删 → 整包重写。
原地追加 zip 在 central directory 层面要处理偏移重定位，7z 没有公开的
增量 API；重建慢一些但语义简单且总能成功。只对 7z / zip 开放，
tar 系追加要重写整条流，收益为负。

### 分卷

两层含义分开：**归档分卷**（`CreateArchiveReq.split_to`，压缩完按大小切）
与**任意文件分卷**（`split_archive_file` / `join_archive_parts`，对任何文件可用）。
卷名 `.001` 起三数字编号；打开 `.001` 时若存在后续卷，
`OpenArchiveVO.parts` 把全序列给前端，浏览浮窗的增删解压都落在首卷。

## 前端

| 入口 | 位置 |
|---|---|
| 工具菜单「压缩 / 解压…」 | `tools.archive`（Ctrl+Alt+A），预填当前标签 |
| 文件树右键 | 压缩包节点给打开 / 解压到 / 测试；`.001` 给合并；其余给添加到压缩包 / 文件分卷 |
| 压缩包浏览浮窗 | `ArchiveBrowserDialog`：条目表 / 增删 / 解压 / 测试 / 转换 / 预览 |
| 任务面板 | `ArchiveTasksDialog`（Ctrl+Alt+T）+ 状态栏活动任务指示 |
| 系统关联双击 | 关联 .zip 等之后双击 / 拖到 exe：`useShellIntent` 分流到浏览浮窗（specs/17） |

事件驱动不轮询：`stores/archive.ts` 订阅两个事件维护任务快照，队列 250ms
心跳推全量。格式下拉、能力徽标（加密 / 固实 / 分卷）全部来自
`get_archive_formats` 与 `OpenArchiveVO`，前端不写死格式清单。

## 约束

- 单文件压缩格式（gz / bz2 / zst / xz / lz4 / br）一次只接受一个文件源，多源报 9005
- 追加 / 删除仅 7z / zip；tar 系与单文件格式不开放
- 条目预览 256KB 上限（文本截断显示，图片 > 8MB 不内嵌）
- 7z「仅存储」档映射到 LZMA2 最低预设（库不支持纯存储）
- zip 的 Zstd / BZip2 方法与 AES 加密别的工具读得上，但老解压器可能不认——
  创建对话框默认仍给 Deflate
- 归档条目的 `name` 统一 `/` 分隔、目录不带尾斜杠；跨边界时间一律毫秒整数

## 取舍

| 选择 | 优势 | 代价 |
|---|---|---|
| Rust 原生库而非拉起 7z.exe | 无外部依赖、行为编译期锁定、测试可跑 | rar 读不了；7z 只能写 LZMA2 |
| 长任务队列 + 事件推送 | 界面不冻结，多任务并行，进度真实 | 前后端多一套任务协议（两个事件 + 任务快照） |
| 追加 / 删除重建式实现 | 语义简单、永不产生损坏的中间态 | 大包改动要整包重写一次 |
| 同步 open + 队列化其余 | 打不开 / 要密码立刻知道，不用去任务面板找 | 两条路径并存，前端要分流处理 |
| 魔数优先探测 | 改了扩展名的包照样打得开 | tar 流要多读一次头（512B，可忽略） |
