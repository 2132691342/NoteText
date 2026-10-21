# 12 · 测试与验证

## 定位

`src-tauri/tests/integration.rs`（5 条链）+ `scripts/check.ps1`（验证入口）。
完整的测试规范在 [AGENTS.md §3](../AGENTS.md)，这里只讲「为什么是这 5 条」。

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

按域聚合而不是按功能：一个域一条链，改动一个域时 `-Chain` 一次命中整条链，
不必知道它内部有几个断言。

### 五条链

| 入口 | 守护的坏事 |
|---|---|
| `test_bootstrap_chain` | **启动即失败**（DB 建不起来，用户看到原生错误框）；**重启后窗口尺寸与标签顺序全丢**；书签连按两次 F2 多出一行；**空面板崩白屏**（前端漏写 `?? []` 就 `Cannot read properties of null`） |
| `test_file_roundtrip_chain` | **打开 CRLF 文件按一下保存，整个文件行尾变 LF**，同事那边 diff 出几千行；GBK 打开一片乱码；保存后文件头多三个字节；**保存中途断电留下半个文件**，同目录堆一堆 `.xxx.tmp` |
| `test_text_pipeline_chain` | **Ctrl+/ 把代码注释坏**（半注释块出 `// //`）；「转到行」在中文上跳错位置；统计把字节当字符，用户以为数字不对 |
| `test_tools_chain` | **浮窗给出错误结果或卡死**：坏 JSON 不说在哪一行、diff 增删算错、正则死循环挂住 UI；**算出来的摘要与 certutil 对不上**，用户校验文件时误判文件被改 |
| `test_batch_chain` | **批量替换静默改掉用户没勾选的文件**；**重命名把已有文件覆盖**——两种都是「用户的文件没了」，且没有撤销键 |

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

## 约束

- 一个断言只讲一种坏法：能写成「A、B、C 都对」的断言，
  失败时报文说不清坏的是哪个；
- 同一个行为只断言一次：repo 的语义归 repo 的链，service 的归 service 的链；
- 入口一律叫 `test_<domain>_chain`——它就是 grep 的把手，也是 `-Chain` 的入参；
- **不写 `-short` 隔离**：当前没有依赖真实网络 / 真实 git 的链路。

## 取舍

**5 条链放在一个文件**：Rust 的集成测试按「测试二进制」分文件，
5 条链放一个 `integration.rs` 读起来是一份完整的契约清单，
拆开反而要跳五个文件。

**不引 mock 框架**：`bootstrap::build(&paths)` 就是最好的装配入口——
给它一个临时目录，拿到的就是真实的 `AppState`。
Mock 一层只会让「测试通过但生产挂掉」。
