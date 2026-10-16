//! JSON-RPC 2.0 envelope and the error model.

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::open_enum;

pub const JSONRPC_VERSION: &str = "2.0";

/// Standard JSON-RPC error codes. Application errors always use [`APP_ERROR`]
/// and are distinguished by `error.data.code`.
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const APP_ERROR: i64 = -32000;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

open_enum! {
    /// Application error code carried in `error.data.code`.
    pub enum ErrorCode {
        VersionUnsupported => "version_unsupported",
        NotInitialized => "not_initialized",
        Unauthenticated => "unauthenticated",
        Forbidden => "forbidden",
        NotFound => "not_found",
        Conflict => "conflict",
        Expired => "expired",
        Unsupported => "unsupported",
        Unavailable => "unavailable",
        AgentError => "agent_error",
        IdempotencyMismatch => "idempotency_mismatch",
        TooLarge => "too_large",
        RateLimited => "rate_limited",
        Timeout => "timeout",
        ShuttingDown => "shutting_down",
        Internal => "internal",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ErrorData {
    pub code: ErrorCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
    /// Code-specific details, e.g. `resolved_by` for `conflict`.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, thiserror::Error)]
#[error("rpc error {code}: {message}")]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<ErrorData>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), data: None }
    }

    /// An application error (`-32000`) with the given `data.code`.
    pub fn app(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code: APP_ERROR,
            message: message.into(),
            data: Some(ErrorData { code, reason: None, retryable: None, extra: Map::new() }),
        }
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        if let Some(d) = self.data.as_mut() {
            d.reason = Some(reason.into());
        }
        self
    }

    pub fn with_extra(mut self, key: &str, value: Value) -> Self {
        if let Some(d) = self.data.as_mut() {
            d.extra.insert(key.to_owned(), value);
        }
        self
    }

    pub fn app_code(&self) -> Option<&ErrorCode> {
        self.data.as_ref().map(|d| &d.code)
    }

    pub fn reason(&self) -> Option<&str> {
        self.data.as_ref().and_then(|d| d.reason.as_deref())
    }
}

/// A loosely parsed inbound message; [`RawMessage::classify`] sorts it out.
#[derive(Debug, Deserialize)]
pub struct RawMessage {
    #[serde(default)]
    pub jsonrpc: Option<String>,
    #[serde(default)]
    pub id: Option<RequestId>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub params: Option<Value>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<RpcError>,
}

#[derive(Debug)]
pub enum Message {
    Request { id: RequestId, method: String, params: Value },
    Notification { method: String, params: Value },
    Response { id: RequestId, result: Result<Value, RpcError> },
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
                (None, None) => Ok(Message::Response { id, result: Ok(Value::Null) }),
            },
            (None, None) => Err(RpcError::new(INVALID_REQUEST, "message has neither id nor method")),
        }
    }
}

/// A request method: name plus typed params and result.
pub trait Method {
    const NAME: &'static str;
    type Params: Serialize + DeserializeOwned + JsonSchema;
    type Result: Serialize + DeserializeOwned + JsonSchema;
}

/// A notification method: name plus typed params.
pub trait Notification {
    const NAME: &'static str;
    type Params: Serialize + DeserializeOwned + JsonSchema;
}

#[derive(Debug, Serialize)]
pub struct OutRequest<'a, P> {
    pub jsonrpc: &'static str,
    pub id: RequestId,
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
    pub id: &'a RequestId,
    pub result: R,
}

#[derive(Debug, Serialize)]
pub struct OutError<'a> {
    pub jsonrpc: &'static str,
    pub id: Option<&'a RequestId>,
    pub error: &'a RpcError,
}

pub fn request<M: Method>(id: RequestId, params: M::Params) -> OutRequest<'static, M::Params> {
    OutRequest { jsonrpc: JSONRPC_VERSION, id, method: M::NAME, params }
}

pub fn notification<N: Notification>(params: N::Params) -> OutNotification<'static, N::Params> {
    OutNotification { jsonrpc: JSONRPC_VERSION, method: N::NAME, params }
}
