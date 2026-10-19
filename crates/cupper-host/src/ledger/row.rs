//! The `commands` table (design 5.4.2): the ledger on disk. The writer thread
//! runs the writes; [`load`] runs at startup.

use cupper_protocol::error::RpcError;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Begun, Entry, Key, LedgerError, Outcome, State};

/// The `result` column: `{"result": ...}` or `{"error": {...}}`, as in a
/// JSON-RPC response.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Stored {
    Result(Value),
    Error(RpcError),
}

pub(super) fn outcome_json(outcome: &Outcome) -> String {
    let stored = match outcome.clone() {
        Ok(result) => Stored::Result(result),
        Err(error) => Stored::Error(error),
    };
    serde_json::to_string(&stored).expect("JSON values and errors always serialize")
}

/// Writes the command's row: `done` with `result`, or `pending_external`
/// without one. A row left from an expired run of the same key goes.
pub(super) fn upsert(
    conn: &Connection,
    key: &Key,
    begun: &Begun,
    result: Option<&str>,
) -> rusqlite::Result<()> {
    let state = if result.is_some() {
        "done"
    } else {
        "pending_external"
    };
    conn.execute(
        "INSERT OR REPLACE INTO commands
           (principal, command_id, method, params_hash, state, result, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            key.principal.as_str(),
            key.command.as_str(),
            &*begun.method,
            begun.hash.as_str(),
            state,
            result,
            begun.created_at,
        ],
    )?;
    Ok(())
}

pub(super) fn delete(conn: &Connection, key: &Key) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM commands WHERE principal = ?1 AND command_id = ?2",
        params![key.principal.as_str(), key.command.as_str()],
    )?;
    Ok(())
}

/// The rows created after `expired_up_to`: `done` ones replay, and
/// `pending_external` ones, which never got their outcome, are unknown.
pub(super) fn load(
    conn: &Connection,
    expired_up_to: u64,
) -> Result<Vec<(Key, Entry)>, LedgerError> {
    let mut select = conn.prepare(
        "SELECT principal, command_id, method, params_hash, state, result, created_at
           FROM commands WHERE created_at > ?1",
    )?;
    let mut rows = select.query([expired_up_to])?;
    let mut entries = Vec::new();
    while let Some(row) = rows.next()? {
        let text = |i| row.get::<_, String>(i);
        let command = text(1)?;
        let corrupt = |column: &str| LedgerError::Corrupt(format!("{column} of {command:?}"));
        let key = Key {
            principal: text(0)?.parse().map_err(|_| corrupt("principal"))?,
            command: command.parse().map_err(|_| corrupt("command_id"))?,
        };
        let begun = Begun {
            method: text(2)?.into(),
            hash: text(3)?.parse().map_err(|_| corrupt("params_hash"))?,
            created_at: row.get(6)?,
        };
        let state = match (text(4)?.as_str(), row.get::<_, Option<String>>(5)?) {
            ("done", Some(json)) => match serde_json::from_str(&json) {
                Ok(Stored::Result(result)) => State::Done(Ok(result)),
                Ok(Stored::Error(error)) => State::Done(Err(error)),
                Err(_) => return Err(corrupt("result")),
            },
            ("pending_external", None) => State::Unknown,
            _ => return Err(corrupt("state")),
        };
        entries.push((key, Entry { begun, state }));
    }
    Ok(entries)
}
