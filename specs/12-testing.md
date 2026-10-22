# 12 · 测试与验证

## 定位

`src-tauri/tests/`（按域分文件，4 条长链路）+ `scripts/check.ps1`（唯一验证入口）。
测试规范见 [AGENTS.md §3](../AGENTS.md)，这里解释「为什么这么组织」。

## 设计

### 判据

写之前先问：**这条测试失败时，用户界面上会出现什么坏事？**
说不出那句坏事，它就不该存在。

| 保留 | 不写 |
|---|---|
| 跨模块的长链路（装配 → 建表 → 会话往返 → 序列化契约） | 单个纯函数的输入输出 |
| 协议硬约束（空数组永不 null、行尾按原样写回、BOM 剥离与补回） | 简单 CRUD、字段映射、枚举转换 |
| 安全护栏（批量操作不覆盖别人的文件、目录不进自己的子目录） | 只验「不 panic」「非 nil」的弱断言 |

纯函数断言是最大的一类冗余：函数签名就是契约，编译器挡了一半，
剩下一半坏了也只影响一个工具窗的一个按钮，不值得为它付出
「每次改代码都要跑一遍」的代价。**护栏类断言失败时对应的都是
「用户的文件没了 / 界面崩白屏」级别的事故**，必须常驻。

### 文件划分：一个域一个文件

```
src-tauri/tests/
├── common/mod.rs      临时目录、装配、会话标签样本
├── session_chain.rs   test_bootstrap_chain
├── file_chain.rs      test_file_domain_chain
├── tools_chain.rs     test_tools_contract_chain
└── batch_chain.rs     test_batch_safety_chain
```

改某个域只读那个文件，不必通读全部测试。Rust 集成测试按「测试二进制」
分文件，一个域一个文件，编译单元也跟着分开。
入口一律叫 `test_<domain>_chain`——它是 grep 的把手。

`common/mod.rs` 只放**每个域都要用**的东西。域内的断言不许下沉到这里——
共用层一旦长起来，每个测试文件都会拖着一堆用不上的依赖。

### 链索引

| 文件 | 链 | 守护的坏事 |
|---|---|---|
| `session_chain` | `test_bootstrap_chain` | **启动即失败**（DB 建不起来）；**重启后标签顺序与窗口尺寸全丢**；书签重复加出双份；**空面板崩白屏**（空列表必须序列化成 `[]` 而非 `null`） |
| `file_chain` | `test_file_domain_chain` | **打开 CRLF 文件按一下保存，整个文件行尾变 LF**（同事那边 diff 出几千行）；保存后文件头多三个字节；**原子写失败留下半个文件**；GBK 打开一片乱码；**重命名覆盖已有文件**；**目录拖进自己子目录树成环** |
| `tools_chain` | `test_tools_contract_chain` | 坏 JSON 不说在哪一行，用户找不到错处；正则空匹配不推进挂死界面；**算出的摘要与系统命令对不上**（用户误判文件被改）；**Ctrl+/ 把代码注释坏**（去注释回不到原文、半注释块出 `// //`） |
| `batch_chain` | `test_batch_safety_chain` | **批量替换静默改掉用户没勾选的文件**；**重命名覆盖已有文件**——用户的文件没了，且没有撤销键 |

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
自动 checkpoint WAL，进程退出本身会释放文件句柄，所以测试里不需要显式关
（但要保证 `state` 在 `dir` 之前 drop）。

## 验证入口

`scripts/` 下只有一个脚本，全部验证动作收在这里。

```
pwsh scripts/check.ps1          快门：rustfmt → cargo check → 全量测试
pwsh scripts/check.ps1 -Full    门禁：+ clippy -D warnings + 前端构建
pwsh scripts/check.ps1 -List    列出全部测试链
pwsh scripts/check.ps1 -Detail  打印每条链的名字与结果
```

### 分档的理由

| 档位 | 什么时候用 | 省的是什么 |
|---|---|---|
| 默认快门 | 日常改代码 | 一次跑完全部 rustfmt / check / 测试 |
| `-Full` | 改了前端、提交前 | clippy 的 lint 与 `vue-tsc` 类型检查，编译期发现不了 |
| `-List` | 不知道入口叫什么 | 列出 4 条链的名字 |

**耗时大头是编译与链接测试二进制，断言本体在亚秒级**——
全量测试 < 1 秒，所以不设「只跑一条链」的开关：加开关省不了多少时间，
却要多记一个参数。

### 失败摘要

脚本把测试输出落到 `.check/test-output.log`，失败时摘出：

```
^test .* FAILED | ^---- | ^thread .* panicked | \.rs:\d+:
```

抽不到再退回末尾 30 行。CI 里同一份内容追加进 job summary，
不必下载 artifact。

## 约束

- 一个断言只讲一种坏法：能写成「A、B、C 都对」的断言，
  失败时报文说不清坏的是哪个；
- 同一个行为只断言一次：repo 的语义归 repo 的链，编排层的语义归 service 的链；
- 昂贵的 setup（临时目录、装配服务容器）只在链入口准备一次；
- 不写 `-short` 隔离：当前没有依赖真实网络 / 真实版本库的链路。

## 取舍

| 选择 | 优势 | 代价 |
|---|---|---|
| 按域拆 4 个文件 | 改某个域只读那个文件；编译单元分离 | 跨域的阅读顺序被打断，靠文件名与域同名（`file_chain.rs` ↔ `service/file.rs`）补偿 |
| 不引 mock 框架 | `bootstrap::build(&paths)` 给临时目录就拿到真实 `AppState`，没有测试替身与生产不一致的空间 | 每条链都要真的建一次库（亚秒级，可接受） |
| 断言写人话失败原因（`assert_eq!(x, y, "应为 CRLF")`） | 失败报文直接指到「哪条规则被破坏」 | 断言本身长了三四个字 |
