# 02 · 数据库与迁移

## 定位

`src-tauri/src/db/mod.rs` 负责打开连接、配置 WAL、跑迁移。
`repo/` 只写 SQL，不关心连接怎么来的。

## 设计

### 打开与配置

```rust
Db::open(&paths.db_path)?     // 生产
Db::open_in_memory()?         // 测试
```

连接配置（顺序固定，都是 fail-fast）：

| pragma | 值 | 理由 |
|---|---|---|
| `journal_mode` | WAL | 读不阻塞写，编辑器保存时界面不卡 |
| `synchronous` | NORMAL | 崩溃最多丢最后一笔已提交事务；FULL 每次提交都 fsync，大文件保存会卡 |
| `foreign_keys` | ON | 默认就要生效，不靠调用方记得开 |
| `busy_timeout` | 5000 | 两个窗口同时写时的兜底（正常不会发生，单实例已挡掉） |

### Db 的接口

```rust
impl Db {
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T>
}
```

`Mutex<Connection>` 而不是 `RwLock`：SQLite 写要独占，
读多写少的优化在这里没有意义（编辑器场景写操作都是毫秒级小事务）。

**不重试**：并发冲突由 `busy_timeout` 兜，重试会把「SQL 写错」也重试三遍才报，
排障更难。

### 迁移

```sql
-- 基础 schema：CREATE TABLE IF NOT EXISTS，幂等
CREATE TABLE IF NOT EXISTS bookmarks (...);
```

版本记在 `PRAGMA user_version`：

```rust
if v < 1 { conn.execute_batch(MIGRATION_V1)?; conn.execute_batch("PRAGMA user_version = 1")?; }
```

下一个迁移写成 `MIGRATION_V2` 并加一段 `if v < 2`：顺序执行，
跳版本（v0 → v2）时两段都会跑。

## 约束

- **DB 打不开时 fail-fast**：书签 / 片段 / 宏 / 会话恢复都依赖它，
  半残启动会让用户在第一次打开面板时才看到报错，反而误导；
- 新增字段只增不删、零值兜底；废弃字段留在表里（改语义靠代码兼容），不主动 DROP；
- `#[serde(default)]` 让旧配置读出来缺项自动补默认，不需要写迁移代码；
- 测试必须在结束时关连接（Windows 上句柄不释放，`tempdir` 删不掉）。

## 取舍

**rusqlite bundled 而不是系统 SQLite**：用户机器不需要单独装 SQLite，
也没有 CGO 依赖链（Wails/Go 那套要装 gcc 的麻烦不存在）。
代价是二进制 +1.5MB——对 5MB 的包来说是显著的，但换来的零依赖值得。

**不引 ORM**：GORM / diesel 那层抽象在这个规模下是净负担——
7 张表、几十个字段，手写 SQL 更短也更快。要引 ORM 的门槛是
「跨库可移植」，而 NoteText 只跑 SQLite。
