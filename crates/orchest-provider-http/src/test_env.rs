//! Test-only helpers for tests that change process environment variables.
//!
//! Tests run on parallel threads of one process, and the adapters read
//! variables such as API keys and OpenRouter routing headers at call time.
//! Every test that sets or removes a variable holds [`ENV_LOCK`] for its
//! whole body and changes variables only through [`EnvVarGuard`], which
//! restores the previous value on drop, including when the test panics.

use tokio::sync::{Mutex, MutexGuard};

/// One crate-wide lock for environment changes. `tokio::sync::Mutex` is not
/// poisoned by a panicking test, and async tests can hold it across `.await`.
static ENV_LOCK: Mutex<()> = Mutex::const_new(());

/// Takes the env lock from a synchronous `#[test]`.
pub(crate) fn lock_env() -> MutexGuard<'static, ()> {
    ENV_LOCK.blocking_lock()
}

/// Takes the env lock from an async test.
pub(crate) async fn lock_env_async() -> MutexGuard<'static, ()> {
    ENV_LOCK.lock().await
}

/// Sets or removes one variable and restores its previous value on drop.
pub(crate) struct EnvVarGuard {
    name: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    pub(crate) fn set(name: &'static str, value: &str) -> Self {
        let previous = std::env::var(name).ok();
        std::env::set_var(name, value);
        Self { name, previous }
    }

    pub(crate) fn remove(name: &'static str) -> Self {
        let previous = std::env::var(name).ok();
        std::env::remove_var(name);
        Self { name, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var(self.name, value),
            None => std::env::remove_var(self.name),
        }
    }
}
