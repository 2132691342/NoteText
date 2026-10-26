# 18 · 归档 / 压缩

## 定位

压缩子系统：**格式真相在 `domain/archive_format.rs`，引擎在
`service/archive/`，任务队列负责进度与取消**。目标是对齐主流压缩工具的用户
路径——创建 / 打开浏览 / 解压 / 测试 / 转换 / 条目增删 / 分卷合并——
而不是对齐它们的全部格式清单；本期支持 15 种格式：

| 格式 | 容器 | 加密 | 引擎 |
|---|---|---|---|
| 7z | ✓ | AES-256 + 可加密文件名 | `sevenz-rust`（LZMA2） |
| zip | ✓ | AES-256 | `zip`（Deflate / BZip2 / Zstd / Store 写；Deflate64 / LZMA 也能读） |
| tar / tgz / txz / tbz2 / tzst / tlz4 / tbr | ✓ | — | `tar` + 六种流编码器（组合后缀全家族） |
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
前两者标了 `#[tauri::command(async)]`：Tauri 的同步命令在**主线程**执行，
大包列条目 / 固实 7z 预览解整块会把整个窗口冻住；标 async 后仍在 worker
线程池跑、立刻返回，语义不变（打开失败还是同步报错）。

### 暂停 / 取消的检查点粒度

`TaskCtx.check()` 在**每个条目 / 每块缓冲**之间被调用（创建逐条目、
解压逐条目、流式数据统一走 `copy_with_ctx` 按 256KB 块）。检查点足够密，
取消延迟 ≤ 一块；又不至于每字节检查，开销可忽略。暂停即 `check()` 里等一个
Condvar。zip 单条目几 GB 的文件同样按块过检查点——`io::copy` 一口气搬完
的写法会让取消按不到、进度条僵死。

## 关键设计

### 格式探测：魔数优先，扩展名兜底

`util::detect_format` 先读前 512 字节走 `ArchiveFormat::from_magic`，
命中流编码（gz / zst / bz2 / xz / lz4）时再解出前 512 字节探
`ustar` 标记——**包着 tar 的压缩流必须归一为 tar 家族变体**（`.tar.xz` →
txz），否则用户打开看到的是一个叫 `archive.tar` 的条目而不是目录树。
brotli 没有魔数，`.tar.br` 走扩展名路径后再探 tar，两条路都通。
魔数认不出再退扩展名。tar 本身魔数在偏移 257，单列判断。

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

### 转换保留源包属性

`convert_archive` 先 `list_archive` 读一次源包，加密、加密文件名、固实三项跟随源包。
源包有密码而 `new_password` 为空时**直接拒绝**——静默把加密包转成明文包是安全降级，
用户会以为它还是加密的。分卷大小不继承（源包的卷大小可读性差），转出即单文件。

### 分卷

两层含义分开：**归档分卷**（`CreateArchiveReq.split_to`，压缩完按大小切）
与**任意文件分卷**（`split_archive_file` / `join_archive_parts`，对任何文件可用）。
卷名 `.001` 起三数字编号；打开 `.001` 时若存在后续卷，
`OpenArchiveVO.parts` 把全序列给前端，浏览浮窗的增删解压都落在首卷。

## 前端

| 入口 | 位置 |
|---|---|
| 顶级「压缩」菜单 | 六条：新建压缩包 `archive.create`（Ctrl+Alt+A，预填当前标签）/ 解压压缩包 `archive.extract`（Ctrl+Alt+X，选包进解压浮窗）/ 打开浏览 / 测试 / 文件分卷与合并 / 压缩任务 `archive.tasks`（Ctrl+Alt+T） |
| 解压浮窗 | `ExtractArchiveDialog`：选包 → 条目勾选（默认全解）+ 密码行 → 目录 / 覆盖策略 / 同名子文件夹 → 解压或测试 |
| 文件树右键 | 压缩包节点给打开 / 解压到…（进解压浮窗）/ 解压到同名文件夹 / 测试；`.001` 给合并；其余给添加到压缩包 / 文件分卷；空白处给新建 / 解压 |
| 工具栏「更多」 | 新建压缩包 / 解压压缩包 / 压缩任务三条 |
| 压缩包浏览浮窗 | `ArchiveBrowserDialog`：条目表 / 增删 / 解压 / 测试 / 转换 / 预览 |
| 任务面板 | `ArchiveTasksDialog`（Ctrl+Alt+T）+ 状态栏活动任务指示 |
| 系统关联双击 | 关联 .zip 等之后双击 / 拖到 exe：`useShellIntent` 分流到浏览浮窗（specs/17） |

