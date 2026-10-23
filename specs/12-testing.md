# 12 · 测试与验证

## 定位

`src-tauri/tests/chain_tests.rs`（单文件、按 section 分四组：文件 / 会话 / 批量 / 工具箱）
+ `scripts/` 下两个脚本（唯一验证入口）。判据与条文见 [AGENTS.md §3](../AGENTS.md)，
这一篇只解释「为什么这么组织」。

## 为什么单文件、一个测试一种坏法

Rust 集成测试按「测试二进制」分文件，但 16 个测试收敛进一个文件，编译只链接一次 lib，
改代码不必等 4 个二进制各自编译。**测试函数名就是它守护的那件坏事**，失败时不需要
拿行号去文件里找第几条断言。一个函数只讲一种坏法：能写成「A、B、C 都对」的函数，
失败时报文说不清坏的是哪个。

```
src-tauri/tests/
└── chain_tests.rs   文件(5) / 会话(4) / 批量(4) / 工具箱(3) = 16 个测试
```

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
| 单文件收口 16 个测试 | 编译只链接一次 lib，改代码不必等 4 个二进制 | 跨域阅读顺序靠 section 注释补偿 |
| 一个测试一种坏法 | 失败报文直接指出坏在哪 | 每个测试各自建一次临时目录与装配（亚秒级） |
| 不引 mock 框架 | 测试与生产共用装配链，没有替身与生产不一致的空间 | 每个测试真的建一次库 |
| 断言写人话失败原因 | 失败报文直接指到哪条规则被破坏 | 断言长了几个字 |
