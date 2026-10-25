# 12 · 测试与验证

## 定位

`src-tauri/tests/chain_tests.rs`（单文件，9 条测试按「文件 / 会话 / 批量 / 备份 /
压缩 / 系统关联」六组排列）+ `scripts/` 下两个脚本（唯一验证入口）。
这一篇解释「为什么这么组织」，条文判据见 [AGENTS.md §3](../AGENTS.md)。

## 组织原则：只测长链路，一条测试一个用户故事

单元级断言不入链：纯函数的输入输出一眼可见，单测它们不守护「用户文件没了 /
界面崩白屏」级别的事故。链路测试必须满足「它失败时，用户界面上会出现一件
具体的坏事」——测试函数名就是那件坏事的描述，失败报文直接指认，不用对行号。

一条测试讲一个**完整的用户故事**（打开→改→存→重开、扫描→勾选→应用、
压缩→分卷→合并→还原），故事内的每一段有自己的断言与失败说明；同一域的
相近故事合并进同一条，省掉重复 setup，失败消息仍能指认到段。

```
src-tauri/tests/
└── chain_tests.rs   文件(2) / 会话(1) / 批量(1) / 备份(1) / 压缩(3) / 系统关联(1) = 9 个测试
```

| 组 | 测试 | 守护的坏事 |
|---|---|---|
| 文件 | `file_read_write_roundtrip_preserves_encoding_and_atomic_write` | 行尾没按原样写回 / BOM 丢或多字节 / 强制编码打开仍是乱码 / 选错编码打不开 / 临时文件堆积 / 打错路径在磁盘留痕 |
| | `file_tree_ops_reject_destructive_requests` | 重命名无声覆盖文件 / 目录树成环 / 路径逃逸 / 一个陈旧路径让整批删除失败 / 新建同名清空已有内容 |
| 会话 | `first_launch_seeds_defaults_and_session_round_trip_restores_tabs` | 首启没写默认配置 / 空列表序列化成 null 白屏 / 重启丢标签顺序与窗口尺寸 / 关光标签丢会话行 |
| 批量 | `batch_two_phase_applies_only_confirmed_plans` | 扫描就落盘 / 改掉没勾选的文件 / 重命名覆盖已有文件 / 该落地的没落地 |
| 备份 | `backup_round_trip_restores_user_tables_and_keeps_local_rows` | 备份只导出不导入（换机丢数据）/ 导入抹掉本机已有数据 |
| 压缩 | `archive_round_trip_all_formats_preserves_content` | 某个格式压缩往返丢内容 / 中文名乱 / 目录层级塌 / 改名后的 .tar.xz .tar.br 没被识别成 tar 家族 |
| | `archive_update_split_join_preserve_bytes` | 追加 / 删除条目后丢内容 / 分卷首卷打不开 / 合并还原的字节不对 |
| | `archive_rejects_traversal_and_distinguishes_password_and_corruption` | 恶意条目名把文件写到目标目录之外 / 要密码与密码错混成一个错（前端弹不出密码框）/ 损坏的包测试照样说「好」 |
| 系统关联 | `file_assoc_register_round_trip_lands_open_with_and_default` | 双击关联文件打不开 NoteText（open 命令丢引号 / ProgId 挂错键）/ 注销后留悬空默认 / 注销一组误伤另一组 |

## 隔离：测试与生产共用同一条装配链

```rust
let dir = tempfile::tempdir().expect("建临时目录失败");
let paths = AppPaths::from_root(dir.path());
let state = bootstrap::build(&paths).expect("装配失败");
```

**测试绝不碰用户真实的 `%APPDATA%\NoteText`**：开发期反复写库会把用户的书签 /
片段搞乱。`bootstrap::build` 接受 `&AppPaths` 参数就是为了这个——测试与生产
共用同一份装配链路，不存在「测试通过但生产挂掉」的分支，也因此不需要 mock 框架。

**注册表链路同样不碰真实关联**：文件关联测试把整套键位写进
`HKCU\Software\NoteTextAssocTest-<uuid>` 沙箱（scope 参数），测试进程的
winreg 直读做独立仲裁，Drop 时整树删除。

Windows 上 `tempdir` 要求句柄已释放：`Db` 的 `Connection` 在 Drop 时自动
checkpoint WAL，测试里不显式关，但保证 `state` 在 `dir` 之前 drop。

## 验证入口

`scripts/` 下只有两个脚本，全部验证动作收在这里。

```
pwsh scripts/check.ps1          快门：rustfmt → cargo check → 全量测试
pwsh scripts/check.ps1 -Full    门禁：+ i18n 审计 + 前端构建 + clippy -D warnings
pwsh scripts/check.ps1 -List    列出全部测试名（按 #[test] 提取，helper 不会混入）
node scripts/i18n-audit.mjs     单独跑国际化审计
```

不设「只跑一个测试」的开关：全量测试亚秒级，加开关省不了多少时间，却要多记一个参数。
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
| 9 条长链路收进一个文件 | 编译只链接一次 lib；改代码只等一次编译 | 跨域阅读顺序靠 section 注释补偿 |
| 同域故事合并进一条测试 | 测试数少而每条都完整；setup 只做一次 | 段内多个断言要靠失败消息区分 |
| 不引 mock 框架 | 测试与生产共用装配链，没有替身与生产不一致的空间 | 每个测试真的建一次库（亚秒级） |
| 断言写人话失败原因 | 失败报文直接指到哪条规则被破坏 | 断言长了几个字 |
