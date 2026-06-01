# 005 · SqliteSessionStore

## 背景

004 提供了 `SessionStore` trait 和 `InMemorySessionStore`（进程内临时存储）。`InMemorySessionStore` 进程退出即丢失，无法支持"崩溃恢复"或"跨进程多轮对话"场景。

`SqliteSessionStore` 是第一个持久化后端，提供本地文件级 session 存储，无需外部数据库。

本 issue 依赖 004（SessionStore trait 落地）。

## 目标

实现 `SqliteSessionStore`，以 SQLite 文件作为持久化后端，存储 `SessionSnapshot` 的 JSON 序列化内容。

## 范围

### Cargo 依赖策略

```toml
# crates/agent-runtime-core/Cargo.toml

[dependencies]
rusqlite = { version = "0.31", features = ["bundled"], optional = true }

[features]
sqlite-session = ["dep:rusqlite"]
```

- `bundled` feature 静态链接 sqlite3，无外部系统库依赖
- `agent-runtime-py` / `agent-runtime-node` 的 `Cargo.toml` **不**开启此 feature（不强制传播）
- 需要 SQLite 后端的用户在其应用 crate 中开启 `agent-runtime-core/sqlite-session`

### 实现

```rust
// session/sqlite.rs（#[cfg(feature = "sqlite-session")] 整文件）

use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

pub struct SqliteSessionStore {
    conn: Mutex<Connection>,
}

impl SqliteSessionStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SessionError>;
    pub fn open_in_memory() -> Result<Self, SessionError>;
}
```

Schema（在 `open` 时建表）：

```sql
CREATE TABLE IF NOT EXISTS sessions (
    session_id TEXT PRIMARY KEY,
    schema_version TEXT NOT NULL,
    snapshot_json TEXT NOT NULL,
    saved_at INTEGER NOT NULL   -- Unix timestamp（秒）
);
```

`snapshot_json` 存储完整 `SessionSnapshot` 的 JSON 字符串。

**save**：`INSERT OR REPLACE INTO sessions (session_id, schema_version, snapshot_json, saved_at) VALUES (?, ?, ?, ?)`

**load**：`SELECT snapshot_json, schema_version FROM sessions WHERE session_id = ?`，先验证 schema_version，再反序列化 JSON

**delete**：`DELETE FROM sessions WHERE session_id = ?`

**list**：`SELECT session_id FROM sessions ORDER BY saved_at DESC`

**并发安全**：`rusqlite::Connection` 不是 `Send`，使用 `Mutex<Connection>` 包裹，`async fn` 通过 `tokio::task::spawn_blocking` 在阻塞线程执行。

```rust
#[async_trait]
impl SessionStore for SqliteSessionStore {
    async fn save(&self, session_id: &str, snapshot: &SessionSnapshot) -> Result<(), SessionError> {
        let json = serde_json::to_string(snapshot)?;
        let schema = snapshot.schema_version.clone();
        let id = session_id.to_string();
        let conn = &self.conn;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        tokio::task::spawn_blocking(move || {
            // 需要访问 conn，但 Mutex 不能 move 进 spawn_blocking
            // 实际实现：用 Arc<Mutex<Connection>> 而非 Mutex<Connection>
        }).await.map_err(|e| SessionError::Storage(e.to_string()))?
    }
}
```

修正：使用 `Arc<Mutex<Connection>>`：

```rust
pub struct SqliteSessionStore {
    conn: Arc<std::sync::Mutex<Connection>>,
}
```

`spawn_blocking` 时 clone Arc 并 move 进 closure。

### 模块组织

```rust
// session/mod.rs
#[cfg(feature = "sqlite-session")]
pub mod sqlite;
#[cfg(feature = "sqlite-session")]
pub use sqlite::SqliteSessionStore;
```

## 验收标准

- [ ] `agent-runtime-core/Cargo.toml` 中 `rusqlite` 为可选依赖，`sqlite-session` feature 可控
- [ ] `SqliteSessionStore::open(path)` 可打开或创建 SQLite 文件，建表成功
- [ ] `SqliteSessionStore::open_in_memory()` 用于测试
- [ ] save → load → 得到等价 SessionSnapshot（JSON round-trip 正确）
- [ ] load 时 schema_version 不匹配返回 `SessionError::SchemaMismatch`
- [ ] delete 后 load 返回 `Ok(None)`
- [ ] list 返回所有 session_id
- [ ] 并发 save 不死锁（Mutex + spawn_blocking）
- [ ] `agent-runtime-py` 和 `agent-runtime-node` 的默认构建（不开启 sqlite-session feature）不受影响
- [ ] `cargo test --workspace` 全绿（sqlite-session feature 下和默认 feature 下都通过）
- [ ] `cargo clippy --workspace -- -D warnings` 全绿（两种 feature 配置）

## 注意事项

- `rusqlite` bundled feature 会在编译时编译 C 代码，首次构建较慢；CI 应缓存 `target/`
- `SqliteSessionStore` 的测试使用 `open_in_memory()`，不产生文件
- 测试使用 `#[cfg(feature = "sqlite-session")]` guard，默认 `cargo test` 不运行这些测试；需在 CI 中显式运行 `cargo test --features sqlite-session`
- 若 `spawn_blocking` 的 closure panic（rusqlite 操作失败），返回 `SessionError::Storage(e.to_string())`，不 propagate panic
