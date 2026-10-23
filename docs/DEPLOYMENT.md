# 构建与分发

## 产物

```
target/release/                        # workspace 根的 target/（根 Cargo.toml 是 workspace root）
├── NoteText.exe                    # 主程序（前端资源已 embed，单文件可跑）
└── bundle/
    └── nsis/
        └── NoteText_1.0.0_x64-setup.exe   # NSIS 安装包
```

产物**不入库**（`.gitignore` 排除 `/target/`）：本地构建一次就产生几百 MB，
入库只会让 clone 变慢。

## 流水线

两条 GitHub Actions，分工是「日常门禁」与「出包」，两条都跑与本地同一个入口：

| 工作流 | 触发 | 做什么 |
|---|---|---|
| `.github/workflows/ci.yml` | push main / PR / 手动（**tag 不触发**） | 装 Rust + Node → 跑 `pwsh scripts/check.ps1 -Full`；失败时脚本把「哪条链坏了、错在哪一行」写进 job summary |
| `.github/workflows/release.yml` | tag `v<major>.<minor>.<patch>` / 手动 | 静态门禁（rustfmt / check / clippy）+ `tauri build --bundles nsis` → 校验产物体积 → 建 GitHub Release 并上传安装器 |

**「推 main 不发版、只有推 tag 才发版」是靠两个工作流各自的触发条件锁死的**：

- `ci.yml` 的 `push` 显式写了 `branches: [main]` **与** `tags-ignore: ['v*']`。
  只写 `branches` 时，GitHub 的过滤器是「或」关系，tag 推送仍会命中 `push` 触发器；
  补上 `tags-ignore` 才能把 tag 彻底排除在这条之外
- `release.yml` 只认 `push.tags`，且用 `v[0-9]+.[0-9]+.[0-9]+` 而不是 `v*`：
  形如 `v1.0.0` 的 tag 才会发版，顺带挡掉误推的 `v-next` 之类分支式 tag
- `release.yml` 在上传前校验产物存在且体积未异常偏大（>20MB 即失败）：
  前端误引一个大包会让安装包翻倍，本地不易察觉，这里是最后一道回归信号

其余约定，改动流水线前要知道：

- **门禁与出包分层**：日常质量门禁集中在 `ci.yml`；`release.yml` 只做静态编译门禁，
  不重跑已由 main 门过的测试（`tauri build` 自己会跑前端构建）
- **两个 job 都跑 `windows-latest`**：目标平台只有 Windows amd64，
  门禁与出包必须在同一套系统调用上验
- **tauri-cli 用 npm 版**：`frontend/package.json` 的 devDependencies 已锁
  `@tauri-apps/cli@^2.12.4`，与 tauri crate 同一条发布线。不装 `cargo-tauri`：
  多一份全局工具链，版本还可能与 crate 对不上
- **必须从仓库根跑 CLI**：CLI 要向上找到 `src-tauri/tauri.conf.json`，
  从 `frontend/` 里跑会报「Couldn't recognize the current folder as a Tauri project」
- **参数是 `--bundles`（复数）**：`--bundler` 是 1.x 的写法，2.x 已移除
- **NSIS 工具链要缓存**：Tauri CLI 首次打包会从 GitHub 下载 NSIS（~10MB），
  不缓存则每次发版都要重下，网络抖动就会让发版失败

## 构建

```powershell
# 从仓库根执行：CLI 要向上找到 src-tauri/tauri.conf.json
./frontend/node_modules/.bin/tauri build              # 产物 + bundle（按 tauri.conf.json 的 targets）
./frontend/node_modules/.bin/tauri build --bundles nsis  # 只出安装包
./frontend/node_modules/.bin/tauri build --debug        # 调试构建（快，但体积大）
```

**首次打包要联网**：Tauri CLI 会下载 NSIS 工具链到 `~/AppData/Local/tauri`，
之后离线可用。

产物只有一个 exe：前端资源由 `tauri.conf.json` 的 `frontendDist` 指向
`frontend/dist`，由 `tauri-build` 在编译期嵌进二进制。
不内置运行时（Node / Python 都不需要），所以 exe 只有 ~5MB。

### 构建体积参考

| 形态 | 体积 |
|---|---|
| `--debug` exe | ~40MB |
| `--release` exe | ~5MB（strip symbols） |
| NSIS 安装包 | ~2.5MB（LZMA 压缩） |

release profile 设了 `strip = "symbols"` 与 `panic = "abort"`，
但**没开 `lto = true`**：全量 LTO 让每次 build 多等 30s+，
而编辑器启动已 < 1s，不值得。

## 安装器

`tauri.conf.json` 的 `bundle.windows.nsis` 配置：

| 项 | 值 | 理由 |
|---|---|---|
| `installMode` | `currentUser` | 不需要管理员权限；多用户机器不互相干扰 |
| `languages` | `SimpChinese`, `English` | 目标用户是中英双语 |
| `fileAssociations` | txt / md / log / json / yml / toml / ini / csv / xml | 常见的纯文本类型 |

**文件关联用 `role: "Viewer"`**：出现在「打开方式」列表里，
不抢占用户已有的默认程序。抢占默认是安装器最招人骂的行为之一。

### 卸载与数据

卸载器**不清数据**：`%APPDATA%\NoteText\` 里的书签 / 片段 / 宏 / 草稿是用户资产，
卸载只该删程序。要清空：设置 → 关于 → 打开数据目录，手动删。

开机自启**本期不做**：编辑器的打开频率低，开机自启的收益小于「登录后看到一个
不需要的窗口」的烦人程度。

## 分发注意

- 单实例：`tauri-plugin-single-instance` 在二次启动时把文件路径转交给已有实例；
- 无网络请求：应用不发任何遥测、不做版本检查，用户数据不出本机；
- WebView2 Runtime：Windows 11 与大部分 Windows 10 已内置。
  没装的机器上启动会弹原生消息框提示（`platform::dialog_win`）。

## 取舍

- **前端资源 embed 进 exe**：分发只有一个文件，没有「忘了拷 dist 目录」这种失败模式；
  代价是 exe 从 ~2MB 变成 ~5MB，以及每次前端改动都要重新链接。
- **不做自动更新**：没有更新服务与签名校验链路，安装包就是全部；
  代价是修 bug 要靠用户重新下载安装，覆盖安装时先杀常驻进程
  （托盘常驻时 exe 仍被占用，NSIS 的安装脚本会先 `taskkill`）。
- **只发 Windows amd64**：单平台让托盘、WebView2、路径分隔符都不必抽象；
  代价是其他平台完全不可用。Tauri 本身支持 mac / linux，
  真要扩平台时先扩 `platform/` 的平台分文件，再处理菜单快捷键差异。
- **NSIS 而不是 MSI**：NSIS 支持 currentUser 安装（不需要管理员），
  且脚本可读可改；MSI 的每用户安装要额外工具链。