浮窗内的即时反馈与批量动作：

| 动作 | 落点 |
|---|---|
| 解压 / 测试 / 创建提交后 | 浮窗内出现进度行（当前条目 + 百分比 + 取消），任务面板不必再切一次；完成自动关窗 |
| 批量解压 | 解压浮窗「批量解压…」多选包（`open_files_dialog`），每包各自 `create_subdir` 解到同名文件夹，全部进队列 |
| 拖放添加 | 创建浮窗的源清单可直接接资源管理器拖入（`.stop` 阻止 window 级拖放把文件开成标签），拿不到路径时提示改用「添加文件…」 |
| 专有归档（rar / lzh / cab / iso） | 不作为二进制丢进 Hex：预览器给「用 7-Zip 解压后再打开」的卡片 + 外部程序入口 |

事件驱动不轮询：`stores/archive.ts` 订阅两个事件维护任务快照，队列 250ms
心跳推全量。格式下拉、能力徽标（加密 / 固实 / 分卷）全部来自
`get_archive_formats` 与 `OpenArchiveVO`，前端不写死格式清单。

条目表是**定高虚拟列表**（`composables/useVirtualList.ts` + `base.css`
的 `.nt-vlist-*` 三件套）：几万条目也只渲染滚动窗口内的几十行，
`base.css` 的占位块撑滚动高度、窗口层 transform 平移。全量 `v-for`
在十万条目级会把 WebView2 渲染线程打满——打开浮窗即冻结，这是
2.0 早期「解压前卡死」的另一半根因（前一半是同步命令占主线程）。
打开 / 预览期间给文案态（`archive.opening` / `archive.previewLoading`），
固实 7z 预览单条目要解整块，没有反馈会被当成死机。

## 约束

- 单文件压缩格式（gz / bz2 / zst / xz / lz4 / br）一次只接受一个文件源，多源报 9005
- tar 家族组合格式（tgz / txz / tbz2 / tzst / tlz4 / tbr）与纯 tar 走同一条
  流式管线，压缩层固定、不可换方法；`ArchiveFormat::all()` 是格式清单的唯一来源
- zstd 压缩开多线程（`zstdmt` 特性 + 按核数给 worker），解压单线程足够
- 追加 / 删除仅 7z / zip；tar 系与单文件格式不开放
- 分卷上限 9999 卷（`split_file` 与 `split_parts` 同值，不一致会出现「假单卷」）
- 转换：源包加密时新密码必填，否则 9005 拒绝；固实 / 加密文件名跟随源包
- 条目预览 256KB 上限（文本截断显示，图片 > 8MB 不内嵌）
- 7z「仅存储」档映射到 LZMA2 最低预设（库不支持纯存储）
- zip 的 Zstd / BZip2 方法与 AES 加密别的工具读得上，但老解压器可能不认——
  创建对话框默认仍给 Deflate
- 归档条目的 `name` 统一 `/` 分隔、目录不带尾斜杠；跨边界时间一律毫秒整数

## 取舍

| 选择 | 优势 | 代价 |
|---|---|---|
| Rust 原生库而非拉起 7z.exe | 无外部依赖、行为编译期锁定、测试可跑 | rar / lzh 读不了（专有算法无开源实现），预览器给出 7-Zip 指引；7z 只能写 LZMA2 |
| 长任务队列 + 事件推送 | 界面不冻结，多任务并行，进度真实 | 前后端多一套任务协议（两个事件 + 任务快照） |
| open / preview 标 async + 条目表虚拟滚动 | 大包打开与固实块预览不冻窗 | 打开 / 预览是异步等待，要 loading 文案兜底；虚拟列表要求行定高 |
| 追加 / 删除重建式实现 | 语义简单、永不产生损坏的中间态 | 大包改动要整包重写一次 |
| 同步 open + 队列化其余 | 打不开 / 要密码立刻知道，不用去任务面板找 | 两条路径并存，前端要分流处理 |
| 魔数优先探测 | 改了扩展名的包照样打得开 | tar 流要多读一次头（512B，可忽略） |
| 条目表用扁平虚拟列表而非目录树 | 几万条目只渲染可视行，配合「搜索条目名」定位比逐层展开快 | 没有层级折叠；想看目录结构要解压出来 |
