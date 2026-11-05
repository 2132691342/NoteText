# 12 · 测试与验证

## 定位

集成测试只覆盖「失败时用户会看到坏事」的长链路。纯函数、字段映射、幂等 setter
一眼可见，不守护任何事故，不入链；源码里不挂 `#[cfg(test)]` 单元测试，验证入口才不会被切碎。

## 结构

```
src-tauri/tests/       八个文件，每域一条长链路编排
  common/mod.rs         隔离装配与构造器（不单独编译成测试）
  chain_file.rs         文件读写往返、大文件分块、文件树护栏
  chain_session.rs      默认配置、会话恢复、空列表序列化
  chain_batch.rs        两阶段查找替换与重命名
  chain_backup.rs       导出导入
  chain_archive.rs      可写格式全生命周期、异常拒绝
  chain_archive_readonly.rs  CAB 写 / RAR-LHA-CPio-AR 只读解析
  chain_sheet.rs        样式保留与跨表查找
  chain_file_assoc.rs   注册与注销（仅 Windows）
src-tauri/src/pkg/stream.rs  2 条分块解码守护（模块内）
```

每条长链路对应一个二进制，编译相互独立、可单独跑。非 Windows 下 `chain_file_assoc`
不编译，共 11 条；本项目只发 Windows、CI 跑 `windows-latest`，门禁口径按 12 条。

## 12 条链路

| 测试 | 文件 | 用户故事 | 守住的坏事 |
|---|---|---|---|
| `file_round_trip_keeps_line_ending_bom_and_forced_encoding` | chain_file | 打开 → 保存 → 重开 | 行尾每次保存都漂移 / BOM 丢或多出来 / 强制编码打不开 / GBK 表格与文本解出两份内容 / 临时文件堆积 |
| `big_file_block_index_reads_blocks_and_reports_lines` | chain_file | 打开 15MB 日志 → 分块浏览 → 跳末行 → 外部改动 → 改中间一块 → 整文流式查找 | 逐块拼不回原文 / 行号定位落错块 / 外部修改不报 stale / 单块写动到前后字节 / 过期基线能覆盖新内容 / 查找命中行号与块读出文本对不上 / 空命中序列化成 null |
| `file_ops_refuse_overwrite_cycle_escape_and_partial_delete` | chain_file | 改名、删一批、新建同名 | 无声覆盖已有文件 / 目录移进自己子目录 / 路径逃逸 / 一个坏路径让整批删除失败 |
| `session_round_trip_restores_tabs_and_empty_lists_serialize_as_arrays` | chain_session | 首次启动 → 开三个标签 → 重启 | 空列表序列化成 null（前端白屏）/ 标签顺序与活跃项丢失 / 关光标签丢会话行 |
| `batch_preview_then_apply_touches_only_confirmed_files` | chain_batch | 扫描 → 勾选 → 替换 → 改名 | 扫描阶段就落盘 / 改掉没勾选的文件 / 改名覆盖已存在文件 |
| `backup_export_import_restores_tables_and_keeps_local_rows` | chain_backup | 导出 → 清空 → 导入 | 备份导不回来（换机丢数据）/ 导入抹掉本机数据 |
| `archive_lifecycle_covers_all_formats_update_and_split` | chain_archive | 16 种可写格式打包解包 → 追加删除 → 分卷合并 | 某格式往返丢内容 / 中文名与嵌套层级塌 / 改名后认不出 tar 家族 / 合并后字节变了 |
| `archive_rejects_escape_and_distinguishes_password_from_corruption` | chain_archive | 打开坏包 / 加密包 | 恶意条目写到目标目录外 / 缺密码与密码错混成一个错 / 损坏包测试照样说「好」 |
| `archive_readonly_formats_list_extract_and_refuse_create` | chain_archive_readonly | CAB 写→列→解；对 rar/lzh/cpio/ar 点创建；手工构造 cpio/ar 样本列条目并解出 | 能力表说能写但引擎拒绝 / CAB 往返丢内容 / 创建只读格式留下半个文件 / 只读容器头偏移算错解出垃圾 |
| `archive_reads_real_rar_and_lha_packages` | chain_archive_readonly | 仓库内的极小 RAR / LHA 样本：列 → 解 → 预览 | RAR / LHA 头解析错 → 空列表或一屏乱码；只读格式「能列不能解」 |
| `sheet_edit_preserves_styles_and_finds_across_sheets` | chain_sheet | 打开带样式的 xlsx → 跨表查找 → 改格加行 → 重开 | 改单元格把样式表与未编辑工作表全冲掉 / 共享串解成乱码 / 新增行丢失 |
| `file_assoc_toggle_registers_and_restores_shell_entries` | chain_file_assoc | 开关联 → 设默认 → 关关联 | 双击没反应（open 命令丢引号、ProgId 挂错）/ 注销后留悬空默认 / 注销一组误伤另一组 |

一条测试讲一个完整故事：故事内每一段有自己的断言与失败说明，失败报文直接指认
坏在哪一步。同域的相近动作合并进同一条，setup 只做一次。

## 模块内单元测试的保留判据

只有一类例外进源码：**流式分块解码的静默数据损坏**——不报错、不 panic，
只是中文在块边界上悄悄变成替换符。它放不进「用户故事」链路，又必须逐块长穷举，
所以在 `pkg/stream.rs` 里留两条：

| 测试 | 守住的坏事 |
|---|---|
| `decodes_chunks_without_breaking_multibyte` | 分块解码在多字节边界出错 → 中文变替换符（四种编码 × 17 种块长） |
| `decodes_utf16_in_odd_chunks` | UTF-16 半码元边界把字拆坏 |

其余能力（只读格式、行索引、编码探测、跨块查找）的失败面由 12 条长链路与手工验收覆盖。

## 隔离

测试与生产共用同一条装配链 `bootstrap::build` + `AppPaths::from_root(tempdir)`，
因此不需要 mock 框架，也不存在「测试通过、生产挂掉」的分支。测试永不碰 `%APPDATA%\NoteText`。

文件关联测试把整棵注册表键写进 `HKCU\Software\NoteTextAssocTest-<uuid>`，
用 winreg 直读做独立仲裁，`Drop` 时整树删除。

## 验证入口

```bash
npm --prefix frontend run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo check --manifest-path src-tauri/Cargo.toml --all-targets
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --test chain_file --test chain_session --test chain_batch --test chain_backup --test chain_archive --test chain_archive_readonly --test chain_sheet --test chain_file_assoc
```

前端构建必须早于 `cargo check`、Clippy 与测试：`tauri::generate_context!()` 会在
Rust 编译期读取 `frontend/dist`。局部验证可在已有 `dist` 后只跑一个域：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test chain_archive -- archive
```

`ci.yml` 按上述顺序执行门禁；`release.yml` 只负责出包，不重复这一轮。

## 取舍

| 选择 | 优势 | 代价 |
|---|---|---|
| 每域一个测试二进制 | 报错时文件小、定位快；单域改动只重编一个测试 | 共享设施抽到 `common/mod.rs` |
| 模块内只留 2 条分块解码单测 | 静默损坏有穷举守护，验证入口仍只有一处 | 其余纯函数的边界错误只能靠代码审查 |
| 一条测试一个完整故事 | 测试数少而每条都读完即懂 | 段内多断言要靠失败消息区分 |
| 不引 mock 框架 | 测试与生产共用装配链 | 每条测试真的建一次库（毫秒级） |
| release 不重跑门禁 | 出包少一轮编译 | 依赖 main 分支的 CI 结果 |
