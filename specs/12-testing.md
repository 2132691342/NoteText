# 12 · 测试与验证

## 定位

`src-tauri/tests/chain_tests.rs`（9 条长链路）+ 各模块内的极少数单测 +
`scripts/` 两个脚本（唯一验证入口）。这一篇解释「测什么、怎么跑、为什么这么切」。

## 只测长链路

一条测试的价值等于「它失败时用户会看到什么坏事」。说不出那句坏事的断言不写。

| 保留 | 不写 |
|---|---|
| 跨模块长链路（装配 → 建表 → 会话恢复 → 关闭） | 单个纯函数的输入输出 |
| 协议硬约束（空数组永不 null、行尾按原样写回、BOM 剥离与补回） | 简单 CRUD、字段映射、枚举转换 |
| 安全护栏（路径穿越、目标覆盖、成环、原子写不留临时文件） | 幂等 setter、构造函数冒烟 |
| 顺序语义（标签顺序、光标恢复、加密与密码三态） | 只验「不 panic」「非 nil」的弱断言 |

一条测试讲一个**完整的用户故事**（打开 → 改 → 存 → 重开、扫描 → 勾选 → 应用、
压缩 → 分卷 → 合并），故事内的每一段有自己的断言与失败说明；同一域的相近故事
合并进同一条，省掉重复 setup。

## 清单：9 条

```
src-tauri/tests/chain_tests.rs
├── 文件(3)
│   ├── file_round_trip_preserves_encoding_bom_and_line_ending   行尾 / BOM / 强制编码 / 原子写
│   ├── gbk_csv_preview_and_text_view_agree                      GBK 表格与文本两条链路
│   └── file_ops_refuse_destructive_targets                      覆盖 / 成环 / 相对路径 / 逐条记账
├── 会话(1)   first_launch_seeds_defaults_and_session_restores_tabs
├── 批量(1)   batch_two_phase_applies_only_confirmed_plans
├── 备份(1)   backup_round_trip_merges_with_local_rows
├── 压缩(2)
│   ├── archive_round_trip_all_formats_then_update_split_join     15 格式 + 增删 + 分卷 + 合并
│   └── archive_rejects_traversal_and_separates_password_errors   穿越 / 密码三态 / 损坏检测
└── 系统关联(1) file_assoc_register_round_trip_lands_open_with_and_default
```

| 组 | 守护的坏事 |
|---|---|
| 文件 | 行尾 / BOM 在打开-保存-重开间漂移；选错编码打不开；GBK 表整列 mojibake；临时文件堆积；重命名覆盖文件、目录成环 |
| 会话 | 首启没写默认配置；空列表序列化成 `null` 白屏；重启丢顺序与窗口尺寸；关光标签丢会话行 |
| 批量 | 扫描就落盘；改掉没勾选的文件；重命名覆盖已有文件 |
| 备份 | 换机丢数据；导入抹掉本机独有数据；宏的 UUID 变了导致引用断链 |
| 压缩 | 某格式内容变了或层级塌了；增删丢条目；首卷打不开；合并字节不一致；穿越写到目标之外；两种密码错误混成一个码；损坏包测出「好」 |
| 关联 | 注册写错键值（引号丢了、ProgId 挂错位置）→ 双击没反应；注销后留悬空默认或误伤另一组 |

单测只有 `service/encoding.rs` 里的 2 条：三种易判错的字节序列（GBK 中文、
截断的 UTF-8、无 BOM 的 UTF-16）与一次编码往返。它们跑在毫秒级，
是「编码判定」这一处高风险的定点哨。

## 验证入口

一个脚本收口全部动作，按改动范围选档：

| 命令 | 跑什么 | 什么时候用 |
|---|---|---|
| `pwsh scripts/check.ps1` | rustfmt → cargo check → 全部测试 | 改后端，日常 |
| `pwsh scripts/check.ps1 -Frontend` | i18n 审计 → 前端构建 | 改前端 |
| `pwsh scripts/check.ps1 -Full` | 两侧全跑 + clippy `-D warnings` | 提交前 |
| `pwsh scripts/check.ps1 -List` | 列出全部测试名 | 忘了入口叫什么 |

只跑一条链路（编译一次、只执行匹配的测试，是最常用的定位手段）：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test chain_tests 压缩
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

`node scripts/i18n-audit.mjs` 可单独跑，查五件事——key 引用是否都有定义、
zh/en 是否对齐、模板里有没有裸中文、后端错误码是否两边都有文案、
编码清单是否两边都有显示名。加 `--verbose` 才列出未被静态引用的 key。

## 规模约束

- **单个集成测试文件**：编译只链接一次 lib，9 条测试的 setup 各自独立但共用 helper
- **昂贵 setup 只在入口准备**：临时目录、服务容器都走 `boot()`
- 测试用 `tempfile::tempdir()` 隔离，**绝不碰用户真实的 `%APPDATA%\NoteText`**
- 全量测试 1.6 秒级，不做「只跑某组」的脚本开关——用 cargo 的过滤参数即可

## 取舍

| 选择 | 优势 | 代价 |
|---|---|---|
| 9 条长链路收进一个文件 | 编译只链接一次；改代码只等一次编译 | 单文件接近千行，靠分节注释导航 |
| 同域故事合并进一条 | 测试数少而每条都完整；setup 只做一次 | 一条失败要读到断言消息才知道是哪段 |
| 不引 mock 框架 | 测试与生产共用装配链，没有替身与生产不一致的空间 | 每条测试真的建一次库（亚秒级） |
| 前端只跑构建 + i18n 审计，不跑组件测试 | 桌面前端的价值在交互，不在渲染断言；靠审计脚本兜住文案类回归 | 组件级回归要靠手测 |
