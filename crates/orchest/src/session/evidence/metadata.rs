use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{
    store::MAX_PAGE_RECORDS, AttemptOutcome, AttemptPurpose, EvidenceError, EvidenceKind,
    EvidenceSession,
};
use crate::run::RunId;

/// A metadata-only reader fixed to the Session instance present at authorization.
#[derive(Clone)]
pub struct AttemptReader {
    session: EvidenceSession,
    instance: RunId,
    cursor_key: Uuid,
}

#[derive(Debug, Deserialize, Serialize)]
#[non_exhaustive]
pub struct AttemptPage {
    pub records: Vec<AttemptMetadata>,
    pub next_cursor: Option<String>,
    pub invalid_records: usize,
    pub expired_records: u64,
    pub observed_drops: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AttemptMetadata {
    AttemptStarted {
        attempt_id: Uuid,
        run_id: RunId,
        step: u32,
        ordinal: u32,
        purpose: AttemptPurpose,
        provider: Option<String>,
        model: Option<String>,
    },
    AttemptFinished {
        attempt_id: Uuid,
        run_id: RunId,
        step: u32,
        outcome: AttemptOutcome,
    },
}

impl EvidenceSession {
    /// Authorize access to the current instance's attempt metadata.
    pub async fn authorize_attempts(&self) -> Result<AttemptReader, EvidenceError> {
        let reader = AttemptReader {
            session: self.clone(),
            instance: self.instance().await?,
            cursor_key: Uuid::new_v4(),
        };
        reader.read(None, 1).await?;
        Ok(reader)
    }
}

impl AttemptReader {
    /// Read up to `limit` raw records and return their allowed metadata.
    /// Follow the cursor even when every record in a page was filtered out.
    pub async fn read(
        &self,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<AttemptPage, EvidenceError> {
        if !(1..=MAX_PAGE_RECORDS).contains(&limit) {
            return Err(EvidenceError::InvalidPageSize);
        }
        let cursor = cursor.map(|cursor| self.parse_cursor(cursor)).transpose()?;
        self.check_instance().await?;
        let page = self
            .session
            .database
            .read_bounded(self.instance, cursor, limit)
            .await?;
        if page.revoked {
            return Err(EvidenceError::SessionUnavailable);
        }
        self.check_instance().await?;
        Ok(AttemptPage {
            records: page
                .records
                .into_iter()
                .filter_map(|record| match record.kind {
                    EvidenceKind::AttemptStarted {
                        attempt_id,
                        ordinal,
                        purpose,
                        provider,
                        model,
                    } => Some(AttemptMetadata::AttemptStarted {
                        attempt_id,
                        run_id: record.run_id,
                        step: record.step,
                        ordinal,
                        purpose,
                        provider,
                        model,
                    }),
                    EvidenceKind::AttemptFinished {
                        attempt_id,
                        outcome,
                    } => Some(AttemptMetadata::AttemptFinished {
                        attempt_id,
                        run_id: record.run_id,
                        step: record.step,
                        outcome,
                    }),
                    _ => None,
                })
                .collect(),
            next_cursor: page.next_cursor.map(|cursor| self.encode_cursor(cursor)),
            invalid_records: page.invalid_records,
            expired_records: page.expired_records,
            observed_drops: page.observed_drops,
        })
    }

    /// Revoke evidence for the authorized instance.
    pub async fn revoke(&self) -> Result<(), EvidenceError> {
        self.check_instance().await?;
        self.session.database.revoke(self.instance).await
    }

    async fn check_instance(&self) -> Result<(), EvidenceError> {
        if self.session.instance().await? == self.instance {
            Ok(())
        } else {
            Err(EvidenceError::SessionUnavailable)
        }
    }

    fn encode_cursor(&self, position: i64) -> String {
        let mut digest = Sha256::new();
        digest.update(position.to_be_bytes());
        digest.update(self.cursor_key.as_bytes());
        format!("{position}:{:x}", digest.finalize())
    }

    fn parse_cursor(&self, cursor: &str) -> Result<i64, EvidenceError> {
        let position = cursor
            .split_once(':')
            .and_then(|(position, _)| position.parse::<i64>().ok())
            .filter(|position| *position > 0 && self.encode_cursor(*position) == cursor);
        position.ok_or(EvidenceError::InvalidCursor)
    }
}
