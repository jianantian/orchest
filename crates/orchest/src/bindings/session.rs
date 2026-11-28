//! Explicit local Session v2 preparation shared by both SDKs.

use std::path::PathBuf;
use std::sync::Arc;

use crate::model::{Message, ModelAdapter};
use crate::run::actor::{ensure_fresh_session, SessionBinding};
use crate::run::{AgentConfig, AgentRun, RunInput};
use crate::session::{sqlite::SqliteSessionWriter, SessionError, SqliteSessionStore};
use crate::tool::registry::ToolRegistry;

use super::run::{BindingRunError, BindingRunReservation, BindingRunStream};

pub struct BindingSessionOptions {
    path: PathBuf,
    session_id: String,
}

impl BindingSessionOptions {
    pub fn new(path: Option<String>, session_id: Option<String>) -> Result<Self, BindingRunError> {
        let (Some(path), Some(session_id)) = (
            path.filter(|value| !value.is_empty() && value != ":memory:"),
            session_id.filter(|value| !value.is_empty()),
        ) else {
            return Err(SessionError::NotConfigured.into());
        };
        Ok(Self {
            path: path.into(),
            session_id,
        })
    }

    async fn bind(
        self,
        config: AgentConfig,
    ) -> Result<(AgentConfig, SqliteSessionWriter), SessionError> {
        let (store, writer) =
            SqliteSessionStore::open_for_sdk(self.path, self.session_id.clone()).await?;
        Ok((
            config.with_session_store(Arc::new(store), self.session_id),
            writer,
        ))
    }
}

impl BindingRunReservation {
    pub async fn start(
        self,
        config: AgentConfig,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
        input: RunInput,
        messages: Vec<Message>,
        session: Option<BindingSessionOptions>,
    ) -> Result<BindingRunStream, BindingRunError> {
        let (config, writer) = if let Some(session) = session {
            let (config, writer) = session.bind(config).await?;
            let binding =
                SessionBinding::from_config(&config).ok_or(SessionError::NotConfigured)?;
            ensure_fresh_session(&binding).await?;
            (config, Some(writer))
        } else {
            (config, None)
        };
        let (handle, events) =
            AgentRun::start_with_messages(config, messages, input, model, registry);
        Ok(BindingRunStream::new(
            self.attach_with_writer(handle, writer),
            events,
        ))
    }

    pub async fn resume(
        self,
        config: AgentConfig,
        model: Arc<dyn ModelAdapter>,
        registry: ToolRegistry,
        session: BindingSessionOptions,
        input: Option<RunInput>,
    ) -> Result<BindingRunStream, BindingRunError> {
        let (config, writer) = session.bind(config).await?;
        let (handle, events) = match input {
            Some(input) => AgentRun::resume_with_input(config, input, model, registry).await?,
            None => AgentRun::resume(config, model, registry).await?,
        };
        Ok(BindingRunStream::new(
            self.attach_with_writer(handle, writer),
            events,
        ))
    }
}
