# 12 · 测试与验证

## 定位

`src-tauri/tests/chain_tests.rs`（单文件、按 section 分三组：文件与编码 / 会话 / 批量）
+ `scripts/` 下两个脚本（唯一验证入口）。判据与条文见 [AGENTS.md §3](../AGENTS.md)，
这一篇只解释「为什么这么组织」。

## 为什么单文件、一个测试一种坏法

Rust 集成测试按「测试二进制」分文件，但 10 个测试收敛进一个文件，编译只链接一次 lib，
改代码不必等 4 个二进制各自编译。**测试函数名就是它守护的那件坏事**，失败时不需要
拿行号去文件里找第几条断言。一个函数只讲一种坏法：能写成「A、B、C 都对」的函数，
失败时报文说不清坏的是哪个。

```
src-tauri/tests/
└── chain_tests.rs   文件与编码(5) / 会话(3) / 批量(2) = 10 个测试
```

| 组 | 测试 | 坏法 |
|---|---|---|
| 文件与编码 | `read_write_roundtrip_preserves_line_ending_and_bom` | 行尾没记住 / BOM 丢或多字节 |
| | `forced_encoding_read_decodes_and_falls_back_when_unsupported` | 选了编码打开的仍是乱码 |
| | `atomic_write_leaves_no_temp_file_and_never_creates_missing_dirs` | 临时文件堆积 / 打错路径在磁盘留痕 |
| | `rename_rejects_overwrite_cycles_and_escapes` | 重命名无声销毁文件 / 树成环 / 路径逃逸 |
| | `delete_accounts_for_stale_paths_and_create_refuses_existing` | 一个陈旧路径让整批删除失败 |
| 会话 | `first_launch_writes_defaults_and_gives_empty_session` | 首次启动没写默认配置 / 空库崩 |
| | `session_round_trip_restores_tabs_and_survives_close_all` | 重启后标签顺序与窗口尺寸丢失 |
| | `empty_lists_serialize_to_array_not_null` | 前端按数组读拿到 null 白屏 |
| 批量 | `batch_replace_scans_readonly_and_applies_only_checked` | 扫描就落盘 / 改掉没勾选的文件 |
| | `batch_rename_blocks_conflicts_and_lands_clean_plans` | 重命名覆盖已有文件 / 该落地的没落地 |

**工具箱没有链路测试**：JSON 格式化 / 哈希 / 注释开关都是单函数、无编排、无跨层
状态，断言的输入输出一眼可见；它们不守护「用户文件没了」级别的事故，不入链。

**同一条链路的两面合并成一条**：批量替换的「扫描不落盘」与「只改勾选」是
同一个两阶段协议的两侧；批量重命名的「冲突拒绝」与「无冲突落地」是同一个
预览-应用故事的两种结局。分开写会有两份重复的 setup，合并后一条测试
就是一条完整的用户故事，失败消息仍能指认到段。

## 隔离：测试与生产共用同一条装配链

```rust
let dir = tempfile::tempdir().expect("建临时目录失败");
let paths = AppPaths::from_root(dir.path());
let state = bootstrap::build(&paths).expect("装配失败");
```

**测试绝不碰用户真实的 `%APPDATA%\NoteText`**：开发期反复写库会把用户的书签 /
片段搞乱。`bootstrap::build` 接受 `&AppPaths` 参数就是为了这个——测试与生产
共用同一份装配链路，不存在「测试通过但生产挂掉」的分支，也因此不需要 mock 框架。

Windows 上 `tempdir` 要求句柄已释放：`Db` 的 `Connection` 在 Drop 时自动
checkpoint WAL，测试里不显式关，但保证 `state` 在 `dir` 之前 drop。

## 验证入口

`scripts/` 下只有两个脚本，全部验证动作收在这里。

```
pwsh scripts/check.ps1          快门：rustfmt → cargo check → 全量测试
pwsh scripts/check.ps1 -Full    门禁：+ i18n 审计 + clippy -D warnings + 前端构建
pwsh scripts/check.ps1 -List    列出全部测试名
node scripts/i18n-audit.mjs     单独跑国际化审计
```

不设「只跑一个测试」的开关：全量测试 < 1 秒，加开关省不了多少时间，却要多记一个参数。
失败时脚本从 cargo 输出里摘出测试名与出错位置。

## 国际化审计

编译期抓不到、运行时只表现成「界面显示原始 key」或「英文界面里冒出中文」的问题，
由 `scripts/i18n-audit.mjs` 机械核对。它查五件事：

| 检查 | 为什么必须机械核对 |
|---|---|
| 引用的 key 是否都有定义 | 缺 key 时 vue-i18n 显示 key 本身，不报错 |
| zh-CN 与 en 是否同构 | 漏翻一处就是中英混排，人工对几百个 key 不现实 |
| 模板里有没有裸中文 | 中文写在模板里永远不会跟着语言切换 |
| 后端错误码是否两边都有文案 | 后端 message 固定中文，英文界面靠前端按码替换 |
| 提示里是否直接传了 key 而非 `t(key)` | `toast.ok('menu.langZh')` 会把 key 印到屏幕上 |

`t('...')` 与 `labelKey` / `hintKey` / `titleKey` 三种引用形态都要扫——命令表用的是
`labelKey`，只扫 `t()` 会整类漏掉。

## 取舍

| 选择 | 优势 | 代价 |
|---|---|---|
| 单文件收口 10 个测试 | 编译只链接一次 lib，改代码不必等 4 个二进制 | 跨域阅读顺序靠 section 注释补偿 |
| 一个测试一种坏法（同链路两面可合并） | 失败报文直接指出坏在哪，又不留重复 setup | 合并的测试要靠断言消息区分段 |
| 不引 mock 框架 | 测试与生产共用装配链，没有替身与生产不一致的空间 | 每个测试真的建一次库（亚秒级） |
| 断言写人话失败原因 | 失败报文直接指到哪条规则被破坏 | 断言长了几个字 |
