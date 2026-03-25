use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ferrum_proto::{ApprovalId, ApprovalRequest, ApprovalState, ExecutionId, ProposalId};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{ApprovalRepo, Result, StoreError};

use super::helpers::{enum_text, fetch_entity_by_id, from_json, to_json};
use sqlx::Row;

#[derive(Clone)]
pub struct SqliteApprovalRepo {
    pool: SqlitePool,
}

impl SqliteApprovalRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApprovalRepo for SqliteApprovalRepo {
    async fn insert(&self, approval: &ApprovalRequest) -> Result<()> {
        let raw_json = to_json(approval)?;
        sqlx::query(
            "INSERT INTO approvals (
                approval_id, intent_id, proposal_id, execution_id, action_digest,
                state, expires_at, created_at, raw_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(approval.approval_id.to_string())
        .bind(approval.intent_id.to_string())
        .bind(approval.proposal_id.to_string())
        .bind(approval.execution_id.map(|id| id.to_string()))
        .bind(&approval.action_digest)
        .bind(enum_text(&approval.state)?)
        .bind(approval.expires_at)
        .bind(approval.created_at)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, approval_id: ApprovalId) -> Result<Option<ApprovalRequest>> {
        fetch_entity_by_id(
            &self.pool,
            "approvals",
            "approval_id",
            &approval_id.to_string(),
        )
        .await
    }

    async fn update(&self, approval: &ApprovalRequest) -> Result<()> {
        let raw_json = to_json(approval)?;
        sqlx::query(
            "UPDATE approvals
             SET execution_id = ?2,
                 action_digest = ?3,
                 state = ?4,
                 expires_at = ?5,
                 raw_json = ?6
             WHERE approval_id = ?1",
        )
        .bind(approval.approval_id.to_string())
        .bind(approval.execution_id.map(|id| id.to_string()))
        .bind(&approval.action_digest)
        .bind(enum_text(&approval.state)?)
        .bind(approval.expires_at)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn resolve(&self, approval_id: ApprovalId, state: ApprovalState) -> Result<()> {
        let Some(mut approval) = self.get(approval_id).await? else {
            return Ok(());
        };
        approval.state = state;
        self.update(&approval).await
    }

    async fn list_pending_cursor(
        &self,
        limit: u32,
        after_cursor: Option<&str>,
    ) -> Result<(Vec<ApprovalRequest>, Option<String>)> {
        // Empty string cursor means "first page request" - treat as None.
        let cursor = after_cursor.filter(|c| !c.is_empty());
        let (query, bind_args): (String, Vec<String>) = match cursor {
            None => (
                "SELECT raw_json, created_at, approval_id FROM approvals
                 WHERE state = ?1
                 ORDER BY created_at DESC, approval_id DESC
                 LIMIT ?2"
                    .to_string(),
                vec![enum_text(&ApprovalState::Pending)?, (limit + 1).to_string()],
            ),
            Some(c) => {
                let cursor_vals = decode_cursor(c)?;
                (
                    "SELECT raw_json, created_at, approval_id FROM approvals
                     WHERE state = ?1
                       AND (created_at, approval_id) < (?2, ?3)
                     ORDER BY created_at DESC, approval_id DESC
                     LIMIT ?4"
                        .to_string(),
                    vec![
                        enum_text(&ApprovalState::Pending)?,
                        cursor_vals.created_at.to_rfc3339(),
                        cursor_vals.approval_id.to_string(),
                        (limit + 1).to_string(),
                    ],
                )
            }
        };

        let rows = if bind_args.len() == 2 {
            sqlx::query(&query)
                .bind(&bind_args[0])
                .bind(&bind_args[1])
                .fetch_all(&self.pool)
                .await?
        } else if bind_args.len() == 4 {
            sqlx::query(&query)
                .bind(&bind_args[0])
                .bind(&bind_args[1])
                .bind(&bind_args[2])
                .bind(&bind_args[3])
                .fetch_all(&self.pool)
                .await?
        } else {
            return Err(StoreError::Internal(format!(
                "unexpected bind args count: {}",
                bind_args.len()
            )));
        };

        let has_more = rows.len() > limit as usize;
        let items: Vec<ApprovalRequest> = rows
            .iter()
            .take(limit as usize)
            .map(|row| from_json(&row.try_get::<String, _>("raw_json")?))
            .collect::<Result<Vec<_>>>()?;

        let next_cursor = if has_more {
            items
                .last()
                .map(|item| encode_cursor(item.created_at, item.approval_id))
        } else {
            None
        };

        Ok((items, next_cursor))
    }

    async fn list_pending_by_proposal_cursor(
        &self,
        proposal_id: ProposalId,
        limit: u32,
        after_cursor: Option<&str>,
    ) -> Result<(Vec<ApprovalRequest>, Option<String>)> {
        // Empty string cursor means "first page request" - treat as None.
        let cursor = after_cursor.filter(|c| !c.is_empty());
        let (query, bind_args): (String, Vec<String>) = match cursor {
            None => (
                "SELECT raw_json, created_at, approval_id FROM approvals
                 WHERE state = ?1 AND proposal_id = ?2
                 ORDER BY created_at DESC, approval_id DESC
                 LIMIT ?3"
                    .to_string(),
                vec![
                    enum_text(&ApprovalState::Pending)?,
                    proposal_id.to_string(),
                    (limit + 1).to_string(),
                ],
            ),
            Some(cursor) => {
                let cursor_vals = decode_cursor(cursor)?;
                (
                    "SELECT raw_json, created_at, approval_id FROM approvals
                     WHERE state = ?1 AND proposal_id = ?2
                       AND (created_at, approval_id) < (?3, ?4)
                     ORDER BY created_at DESC, approval_id DESC
                     LIMIT ?5"
                        .to_string(),
                    vec![
                        enum_text(&ApprovalState::Pending)?,
                        proposal_id.to_string(),
                        cursor_vals.created_at.to_rfc3339(),
                        cursor_vals.approval_id.to_string(),
                        (limit + 1).to_string(),
                    ],
                )
            }
        };

        let rows = match bind_args.len() {
            3 => {
                sqlx::query(&query)
                    .bind(&bind_args[0])
                    .bind(&bind_args[1])
                    .bind(&bind_args[2])
                    .fetch_all(&self.pool)
                    .await?
            }
            5 => {
                sqlx::query(&query)
                    .bind(&bind_args[0])
                    .bind(&bind_args[1])
                    .bind(&bind_args[2])
                    .bind(&bind_args[3])
                    .bind(&bind_args[4])
                    .fetch_all(&self.pool)
                    .await?
            }
            _ => {
                return Err(StoreError::Internal(format!(
                    "unexpected bind args count: {}",
                    bind_args.len()
                )));
            }
        };

        let has_more = rows.len() > limit as usize;
        let items: Vec<ApprovalRequest> = rows
            .iter()
            .take(limit as usize)
            .map(|row| from_json(&row.try_get::<String, _>("raw_json")?))
            .collect::<Result<Vec<_>>>()?;

        let next_cursor = if has_more {
            items
                .last()
                .map(|item| encode_cursor(item.created_at, item.approval_id))
        } else {
            None
        };

        Ok((items, next_cursor))
    }

    async fn list_pending_by_execution_id_cursor(
        &self,
        execution_id: ExecutionId,
        limit: u32,
        after_cursor: Option<&str>,
    ) -> Result<(Vec<ApprovalRequest>, Option<String>)> {
        let cursor = after_cursor.filter(|c| !c.is_empty());
        let (query, bind_args): (String, Vec<String>) = match cursor {
            None => (
                "SELECT raw_json, created_at, approval_id FROM approvals
                 WHERE state = ?1 AND execution_id = ?2
                 ORDER BY created_at DESC, approval_id DESC
                 LIMIT ?3"
                    .to_string(),
                vec![
                    enum_text(&ApprovalState::Pending)?,
                    execution_id.to_string(),
                    (limit + 1).to_string(),
                ],
            ),
            Some(cursor) => {
                let cursor_vals = decode_cursor(cursor)?;
                (
                    "SELECT raw_json, created_at, approval_id FROM approvals
                     WHERE state = ?1 AND execution_id = ?2
                       AND (created_at, approval_id) < (?3, ?4)
                     ORDER BY created_at DESC, approval_id DESC
                     LIMIT ?5"
                        .to_string(),
                    vec![
                        enum_text(&ApprovalState::Pending)?,
                        execution_id.to_string(),
                        cursor_vals.created_at.to_rfc3339(),
                        cursor_vals.approval_id.to_string(),
                        (limit + 1).to_string(),
                    ],
                )
            }
        };

        let rows = match bind_args.len() {
            3 => {
                sqlx::query(&query)
                    .bind(&bind_args[0])
                    .bind(&bind_args[1])
                    .bind(&bind_args[2])
                    .fetch_all(&self.pool)
                    .await?
            }
            5 => {
                sqlx::query(&query)
                    .bind(&bind_args[0])
                    .bind(&bind_args[1])
                    .bind(&bind_args[2])
                    .bind(&bind_args[3])
                    .bind(&bind_args[4])
                    .fetch_all(&self.pool)
                    .await?
            }
            _ => {
                return Err(StoreError::Internal(format!(
                    "unexpected bind args count: {}",
                    bind_args.len()
                )));
            }
        };

        let has_more = rows.len() > limit as usize;
        let items: Vec<ApprovalRequest> = rows
            .iter()
            .take(limit as usize)
            .map(|row| from_json(&row.try_get::<String, _>("raw_json")?))
            .collect::<Result<Vec<_>>>()?;

        let next_cursor = if has_more {
            items
                .last()
                .map(|item| encode_cursor(item.created_at, item.approval_id))
        } else {
            None
        };

        Ok((items, next_cursor))
    }

    async fn list_pending_by_proposal_and_execution_id_cursor(
        &self,
        proposal_id: ProposalId,
        execution_id: ExecutionId,
        limit: u32,
        after_cursor: Option<&str>,
    ) -> Result<(Vec<ApprovalRequest>, Option<String>)> {
        let cursor = after_cursor.filter(|c| !c.is_empty());
        let (query, bind_args): (String, Vec<String>) = match cursor {
            None => (
                "SELECT raw_json, created_at, approval_id FROM approvals
                 WHERE state = ?1 AND proposal_id = ?2 AND execution_id = ?3
                 ORDER BY created_at DESC, approval_id DESC
                 LIMIT ?4"
                    .to_string(),
                vec![
                    enum_text(&ApprovalState::Pending)?,
                    proposal_id.to_string(),
                    execution_id.to_string(),
                    (limit + 1).to_string(),
                ],
            ),
            Some(cursor) => {
                let cursor_vals = decode_cursor(cursor)?;
                (
                    "SELECT raw_json, created_at, approval_id FROM approvals
                     WHERE state = ?1 AND proposal_id = ?2 AND execution_id = ?3
                       AND (created_at, approval_id) < (?4, ?5)
                     ORDER BY created_at DESC, approval_id DESC
                     LIMIT ?6"
                        .to_string(),
                    vec![
                        enum_text(&ApprovalState::Pending)?,
                        proposal_id.to_string(),
                        execution_id.to_string(),
                        cursor_vals.created_at.to_rfc3339(),
                        cursor_vals.approval_id.to_string(),
                        (limit + 1).to_string(),
                    ],
                )
            }
        };

        let rows = match bind_args.len() {
            4 => {
                sqlx::query(&query)
                    .bind(&bind_args[0])
                    .bind(&bind_args[1])
                    .bind(&bind_args[2])
                    .bind(&bind_args[3])
                    .fetch_all(&self.pool)
                    .await?
            }
            6 => {
                sqlx::query(&query)
                    .bind(&bind_args[0])
                    .bind(&bind_args[1])
                    .bind(&bind_args[2])
                    .bind(&bind_args[3])
                    .bind(&bind_args[4])
                    .bind(&bind_args[5])
                    .fetch_all(&self.pool)
                    .await?
            }
            _ => {
                return Err(StoreError::Internal(format!(
                    "unexpected bind args count: {}",
                    bind_args.len()
                )));
            }
        };

        let has_more = rows.len() > limit as usize;
        let items: Vec<ApprovalRequest> = rows
            .iter()
            .take(limit as usize)
            .map(|row| from_json(&row.try_get::<String, _>("raw_json")?))
            .collect::<Result<Vec<_>>>()?;

        let next_cursor = if has_more {
            items
                .last()
                .map(|item| encode_cursor(item.created_at, item.approval_id))
        } else {
            None
        };

        Ok((items, next_cursor))
    }
}

// ------------------------------------------------------------------------------------------------
// Cursor encoding/decoding
// ------------------------------------------------------------------------------------------------

/// Cursor payload for keyset pagination on approvals.
/// Encodes (created_at, approval_id) which form a unique, stable tiebreaker.
#[derive(Debug, Serialize, Deserialize)]
struct ApprovalCursor {
    created_at: DateTime<Utc>,
    approval_id: ApprovalId,
}

/// Encodes a cursor to an opaque ASCII string suitable for transport.
/// Uses URL-safe base64 encoding of JSON {created_at, approval_id}.
fn encode_cursor(created_at: DateTime<Utc>, approval_id: ApprovalId) -> String {
    let payload = ApprovalCursor {
        created_at,
        approval_id,
    };
    let json = serde_json::to_string(&payload).expect("cursor serialization must not fail");
    // Use URL-safe base64 encoding
    base64_encode(json.as_bytes())
}

/// Decodes a cursor string back to its components.
/// Fails closed on invalid/malformed cursors.
fn decode_cursor(cursor: &str) -> Result<ApprovalCursor> {
    let bytes = base64_decode(cursor)
        .map_err(|e| StoreError::Internal(format!("cursor decode failed: {}", e)))?;
    let json = String::from_utf8(bytes)
        .map_err(|e| StoreError::Internal(format!("cursor invalid (not UTF-8): {}", e)))?;
    let decoded: ApprovalCursor = serde_json::from_str(&json)
        .map_err(|e| StoreError::Internal(format!("cursor parse failed: {}", e)))?;
    Ok(decoded)
}

// URL-safe base64 encoding/decoding without external dependencies.
// Character sets: A-Z, a-z, 0-9, '-', '_'

fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut result = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        result.push(CHARS[b0 >> 2] as char);
        result.push(CHARS[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

        if chunk.len() > 1 {
            result.push(CHARS[((b1 & 0x0F) << 2) | (b2 >> 6)] as char);
        }
        if chunk.len() > 2 {
            result.push(CHARS[b2 & 0x3F] as char);
        }
    }
    result
}

fn base64_decode(input: &str) -> Result<Vec<u8>> {
    // URL-safe base64 decoder
    fn decode_char(c: char) -> Result<u8> {
        match c {
            'A'..='Z' => Ok(c as u8 - b'A'),
            'a'..='z' => Ok(c as u8 - b'a' + 26),
            '0'..='9' => Ok(c as u8 - b'0' + 52),
            '-' => Ok(62),
            '_' => Ok(63),
            _ => Err(StoreError::Internal("invalid base64 character".to_string())),
        }
    }

    let chars: Vec<char> = input.chars().collect();
    let mut result = Vec::with_capacity(chars.len() * 3 / 4);
    let mut i = 0;
    while i < chars.len() {
        let a = decode_char(chars[i])?;
        let b = if i + 1 < chars.len() {
            decode_char(chars[i + 1])?
        } else {
            return Err(StoreError::Internal("incomplete cursor".to_string()));
        };
        let c = if i + 2 < chars.len() && chars[i + 2] != '=' {
            decode_char(chars[i + 2])?
        } else {
            0
        };
        let d = if i + 3 < chars.len() && chars[i + 3] != '=' {
            decode_char(chars[i + 3])?
        } else {
            0
        };

        result.push((a << 2) | (b >> 4));
        if i + 2 < chars.len() && chars[i + 2] != '=' {
            result.push((b << 4) | (c >> 2));
        }
        if i + 3 < chars.len() && chars[i + 3] != '=' {
            result.push((c << 6) | d);
        }
        i += 4;
    }
    Ok(result)
}
