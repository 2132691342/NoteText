# 12 · 测试与验证

## 定位

集成测试只覆盖「失败时用户会看到坏事」的长链路。纯函数、字段映射、幂等 setter
一眼可见，不守护任何事故，不入链；源码里不挂 `#[cfg(test)]` 单元测试，验证入口才不会被切碎。

**保留判据**：写之前先问「这条测试失败时，用户界面上会出现什么坏事」。
说不出那句坏事，它就不该存在。唯一的例外是分块解码穷举——
失败面是不报错、不 panic 的静默损坏，放不进「用户故事」链路，只能按编码 × 块长逐块穷举。

| 保留 | 删除 |
|---|---|
| 跨模块的长链路（打开 → 改 → 存 → 重开） | 单个纯函数的输入输出 |
| 协议硬约束（空数组永不 null、行尾按原样写回、BOM 剥离与补回） | 简单 CRUD、字段映射、枚举转换 |
| 安全护栏（路径穿越、原子写不留临时文件） | 幂等 setter、构造函数冒烟 |
| 顺序语义（标签顺序、光标恢复） | 只验「不 panic」「非 nil」的弱断言 |
| 跨边界的端到端（invoke 出入参序列化） | 同一行为在不同文件里的重复断言 |

## 结构

```
src-tauri/tests/       每域一个文件，一条长链路覆盖一个域的全部能力面
  common/mod.rs         隔离装配与构造器（不单独编译成测试）
  chain_file.rs         文件往返保真、大文件分块编辑、分块解码穷举、文件树护栏
  chain_table.rs        大表格：CSV 记录边界与规模、编码与边界、JSON 数组 / JSONL
  chain_sheet.rs        XLSX 分页、样式保留、跨窗口取页与切表
  chain_session.rs      会话恢复与空列表序列化、备份导出导入
  chain_batch.rs        两阶段查找替换与重命名
  chain_archive.rs      可写格式全生命周期、只读格式解析、异常包拒绝
  chain_file_assoc.rs   注册与注销（仅 Windows）
```

每个域一个测试二进制，编译相互独立、可单独跑。非 Windows 下
`chain_file_assoc` 不编译；本项目只发 Windows、CI 跑 `windows-latest`，
门禁口径按 7 个集成测试二进制、14 条链路。

## 链路清单

每条链路对应一个用户故事，故事内每一段有自己的断言与失败说明，失败报文直接指认坏在哪一步。
相近动作合并进同一条，setup 只做一次。

