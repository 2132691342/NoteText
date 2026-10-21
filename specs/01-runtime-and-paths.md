# 01 · 运行时与路径

## 定位

NoteText 全部落盘位置由一个模块决定：`src-tauri/src/pkg/paths.rs`（纯计算）+
`src-tauri/src/app/paths.rs`（进程内唯一实例）。

## 设计

### 路径解析

```
NOTETEXT_HOME（环境变量，测试与开发隔离）
  └─ 有 → 用它作为根
  └─ 无 → dirs::data_dir()/NoteText
```

派生路径：

| 路径 | 值 |
|---|---|
| `data_dir` | 根 |
| `db_path` | `根/notetext.db` |
| `config_path` | `根/config.json`（预留，固定项目前都在 DB） |
| `log_dir` | `根/logs` |
| `backup_dir` | `根/backup` |

用 `dirs` 而不是手拼 `env::var("APPDATA")`：漫游配置与打包后进程环境下
前者仍可靠，后者在某些启动方式（计划任务 / 服务）下取不到。

### 进程内唯一实例

```rust
static PATHS: OnceLock<AppPaths> = OnceLock::new();
```

用 `OnceLock` 而不是每次解析：数据目录一旦定下，整个进程不能改
（改了就会出现「配置写在 A，数据库在 B」）。`app::paths::get()` 在未初始化时
panic——装配期就初始化了，运行期取不到说明装配有 bug，早炸比晚炸好。

### 路径越界判据

```rust
pub fn is_within(root: &Path, candidate: &Path) -> bool
```

比较前先 `canonicalize`：Windows 上同一路径的短名（`RUNNER~1`）与长名、
符号链接形式字面不等，直接 `starts_with` 会误判越界（或漏判）。
文件不存在时 canonicalize 失败——那说明它还没被创建，按字面判。

## 约束

- 测试一律 `AppPaths::from_root(tempdir)`，**绝不碰用户真实数据**；
- `NOTETEXT_HOME` 只用于测试与开发隔离，不在文档里宣传给用户
  （用户改了反而找不到数据）；
- 解析失败（`%APPDATA%` 都取不到）时启动即失败：此时没有可写位置，
  继续跑只会在用户第一次保存时炸。

## 取舍

**为什么不把数据放 exe 同级目录（便携模式）**：装进 `Program Files` 后会因
权限写入失败，而绿色软件与安装版两种形态要写两套路径逻辑。
代价是 U 盘随身携带的用户要手动设 `NOTETEXT_HOME`——这个场景极少。
