# 12 · 测试与验证

## 定位

一个集成测试文件 + 一个验证脚本，覆盖「失败时用户会看到坏事」的长链路。
纯函数、字段映射、幂等 setter 这类一眼可见的东西不写测试——它们不守护任何事故，
却会把验证入口的注意力切碎。

```
src-tauri/tests/chain_tests.rs   8 条链路：文件(2) 会话(1) 批量(1) 备份(1) 压缩(2) 系统关联(1)
scripts/check.ps1                唯一的验证入口（三档）
scripts/i18n-audit.mjs           编译期抓不到的国际化问题
```

## 8 条链路

| 测试 | 用户故事 | 守住的坏事 |
|---|---|---|
| `file_round_trip_keeps_line_ending_bom_and_forced_encoding` | 打开 → 保存 → 重开 | 行尾每次保存都漂移 / BOM 丢或多出来 / 强制编码打不开 / GBK 表格与文本解出两份内容 / 临时文件堆积 |
| `file_ops_refuse_overwrite_cycle_escape_and_partial_delete` | 改名、删一批、新建同名 | 无声覆盖已有文件 / 目录移进自己子目录 / 路径逃逸 / 一个坏路径让整批删除失败 |
| `session_round_trip_restores_tabs_and_empty_lists_serialize_as_arrays` | 首次启动 → 开三个标签 → 重启 | 空列表序列化成 null（前端白屏）/ 标签顺序与活跃项丢失 |
| `batch_preview_then_apply_touches_only_confirmed_files` | 扫描 → 勾选 → 替换 → 改名 | 扫描阶段就落盘 / 改掉没勾选的文件 / 改名覆盖已存在文件 |
| `backup_export_import_restores_tables_and_keeps_local_rows` | 导出 → 清空 → 导入 | 备份导不回来（换机丢数据）/ 导入抹掉本机数据 |
| `archive_lifecycle_covers_all_formats_update_and_split` | 15 种格式打包解包 → 追加删除 → 分卷合并 | 某格式往返丢内容 / 中文名与嵌套层级塌 / 改名后认不出 tar 家族 / 合并后字节变了 |
| `archive_rejects_escape_and_distinguishes_password_from_corruption` | 打开坏包 / 加密包 | 恶意条目写到目标目录外 / 缺密码与密码错混成一个错 / 损坏包测试照样说「好」 |
| `file_assoc_toggle_registers_and_restores_shell_entries` | 开关联 → 设默认 → 关关联 | 双击没反应（open 命令丢引号、ProgId 挂错）/ 注销后留悬空默认 / 注销一组误伤另一组 |

一条测试讲一个完整故事：故事内每一段有自己的断言与失败说明，失败报文直接指认
坏在哪一步，不用回头对行号。同域的相近动作合并进同一条，setup 只做一次。

## 隔离

```rust
let dir = tempfile::tempdir().expect("建临时目录失败");
let state = bootstrap::build(&AppPaths::from_root(dir.path())).expect("装配失败");
```

测试与生产共用同一条装配链（`bootstrap::build`），因此不需要 mock 框架，
也不存在「测试通过、生产挂掉」的分支。测试永不碰 `%APPDATA%\NoteText`。

文件关联测试把整棵注册表键写进 `HKCU\Software\NoteTextAssocTest-<uuid>`，
用 winreg 直读做独立仲裁，`Drop` 时整树删除。

## 验证入口

```bash
pwsh scripts/check.ps1                 # 日常：rustfmt → cargo check → 8 条链路
pwsh scripts/check.ps1 -Test archive   # 改压缩时只跑名字含 archive 的链路
pwsh scripts/check.ps1 -Full           # 提交前：+ i18n 审计 + clippy -D warnings + 前端构建
```

只跑一条链路时不做 fmt / clippy / 前端构建：改一个函数要等整套门禁，
就没人愿意频繁验证——这是 `-Test` 存在的唯一理由。
失败时脚本从日志里摘出测试名与 `.rs:行号`，成功时不输出任何测试清单。

## 国际化审计

`scripts/i18n-audit.mjs` 只查编译期抓不到的五类问题，通过时打印一行统计：

| 检查 | 不查会怎样 |
|---|---|
| 引用的 key 有没有定义 | vue-i18n 直接把 key 显示到界面上 |
| zh-CN 与 en 是否同构 | 中英混排，人工对几百个 key 不现实 |
| 模板里有没有裸中文 | 那段文字永远不跟着语言切换 |
| 后端错误码两边是否有文案 | 英文界面里冒出中文 message |
| toast / 弹窗是否直接传了 key | 屏幕上印出 `menu.langZh` |

## 取舍

| 选择 | 优势 | 代价 |
|---|---|---|
| 8 条链路收进一个文件 | 只链接一次 lib，改代码只等一次编译 | 跨域阅读靠 `// ---------- 域 ----------` 分隔 |
| 一条测试一个完整故事 | 测试数少而每条都读完即懂 | 段内多断言要靠失败消息区分 |
| 不引 mock 框架 | 测试与生产共用装配链 | 每条测试真的建一次库（毫秒级） |
| 只测长链路 | 验证入口小、失败必有事 | 纯函数的边界错误只能靠代码审查 |
| `-Test` 只跑一条链路 | 局部改动秒级反馈 | 参数多记一个（换来的是愿意跑） |
