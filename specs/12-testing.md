# 12 · 测试与验证

## 定位

`src-tauri/tests/`（按域分文件，13 条链）+ `scripts/check.ps1`（验证入口）。
完整的测试规范在 [AGENTS.md §3](../AGENTS.md)，这里只讲「为什么这么组织」。

## 设计

### 判据

写之前先问：**这条测试失败时，用户界面上会出现什么坏事？**
说不出那句坏事，它就不该存在。

| 保留 | 删除 |
|---|---|
| 跨模块的长链路（装配 → 建表 → 会话往返 → 序列化契约） | 单个纯函数的输入输出 |
| 协议硬约束（空数组永不 null、行尾按原样写回、BOM 剥离与补回） | 简单 CRUD、字段映射 |
| 安全护栏（批量操作不覆盖别人的文件、原子写不留临时文件） | 构造函数冒烟 |
| 顺序语义（标签顺序、光标恢复） | 只验「不 panic」的弱断言 |

### 文件划分：一个域一个文件

```
src-tauri/tests/
├── common/mod.rs      临时目录、装配、样本数据
├── session_chain.rs   装配与会话往返
├── file_chain.rs      文件读写与编码探测
├── text_chain.rs      文本变换
├── tools_chain.rs     工具箱
└── batch_chain.rs     批量操作
```

改某个域只读那个文件，不必通读全部测试。Rust 的集成测试按「测试二进制」
分文件，一个文件一个域，编译单元也跟着分开。

`common/mod.rs` 只放**每个域都要用**的东西。域内的断言不许下沉到这里——
共用层一旦长起来，每个测试文件都会拖着一堆用不上的依赖。

### 链索引

| 文件 | 链 | 守护的坏事 |
|---|---|---|
| `session_chain` | `test_bootstrap_chain` | **启动即失败**（DB 建不起来，用户看到原生错误框）；**重启后窗口尺寸与标签顺序全丢**；书签连按两次 F2 多出一行 |
| | `test_serialization_contract_chain` | **空面板崩白屏**（前端漏写 `?? []` 就 `Cannot read properties of null`） |
| `file_chain` | `test_file_roundtrip_chain` | **打开 CRLF 文件按一下保存，整个文件行尾变 LF**，同事那边 diff 出几千行；保存后文件头多三个字节；**保存中途断电留下半个文件** |
| | `test_encoding_detection_chain` | GBK 打开一片乱码；BOM 存回去时丢了 |
| `text_chain` | `test_text_pipeline_chain` | **Ctrl+/ 把代码注释坏**（半注释块出 `// //`） |
| | `test_line_op_chain` | 行编辑去重时打乱顺序，用户以为文件坏了 |
| | `test_position_and_stats_chain` | 「转到行」在中文上跳错位置；统计把字节当字符，用户以为数字不对 |
| `tools_chain` | `test_json_and_diff_chain` | 坏 JSON 不说在哪一行，用户找不到错处；diff 增删算错 |
| | `test_regex_and_hash_chain` | 正则死循环挂住 UI；**算出来的摘要与 certutil 对不上**，用户校验文件时误判文件被改 |
| `batch_chain` | `test_batch_find_replace_chain` | **批量替换静默改掉用户没勾选的文件** |
| | `test_batch_rename_chain` | **重命名把已有文件覆盖**——用户的文件没了，且没有撤销键 |

### 隔离

```rust
let dir = tempfile::tempdir().expect("建临时目录失败");
let paths = AppPaths::from_root(dir.path());
let state = bootstrap::build(&paths).expect("装配失败");
```

**测试绝不碰用户真实的 `%APPDATA%\NoteText`**：开发期反复写库
很可能把用户的书签 / 片段搞乱。`bootstrap::build` 接受 `&AppPaths` 参数
就是为了这个——测试与生产共用同一份装配链路。

Windows 上 `tempdir` 要求句柄已释放：`Db` 的 `Connection` 在 Drop 时
自动 checkpoint WAL，而进程退出本身会释放文件句柄，
所以测试里不需要显式关（但要保证 `state` 在 `dir` 之前 drop）。

## 验证入口

```
pwsh scripts/check.ps1              快门：rustfmt → check → 全量测试
pwsh scripts/check.ps1 -Chain batch  只跑名字命中的那条链
pwsh scripts/check.ps1 -List         列出全部测试链
pwsh scripts/check.ps1 -Full         + clippy -D warnings + 前端构建
```

### 分档的理由

| 档位 | 省的是什么 |
|---|---|
| 默认快门 | 编译全部包 + 跑全部测试 |
| `-Chain` | 只跑命中的那条链，跳过其余的执行（编译省不掉） |
| `-Full` | 提交前：clippy 的 lint 与前端类型检查编译期发现不了 |

**耗时大头是编译与链接测试二进制，断言本体多在亚秒级**——
所以「只跑一条链」省的是执行，这就是 `-Chain` 存在的理由。

### 失败摘要

脚本把测试输出落到 `.check/test-output.log`，失败时摘出：

```
^test .* FAILED | ^---- | ^thread .* panicked | \.rs:\d+:
```

抽不到再退回末尾 30 行。CI 里同一份内容追加进 job summary，
不必下载 artifact。

**摘要收在脚本内，不另起一个脚本**：两个文件就要维护两处。

## 配套脚本

除 `check.ps1` 外，`scripts/` 下有三个实测工具。它们不进 CI——
CI 验的是「能不能过」，这些验的是「过得好不好看」。

| 脚本 | 作用 |
|---|---|
| `inspect-visual.mjs` | 量出图标真实尺寸、字号分布、各层高度、浮窗宽度 |
| `inspect-chunks.mjs` | 看每个 chunk 的体积与它拉进了哪些库 |
| `shot.mjs` | 抓当前视口截图 |

`inspect-visual` 存在的理由：CSS 规则与实际渲染结果可以差很远——
写错一条选择器时，样式表看起来完全正常，渲染出来的图标却是撑满的。
肉眼判断不可靠，量出来才可信。

## 约束

- 一个断言只讲一种坏法：能写成「A、B、C 都对」的断言，
  失败时报文说不清坏的是哪个；
- 同一个行为只断言一次：repo 的语义归 repo 的链，service 的归 service 的链；
- 入口一律叫 `test_<domain>_chain`——它就是 grep 的把手，也是 `-Chain` 的入参；
- **不写 `-short` 隔离**：当前没有依赖真实网络 / 真实 git 的链路。

## 取舍

**按域拆文件，代价是跨域的阅读顺序被打断**。补偿办法是文件名与域同名
（`file_chain.rs` ↔ `service/file.rs`），`check.ps1 -List` 按文件列出，
不用记测试在哪。

**不引 mock 框架**：`bootstrap::build(&paths)` 就是最好的装配入口——
给它一个临时目录，拿到的就是真实的 `AppState`。
Mock 一层只会让「测试通过但生产挂掉」。