| 测试 | 文件 | 用户故事 | 守住的坏事 |
|---|---|---|---|
| `file_round_trip_keeps_line_ending_bom_and_forced_encoding` | chain_file | 打开 → 保存 → 重开 → 二进制通道写回 → 文件树操作 | 行尾每次保存都漂移 / BOM 丢或多出来 / 强制编码打不开 / 原子写留临时文件 / base64 写出垃圾字节 / 改名覆盖已有文件 / 成环 / 路径逃逸 / 批量删除整批失败 |
| `big_file_block_index_reads_blocks_and_reports_lines` | chain_file | 打开 15MB 日志 → 分块浏览 → 跳末行 → 外部改动 → 改中间一块 → 整文流式查找 → GB18030 日志查找 | 逐块拼不回原文 / 行号定位落错块 / 外部修改不报 stale / 单块写动到前后字节 / 过期基线覆盖新内容 / 查找命中行号与块读出文本对不上 / 空命中序列化成 null / 非 UTF-8 文件命中行号错位 |
| `stream_decode_keeps_multibyte_across_any_chunk_boundary` | chain_file | 任意块长分块解码（六编码 × 块长 1–17 穷举） | 块边界切在多字节字符中间解出替换符 / UTF-16 半码元把字拆坏 |
| `session_and_backup_survive_restart_and_reimport` | chain_session | 首次启动 → 开三个标签 → 重启 → 关光标签 → 备份导出导入 | 空列表序列化成 null（前端白屏）/ 标签顺序与活跃项丢失 / 关光标签丢会话行 / 备份导不回来（换机丢数据）/ 导入抹掉本机数据 |
| `batch_preview_then_apply_touches_only_confirmed_files` | chain_batch | 扫描 → 勾选 → 替换 → 改名 | 扫描阶段就落盘 / 改掉没勾选的文件 / 改名覆盖已存在文件 |
| `csv_table_indexes_pages_finds_and_writes_back` | chain_table | 打开 5 万行 CSV → 翻页 → 整表查找 → 改一格删一行 → 另存为 → 引号换行 / 百万行满页 | 记录数算错 / 行下标与内容错位 / 命中行号对不上 / 改格动到别的行 / 另存为改到源文件 / 引号内换行切断记录 / 百万行深处给不满页 |
| `csv_table_decodes_deep_pages_and_counts_trailing_row` | chain_table | GB18030 十万行深处分页 → UTF-16LE BOM 跨块 → 无尾换行末行 → 表头纠正 → 强制编码遇 BOM | 深处分页解成替换符 / UTF-16 行号错位 / 末行取不到 / 表头误判无法纠正 / 首格多出 U+FEFF |
| `json_table_indexes_pages_and_writes_back` | chain_table | 打开跨行的顶层 JSON 数组 → 翻页 → 改一个元素；打开 JSONL → 翻页 → 改一格 | 按行切把元素切碎 / 含逗号的字段被切开 / 改一个元素动到相邻元素 / 嵌套数组被切碎 / 改一格动到相邻行 |
| `sheet_opens_pages_edits_and_preserves_styles` | chain_sheet | 打开 xlsx → 分页浏览 → 跨表查找 → 改格加行 → 重开 | 整块解析把百万行工作表读进内存 / 改单元格把样式表与未编辑工作表全冲掉 / 共享串解成乱码 / 新增行丢失 |
| `sheet_pages_across_windows_and_survives_sheet_switch` | chain_sheet | 两万行工作表跨窗口取页 → 切表 → 切回 | 行偏移采样切错字节使行号与内容错位 / 切表后 XML 换出再回来取错内容 / 总行数算错 |
| `archive_lifecycle_covers_all_formats_update_and_split` | chain_archive | 16 种可写格式打包解包 → 追加删除 → 分卷合并 | 某格式往返丢内容 / 中文名与嵌套层级塌 / 改名后认不出 tar 家族 / 合并后字节变了 |
| `archive_readonly_formats_list_extract_and_refuse_create` | chain_archive | CAB 写→列→解；对 rar/lzh/cpio/ar 点创建；真实 RAR / LHA 样本列→解→预览 | 能力表说能写但引擎拒绝 / CAB 往返丢内容 / 创建只读格式留下半个文件 / 只读容器头偏移算错解出垃圾 / 真实包头解析错导致空列表 |
| `archive_rejects_escape_and_distinguishes_password_from_corruption` | chain_archive | 打开坏包 / 加密包 | 恶意条目写到目标目录外 / 缺密码与密码错混成一个错 / 损坏包测试照样说「好」 |
| `file_assoc_toggle_registers_and_restores_shell_entries` | chain_file_assoc | 开关联 → 设默认 → 关关联 | 双击没反应（open 命令丢引号、ProgId 挂错）/ 注销后留悬空默认 / 注销一组误伤另一组 |

前端唯一的脚本门禁是 `npm run check:grid`：虚拟滚动窗口的计算在 `gridScroll.ts`，
脚本直接 import 生产模块跑一组坐标断言（见 [19](19-large-files-and-sheet-edit.md)）。

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
cargo test --manifest-path src-tauri/Cargo.toml --test chain_file --test chain_session --test chain_batch --test chain_table --test chain_sheet --test chain_archive --test chain_file_assoc
npm --prefix frontend run check:grid
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
| 一条链路覆盖一个域的全部能力面 | 改一个小功能只跑一个域；失败报文自带上下文 | 单条链路内断言较多，要靠失败消息区分段 |
| 分块解码的穷举并入 chain_file | 静默损坏有穷举守护，文件数不膨胀 | 其余纯函数的边界错误只能靠代码审查 |
| 不引 mock 框架 | 测试与生产共用装配链 | 每条测试真的建一次库（毫秒级） |
| release 不重跑门禁 | 出包少一轮编译 | 依赖 main 分支的 CI 结果 |
