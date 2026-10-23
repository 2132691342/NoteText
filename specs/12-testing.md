# 12 · 测试与验证

## 定位

`src-tauri/tests/`（按域分文件）+ `scripts/check.ps1`（唯一验证入口）+
`scripts/i18n-audit.mjs`（国际化审计）。规范条文见 [AGENTS.md §3](../AGENTS.md)，
这一篇解释「为什么这么组织」。

## 判据

写之前先问：**这条测试失败时，用户界面上会出现什么坏事？**
说不出那句坏事，它就不该存在。

| 保留 | 不写 |
|---|---|
| 跨模块的长链路（装配 → 建表 → 会话往返 → 序列化契约） | 单个纯函数的输入输出 |
| 协议硬约束（空数组永不 null、行尾按原样写回、BOM 剥离与补回） | 简单 CRUD、字段映射、枚举转换 |
| 安全护栏（批量操作不覆盖别人的文件、目录不进自己的子目录） | 只验「不 panic」「非 nil」的弱断言 |

纯函数断言是最大的一类冗余：函数签名就是契约，编译器挡了一半，
剩下一半坏了也只影响一个工具窗的一个按钮。**护栏类断言失败时对应的都是
「用户的文件没了 / 界面崩白屏」级别的事故**，必须常驻。

## 组织：一个域一个文件，一个测试一种坏法

```
src-tauri/tests/
├── common/mod.rs      临时目录、装配、会话标签样本
├── session_chain.rs   装配与持久化（5 个测试）
├── file_chain.rs      文件域与文件树护栏（8 个测试）
├── tools_chain.rs     工具箱与文本变换（6 个测试）
└── batch_chain.rs     批量操作安全（4 个测试）
```

Rust 集成测试按「测试二进制」分文件，一个域一个文件，编译单元跟着分开：
改某个域只读那个文件。

**测试函数名就是它守护的那件坏事**（`rename_never_overwrites_an_existing_file`）。
这样失败时不需要「拿行号去文件里找第几条断言」——名字本身说明了坏在哪。
一个函数只讲一种坏法：能写成「A、B、C 都对」的函数，失败时报文说不清坏的是哪个。

### 测试索引

| 文件 | 测试 | 守护的坏事 |
|---|---|---|
| `session_chain` | `first_launch_writes_defaults_and_gives_empty_session` | 启动即失败；首次启动没写默认配置 |
| | `session_round_trip_keeps_order_active_index_and_window_size` | 重启后标签顺序与窗口尺寸全丢 |
| | `closing_all_tabs_keeps_the_session_row_for_window_size` | 关光标签后下次启动开成默认大小 |
| | `bookmark_add_dedupes_and_toggle_removes` | 书签重复加出双份（F2 按两下变两条） |
| | `empty_lists_serialize_to_array_not_null` | 空面板崩白屏（空列表必须是 `[]`） |
| `file_chain` | `read_normalizes_crlf_but_remembers_original` | 行尾样式没记住，写回时改变文件 |
| | `write_keeps_original_line_ending_unless_switched` | 一次保存让整个文件行尾变 LF，diff 出几千行 |
| | `atomic_write_leaves_no_temp_file_and_never_creates_missing_dirs` | 临时文件堆积；打错路径在磁盘留痕 |
| | `bom_is_stripped_on_read_and_restored_on_write` | 正文前多三个不可见字节；BOM 丢失 |
| | `forced_encoding_read_decodes_and_falls_back_when_unsupported` | 选了编码打开的仍是乱码 |
| | `rename_never_overwrites_an_existing_file` | 重命名无声销毁别人的文件 |
| | `rename_rejects_relative_paths_and_directory_cycles` | 文件树成环，递归遍历挂死 |
| | `delete_accounts_for_each_path_and_create_refuses_existing` | 一个陈旧路径让整批删除失败 |
| `tools_chain` | `bad_json_reports_line_and_column` | 只说「格式错误」，用户找不到在哪 |
| | `diff_counts_added_and_removed_lines` | 增删计数错，用户照错结果改代码 |
| | `regex_reports_capture_groups_for_every_match` | 正则空匹配不推进，挂死界面 |
| | `hashes_match_known_vectors` | 摘要与系统命令对不上，用户误判文件被改 |
| | `toggle_comment_round_trips_and_fills_half_commented_blocks` | Ctrl+/ 把代码注释坏 |
| | `toggle_comment_rejects_languages_without_line_comments` | 静默返回原文让用户以为生效 |
| `batch_chain` | `scan_reports_without_touching_files` | 扫描阶段就落盘，绕过用户确认 |
| | `apply_replaces_only_the_checked_files` | 改掉用户没勾选的文件 |
| | `rename_conflict_is_flagged_and_cannot_be_applied` | 重命名覆盖已有文件 |
| | `rename_without_conflict_actually_lands` | 该落地的没落地 |

