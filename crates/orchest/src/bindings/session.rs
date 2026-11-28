//! Explicit local Session v2 preparation shared by both SDKs.

use std::path::PathBuf;
use std::sync::Arc;

use crate::model::Message;
use crate::run::actor::{ensure_fresh_session, SessionBinding};
use crate::run::{AgentConfig, AgentRun, RunInput};
use crate::session::{sqlite::SqliteSessionWriter, SessionError, SqliteSessionStore};

#[cfg(feature = "session-evidence")]
use crate::session::evidence::{AttemptReader, EvidenceError, EvidenceStore};

use super::run::{BindingAgent, BindingRunError, BindingRunReservation, BindingRunStream};

pub struct BindingSessionOptions {
    path: PathBuf,
    session_id: String,
    #[cfg(feature = "session-evidence")]
    evidence: Option<EvidenceStore>,
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
            #[cfg(feature = "session-evidence")]
            evidence: None,
        })
    }

    #[cfg(feature = "session-evidence")]
    pub fn with_evidence(mut self, evidence: Option<EvidenceStore>) -> Self {
        self.evidence = evidence;
        self
    }

    #[cfg(feature = "session-evidence")]
    pub async fn authorize_evidence(
        self,
        evidence: EvidenceStore,
    ) -> Result<AttemptReader, BindingRunError> {
        let store = SqliteSessionStore::open_reader(self.path)
            .await
            .map_err(|_| EvidenceError::Storage)?;
        Ok(evidence
            .bind(Arc::new(store), self.session_id)
            .authorize_attempts()
            .await?)
    }

    async fn bind(
        self,
        config: AgentConfig,
    ) -> Result<(AgentConfig, SqliteSessionWriter), SessionError> {
        let (store, writer) =
            SqliteSessionStore::open_for_sdk(self.path, self.session_id.clone()).await?;
        let store: Arc<dyn crate::session::SessionStore> = Arc::new(store);
        let config = config.with_session_store(Arc::clone(&store), &self.session_id);
        #[cfg(feature = "session-evidence")]
        let config = match self.evidence {
            Some(evidence) => config.with_session_evidence(evidence.bind(store, self.session_id)),
            None => config,
        };
        Ok((config, writer))
    }
}

impl BindingRunReservation {
    pub async fn start(
        self,
        agent: BindingAgent,
        input: RunInput,
        messages: Vec<Message>,
        session: Option<BindingSessionOptions>,
    ) -> Result<BindingRunStream, BindingRunError> {
        let BindingAgent {
            config,
            model,
            registry,
        } = agent;
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
        agent: BindingAgent,
        session: BindingSessionOptions,
        input: Option<RunInput>,
    ) -> Result<BindingRunStream, BindingRunError> {
        let BindingAgent {
            config,
            model,
            registry,
        } = agent;
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
