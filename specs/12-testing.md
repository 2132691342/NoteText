# 12 · 测试与验证

## 定位

一个集成测试文件覆盖「失败时用户会看到坏事」的长链路。
纯函数、字段映射、幂等 setter 这类一眼可见的东西不写测试——它们不守护任何事故，
却会把验证入口的注意力切碎。

```
src-tauri/tests/chain_tests.rs   10 条链路：文件(2) 会话(1) 批量(1) 备份(1) 压缩(4) 系统关联(1) + 文件关联
```

## 10 条链路

| 测试 | 用户故事 | 守住的坏事 |
|---|---|---|
| `file_round_trip_keeps_line_ending_bom_and_forced_encoding` | 打开 → 保存 → 重开 | 行尾每次保存都漂移 / BOM 丢或多出来 / 强制编码打不开 / GBK 表格与文本解出两份内容 / 临时文件堆积 |
| `file_ops_refuse_overwrite_cycle_escape_and_partial_delete` | 改名、删一批、新建同名 | 无声覆盖已有文件 / 目录移进自己子目录 / 路径逃逸 / 一个坏路径让整批删除失败 |
| `session_round_trip_restores_tabs_and_empty_lists_serialize_as_arrays` | 首次启动 → 开三个标签 → 重启 | 空列表序列化成 null（前端白屏）/ 标签顺序与活跃项丢失 |
| `batch_preview_then_apply_touches_only_confirmed_files` | 扫描 → 勾选 → 替换 → 改名 | 扫描阶段就落盘 / 改掉没勾选的文件 / 改名覆盖已存在文件 |
| `backup_export_import_restores_tables_and_keeps_local_rows` | 导出 → 清空 → 导入 | 备份导不回来（换机丢数据）/ 导入抹掉本机数据 |
| `archive_lifecycle_covers_all_formats_update_and_split` | 可写的 16 种格式打包解包 → 追加删除 → 分卷合并 | 某格式往返丢内容 / 中文名与嵌套层级塌 / 改名后认不出 tar 家族 / 合并后字节变了 |
| `archive_rejects_escape_and_distinguishes_password_from_corruption` | 打开坏包 / 加密包 | 恶意条目写到目标目录外 / 缺密码与密码错混成一个错 / 损坏包测试照样说「好」 |
| `archive_readonly_formats_list_extract_and_refuse_create` | CAB 写→列→解；对 rar/lzh/cpio/ar 点创建；cpio/ar 手工构造样本列条目并解出 | 能力表说能写但引擎拒绝（创建对话框给出必然失败的选项）/ CAB 往返丢内容 / 创建只读格式留下半个文件 / 只读容器头偏移算错解出垃圾 |
| `archive_reads_real_rar_and_lha_packages` | 仓库内的极小 RAR / LHA fixture：列 → 解 → 预览 | RAR / LHA 头解析错 → 空列表或一屏乱码；只读格式「能列不能解」 |
| `file_assoc_toggle_registers_and_restores_shell_entries` | 开关联 → 设默认 → 关关联 | 双击没反应（open 命令丢引号、ProgId 挂错）/ 注销后留悬空默认 / 注销一组误伤另一组 |

一条测试讲一个完整故事：故事内每一段有自己的断言与失败说明，失败报文直接指认
坏在哪一步，不用回头对行号。同域的相近动作合并进同一条，setup 只做一次。

## 单元测试的保留判据

只读格式（RAR / LHA / cpio / ar）与流式编解码的失败模式是**静默数据损坏**：
不报错、不 panic，只是内容悄悄变了。这类逻辑放不进「用户故事」式的链路测试里
（构造真实 RAR 需要外部工具），所以按纯函数守护在模块内：

| 测试 | 守住的坏事 |
|---|---|
| `pkg::stream::decodes_chunks_without_breaking_multibyte` | 分块解码在多字节边界出错 → 中文变替换符（四种编码 × 17 种块长） |
| `pkg::stream::decodes_utf16_in_odd_chunks` | UTF-16 半码元边界把字拆坏 |
| `service::bigfile::indexes_and_pages_match_actual_lines` | 行索引把 CRLF 算成两行 → 行号读数错一倍、翻页错位 |
| `service::bigfile::finds_correct_line_numbers_across_blocks` | 整块查找的行号换算错一位 → 点「下一个」跳错地方 |
| `service::bigfile::finds_matches_across_chunk_boundaries` | 跨块的行漏命中 → 用户以为文件里没有这个词 |
| `service::encoding::probes_cjk_and_bomless_utf16_correctly` | GBK 中文 / 截断 UTF-8 / 无 BOM UTF-16 判错 → 打开就乱码 |
| `service::encoding::round_trips_gbk_text` | 编码往返丢字 |
| `domain::archive_format::readonly_formats_are_not_writable` | 能力表与引擎能力漂移 → 创建对话框给出必然失败的选项 |

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
npm --prefix frontend run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo check --manifest-path src-tauri/Cargo.toml --all-targets
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --test chain_tests
```

前端构建必须早于 `cargo check`、Clippy 与测试：`tauri::generate_context!()` 会在
Rust 编译期读取 `frontend/dist`。局部验证可在已有 `dist` 后直接运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test chain_tests -- archive
```

## 取舍

| 选择 | 优势 | 代价 |
|---|---|---|
| 10 条链路收进一个文件 | 只链接一次 lib，改代码只等一次编译 | 跨域阅读靠 `// ---------- 域 ----------` 分隔 |
| 一条测试一个完整故事 | 测试数少而每条都读完即懂 | 段内多断言要靠失败消息区分 |
| 不引 mock 框架 | 测试与生产共用装配链 | 每条测试真的建一次库（毫秒级） |
| 只测长链路 | 验证入口小、失败必有事 | 纯函数的边界错误由模块内单元守护；计数字节这类一眼可见的不测 |