### 隔离

```rust
let dir = tempfile::tempdir().expect("建临时目录失败");
let paths = AppPaths::from_root(dir.path());
let state = bootstrap::build(&paths).expect("装配失败");
```

**测试绝不碰用户真实的 `%APPDATA%\NoteText`**：开发期反复写库会把用户的
书签 / 片段搞乱。`bootstrap::build` 接受 `&AppPaths` 参数就是为了这个——
测试与生产共用同一份装配链路，不存在「测试通过但生产挂掉」的分支。

Windows 上 `tempdir` 要求句柄已释放：`Db` 的 `Connection` 在 Drop 时
自动 checkpoint WAL。测试里不需要显式关，但要保证 `state` 在 `dir` 之前 drop。

`common/mod.rs` 只放**每个域都要用**的东西（临时目录、装配、标签样本）。
域内的断言不许下沉到这里——共用层一旦长起来，每个测试文件都会拖着用不上的依赖。

## 验证入口

`scripts/` 下只有两个脚本，全部验证动作收在这里。

```
pwsh scripts/check.ps1          快门：rustfmt → cargo check → 全量测试
pwsh scripts/check.ps1 -Full    门禁：+ i18n 审计 + clippy -D warnings + 前端构建
pwsh scripts/check.ps1 -List    列出全部测试名
pwsh scripts/check.ps1 -Detail  打印每个测试的名字与结果
node scripts/i18n-audit.mjs     单独跑国际化审计
```

### 分档的理由

| 档位 | 什么时候用 | 省的是什么 |
|---|---|---|
| 默认快门 | 日常改代码 | 一次跑完全部 rustfmt / check / 测试 |
| `-Full` | 改了前端、提交前 | clippy 的 lint、`vue-tsc` 类型检查、i18n 审计，编译期发现不了 |
| `-List` | 不知道入口叫什么 | 列出 23 个测试名 |

**耗时大头是编译与链接测试二进制，断言本体在亚秒级**——
全量测试 < 1 秒，所以不设「只跑一个测试」的开关：加开关省不了多少时间，
却要多记一个参数。

### 失败摘要

脚本把测试输出落到 `.check/test-output.log`，失败时摘出：

```
^test .* FAILED | ^---- | ^thread .* panicked | \.rs:\d+:
```

抽不到再退回末尾 30 行。CI 里同一份内容追加进 job summary，不必下载 artifact。

## 国际化审计

编译期抓不到、运行时只表现成「界面显示原始 key」或「英文界面里冒出中文」的
问题，由 `scripts/i18n-audit.mjs` 机械核对。它查四件事：

| 检查 | 为什么必须机械核对 |
|---|---|
| 引用的 key 是否都有定义 | 缺 key 时 vue-i18n 显示 key 本身（`menu.language`），不报错 |
| zh-CN 与 en 是否同构 | 漏翻一处就是中英混排，人工对 584 个 key 不现实 |
| 模板里有没有裸中文 | 中文写在模板里永远不会跟着语言切换 |
| 后端错误码是否两边都有文案 | 后端 message 固定中文，英文界面靠前端按码替换 |
| 提示里是否直接传了 key 而非 `t(key)` | `toast.ok('menu.langZh')` 会把 key 印到屏幕上 |

`labelKey` / `hintKey` / `titleKey` 三种引用形态都要扫——命令表用的是
`labelKey`，只扫 `t('...')` 会整类漏掉。

## 约束

- 一个测试只讲一种坏法；
- 同一个行为只断言一次：repo 的语义归 repo 的测试，编排层的语义归 service 的测试；
- 昂贵的 setup（临时目录、装配服务容器）每个测试各自准备一份，
  换来的是「任何一条失败都不影响其他条」；
- 不写 `-short` 隔离：当前没有依赖真实网络 / 真实版本库的测试。

## 取舍

| 选择 | 优势 | 代价 |
|---|---|---|
| 按域拆 4 个文件 | 改某个域只读那个文件；编译单元分离 | 跨域的阅读顺序被打断，靠文件名与域同名（`file_chain.rs` ↔ `service/file.rs`）补偿 |
| 一个测试一种坏法（而非一条长链一个函数） | 失败报文直接指出坏在哪，不必对着行号回找 | 每个测试都要自己建一次临时目录与装配（亚秒级，可接受） |
| 不引 mock 框架 | `bootstrap::build(&paths)` 给临时目录就拿到真实 `AppState`，没有测试替身与生产不一致的空间 | 每个测试都要真的建一次库 |
| 断言写人话失败原因（`assert_eq!(x, y, "应为 CRLF")`） | 失败报文直接指到「哪条规则被破坏」 | 断言本身长了几个字 |
