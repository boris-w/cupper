//! Command idempotency: one result per (principal, command_id).
//!
//! A repeated command with the same parameters gets the stored result back with
//! `replayed: true`; different parameters get `idempotency_mismatch`. Concurrent
//! duplicates wait for the first execution. Only successful results are stored:
//! a failed command may be retried and runs again.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};

use cupper_protocol::rpc::{ErrorCode, RpcError};
use serde_json::Value;
use tokio::sync::watch;

use crate::store::Store;
use crate::util::{canonical_hash, now_ms};

/// Keep results at least this long (ADR-04: 24 h).
pub const RETENTION_MS: i64 = 24 * 60 * 60 * 1000;

type Key = (String, String);
type Slot = watch::Receiver<Option<(String, Result<Value, RpcError>)>>;

#[derive(Debug)]
pub struct Commands {
    store: Arc<Store>,
    inflight: Mutex<HashMap<Key, Slot>>,
}

fn replayed(mut v: Value) -> Value {
    if let Value::Object(m) = &mut v {
        m.insert("replayed".into(), Value::Bool(true));
    }
    v
}

fn mismatch() -> RpcError {
    RpcError::app(ErrorCode::IdempotencyMismatch, "command_id reused with different parameters")
}

impl Commands {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store, inflight: Mutex::default() }
    }

    /// Hash of the parameters without the envelope fields.
    pub fn params_hash(method: &str, params: &Value) -> String {
        let mut p = params.clone();
        if let Value::Object(m) = &mut p {
            m.remove("command_id");
            m.remove("attestation");
        }
        canonical_hash(&serde_json::json!({ "method": method, "params": p }))
    }

    pub async fn run<F, Fut>(&self, principal: &str, command_id: &str, method: &str, params: &Value, exec: F) -> Result<Value, RpcError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Value, RpcError>>,
    {
        let hash = Self::params_hash(method, params);
        let internal = |e: crate::store::StoreError| RpcError::app(ErrorCode::Internal, e.to_string());
        if let Some((stored_hash, result)) = self.store.command(principal, command_id).map_err(internal)? {
            if stored_hash != hash {
                return Err(mismatch());
            }
            let v: Value = serde_json::from_str(&result).map_err(|e| RpcError::app(ErrorCode::Internal, e.to_string()))?;
            return Ok(replayed(v));
        }
        let key = (principal.to_owned(), command_id.to_owned());
        let tx = {
            let mut map = self.inflight.lock().expect("inflight mutex poisoned");
            match map.get(&key) {
                Some(rx) => Err(rx.clone()),
                None => {
                    let (tx, rx) = watch::channel(None);
                    map.insert(key.clone(), rx);
                    Ok(tx)
                }
            }
        };
        let tx = match tx {
            Ok(tx) => tx,
            Err(mut rx) => {
                // A duplicate arrived while the first is still executing: wait for it.
                let done = rx.wait_for(Option::is_some).await.map_err(|_| RpcError::app(ErrorCode::Internal, "command dropped"))?;
                let (first_hash, result) = done.clone().expect("checked by wait_for");
                if first_hash != hash {
                    return Err(mismatch());
                }
                return result.map(replayed);
            }
        };
        let result = exec().await;
        if let Ok(v) = &result {
            self.store.save_command(principal, command_id, method, &hash, &v.to_string(), now_ms()).map_err(internal)?;
        }
        let _ = tx.send(Some((hash, result.clone())));
        self.inflight.lock().expect("inflight mutex poisoned").remove(&key);
        result
    }

    pub fn prune(&self) {
        if let Err(e) = self.store.prune_commands(now_ms() - RETENTION_MS) {
            tracing::warn!("pruning command results failed: {e}");
        }
    }
}
