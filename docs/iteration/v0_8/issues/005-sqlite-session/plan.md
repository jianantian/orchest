# 005 · SqliteSessionStore — 实施计划

## 前置条件

- 004（SessionStore trait + InMemorySessionStore）已合入 main
- `cargo test --workspace` 全绿

---

## 步骤

### 步骤 1：`crates/agent-runtime-core/Cargo.toml` — 新增可选依赖

```toml
[dependencies]
# ... 现有依赖 ...
rusqlite = { version = "0.31", features = ["bundled"], optional = true }

[features]
sqlite-session = ["dep:rusqlite"]
```

确认 `agent-runtime-py/Cargo.toml` 和 `agent-runtime-node/Cargo.toml` 中对 `agent-runtime-core` 的依赖**不**带 `features = ["sqlite-session"]`。

### 步骤 2：新建 `session/sqlite.rs`

完整文件用 `#[cfg(feature = "sqlite-session")]` 属性（或在 `session/mod.rs` 中条件声明）。

**结构体与构造函数**：

```rust
use rusqlite::{Connection, params};
use std::sync::{Arc, Mutex};
use crate::session::{SessionSnapshot, SessionStore, SessionError};

pub struct SqliteSessionStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteSessionStore {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, SessionError> {
        let conn = Connection::open(path)
            .map_err(|e| SessionError::Storage(e.to_string()))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self, SessionError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| SessionError::Storage(e.to_string()))?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self, SessionError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                session_id TEXT PRIMARY KEY,
                schema_version TEXT NOT NULL,
                snapshot_json TEXT NOT NULL,
                saved_at INTEGER NOT NULL
            );"
        ).map_err(|e| SessionError::Storage(e.to_string()))?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }
}
```

**SessionStore 实现**（各方法用 `spawn_blocking` 包裹）：

`save`：
```rust
async fn save(&self, session_id: &str, snapshot: &SessionSnapshot) -> Result<(), SessionError> {
    let json = serde_json::to_string(snapshot)?;
    let schema = snapshot.schema_version.clone();
    let id = session_id.to_string();
    let conn = Arc::clone(&self.conn);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    tokio::task::spawn_blocking(move || {
        let c = conn.lock().map_err(|e| SessionError::Storage(e.to_string()))?;
        c.execute(
            "INSERT OR REPLACE INTO sessions (session_id, schema_version, snapshot_json, saved_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, schema, json, now as i64],
        ).map_err(|e| SessionError::Storage(e.to_string()))?;
        Ok(())
    }).await.map_err(|e| SessionError::Storage(e.to_string()))?
}
```

`load` / `delete` / `list` 类似结构。

`load` 额外校验 schema_version：
```rust
if row_schema != SessionSnapshot::CURRENT_SCHEMA_VERSION {
    return Err(SessionError::SchemaMismatch {
        expected: SessionSnapshot::CURRENT_SCHEMA_VERSION.into(),
        found: row_schema,
    });
}
```

### 步骤 3：`session/mod.rs` — 条件 re-export

```rust
#[cfg(feature = "sqlite-session")]
pub mod sqlite;
#[cfg(feature = "sqlite-session")]
pub use sqlite::SqliteSessionStore;
```

### 步骤 4：`src/lib.rs` — 条件 re-export

```rust
#[cfg(feature = "sqlite-session")]
pub use session::SqliteSessionStore;
```

### 步骤 5：单元测试

在 `session/sqlite.rs` 底部（`#[cfg(all(test, feature = "sqlite-session"))]`）：

1. `sqlite_save_and_load_roundtrip`
2. `sqlite_schema_version_mismatch`：save 后手动 UPDATE schema_version，load 返回 SchemaMismatch
3. `sqlite_delete`：save → delete → load 返回 None
4. `sqlite_list`：save 多个 session → list 返回全部 session_id
5. `sqlite_concurrent_saves`：spawn 多个 tokio task 并发 save，不死锁，全部成功

运行时需 `--features sqlite-session`：
```bash
cargo test --workspace --features sqlite-session
cargo clippy --workspace --features sqlite-session -- -D warnings
```

也确认默认 feature 下测试仍通过：
```bash
cargo test --workspace
```
