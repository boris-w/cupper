//! JSON-RPC 2.0 envelope (design 5.3.1) and the method traits. One object per
//! frame, no batches. The host never sends requests, only responses and
//! notifications.

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{ErrorCode, ErrorReason, INVALID_REQUEST, RpcError};
use crate::open_enum;

pub const JSONRPC_VERSION: &str = "2.0";

/// The `id` of a request, chosen by the client and echoed in the response.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum RpcId {
    Number(i64),
    String(String),
}

open_enum! {
    /// A permission a principal holds (design 5.8.2).
    pub enum Scope {
        Read => "read",
        Write => "write",
        Approve => "approve",
        Privileged => "privileged",
        UiLocal => "ui_local",
        Test => "test",
    }
}

/// What calling a method again does (design 5.3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Idempotency {
    /// Takes a `command_id`; the host's ledger answers a repeat with the first outcome.
    Command,
    /// A repeat has no further effect, so no `command_id` is needed.
    Natural,
    /// Neither: a repeat may act again (`initialize`, `auth/elevate`, `test/*`).
    None,
}

impl Idempotency {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Natural => "natural",
            Self::None => "none",
        }
    }
}

/// The first milestone that needs a method (design 5.3.4, column "里程碑").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Milestone {
    M1a,
    M1b,
    M2,
    M3,
}

impl Milestone {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::M1a => "M1a",
            Self::M1b => "M1b",
            Self::M2 => "M2",
            Self::M3 => "M3",
        }
    }
}

/// An error a method may return besides those every method may return (see
/// [`crate::methods`]): a code, narrowed to one reason or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodError {
    pub code: ErrorCode,
    pub reason: Option<ErrorReason>,
}

impl MethodError {
    /// `code` or `code/reason`, as `x-errors` lists it.
    pub fn wire(&self) -> String {
        match &self.reason {
            Some(reason) => format!("{}/{reason}", self.code),
            None => self.code.to_string(),
        }
    }
}

/// A request method. Every method declares its scope, idempotency, milestone
/// and errors, so a method that forgets one does not compile (design 5.1.7).
/// The policy layer reads them, and the schema carries them as `x-scope`,
/// `x-idempotent`, `x-milestone` and `x-errors`.
pub trait Method {
    const NAME: &'static str;
    /// The scope a caller must hold; `None` when any principal may call it.
    const SCOPE: Option<Scope>;
    const IDEMPOTENCY: Idempotency;
    const MILESTONE: Milestone;
    const ERRORS: &'static [MethodError];
    type Params: Serialize + DeserializeOwned + JsonSchema;
    type Result: Serialize + DeserializeOwned + JsonSchema;
}

pub trait Notification {
    const NAME: &'static str;
    const MILESTONE: Milestone;
    type Params: Serialize + DeserializeOwned + JsonSchema;
}

/// A notification the host sends to clients.
pub trait HostNotification: Notification {}

/// A notification a client sends to the host. It goes through the same policy
/// as a request, so it declares the same constants as a [`Method`].
pub trait ClientNotification: Notification {
    const SCOPE: Option<Scope>;
    const IDEMPOTENCY: Idempotency;
}

/// A loosely parsed inbound message; [`RawMessage::classify`] sorts it out.
#[derive(Debug, Deserialize)]
pub struct RawMessage {
    #[serde(default)]
    pub jsonrpc: Option<String>,
    #[serde(default)]
    pub id: Option<RpcId>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub params: Option<Value>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<RpcError>,
}

#[derive(Debug, PartialEq)]
pub enum Message {
    Request {
        id: RpcId,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    Response {
        id: RpcId,
        result: Result<Value, RpcError>,
    },
}

impl RawMessage {
    pub fn classify(self) -> Result<Message, RpcError> {
        if self.jsonrpc.as_deref() != Some(JSONRPC_VERSION) {
            return Err(RpcError::new(INVALID_REQUEST, "jsonrpc must be \"2.0\""));
        }
        let params = self.params.unwrap_or(Value::Null);
        match (self.id, self.method) {
            (Some(id), Some(method)) => Ok(Message::Request { id, method, params }),
            (None, Some(method)) => Ok(Message::Notification { method, params }),
            (Some(id), None) => match (self.result, self.error) {
                (_, Some(e)) => Ok(Message::Response { id, result: Err(e) }),
                (Some(r), None) => Ok(Message::Response { id, result: Ok(r) }),
                (None, None) => Ok(Message::Response {
                    id,
                    result: Ok(Value::Null),
                }),
            },
            (None, None) => Err(RpcError::new(
                INVALID_REQUEST,
                "message has neither id nor method",
            )),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct OutRequest<'a, P> {
    pub jsonrpc: &'static str,
    pub id: RpcId,
    pub method: &'a str,
    pub params: P,
}

#[derive(Debug, Serialize)]
pub struct OutNotification<'a, P> {
    pub jsonrpc: &'static str,
    pub method: &'a str,
    pub params: P,
}

#[derive(Debug, Serialize)]
pub struct OutResult<'a, R> {
    pub jsonrpc: &'static str,
    pub id: &'a RpcId,
    pub result: R,
}

#[derive(Debug, Serialize)]
pub struct OutError<'a> {
    pub jsonrpc: &'static str,
    pub id: Option<&'a RpcId>,
    pub error: &'a RpcError,
}

pub fn request<M: Method>(id: RpcId, params: M::Params) -> OutRequest<'static, M::Params> {
    OutRequest {
        jsonrpc: JSONRPC_VERSION,
        id,
        method: M::NAME,
        params,
    }
}

pub fn notification<N: Notification>(params: N::Params) -> OutNotification<'static, N::Params> {
    OutNotification {
        jsonrpc: JSONRPC_VERSION,
        method: N::NAME,
        params,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    struct Ping;

    impl Method for Ping {
        const NAME: &'static str = "ping";
        const SCOPE: Option<Scope> = None;
        const IDEMPOTENCY: Idempotency = Idempotency::Natural;
        const MILESTONE: Milestone = Milestone::M1a;
        const ERRORS: &'static [MethodError] = &[];
        type Params = Value;
        type Result = Value;
    }

    fn classify(raw: Value) -> Result<Message, RpcError> {
        serde_json::from_value::<RawMessage>(raw)
            .unwrap()
            .classify()
    }

    #[test]
    fn unit_proto_base_rpc_envelope_roundtrip() {
        let out = serde_json::to_value(request::<Ping>(RpcId::Number(1), json!({}))).unwrap();
        assert_eq!(
            out,
            json!({"jsonrpc": "2.0", "id": 1, "method": "ping", "params": {}})
        );
        assert_eq!(
            classify(out).unwrap(),
            Message::Request {
                id: RpcId::Number(1),
                method: "ping".into(),
                params: json!({})
            }
        );
        let note = classify(json!({"jsonrpc": "2.0", "method": "host/hello"})).unwrap();
        assert_eq!(
            note,
            Message::Notification {
                method: "host/hello".into(),
                params: Value::Null
            }
        );
        let err = classify(
            json!({"jsonrpc": "2.0", "id": "a", "error": {"code": -32601, "message": "no"}}),
        );
        let Ok(Message::Response { id, result: Err(e) }) = err else {
            panic!("{err:?}")
        };
        assert_eq!((id, e.code), (RpcId::String("a".into()), -32601));
        assert_eq!(
            classify(json!({"id": 1, "method": "ping"}))
                .unwrap_err()
                .code,
            INVALID_REQUEST
        );
        assert_eq!(
            classify(json!({"jsonrpc": "2.0"})).unwrap_err().code,
            INVALID_REQUEST
        );
    }
}
