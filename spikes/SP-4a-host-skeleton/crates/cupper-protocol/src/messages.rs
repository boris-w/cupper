//! Method and notification types (a subset of the ADR-04 catalogue).
//!
//! The host never sends requests to clients. Approvals and questions are state
//! objects in the event stream; clients answer with commands.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::events::{Event, EventBody, InteractionOutcome, PrincipalKind};
use crate::open_enum;
use crate::rpc::{Method, Notification};

/// Current protocol major version.
pub const PROTOCOL_VERSION: u32 = 1;

fn is_false(b: &bool) -> bool {
    !*b
}

// ---------- initialize ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VersionRange {
    pub min: u32,
    pub max: u32,
}

open_enum! {
    pub enum ClientKind {
        Desktop => "desktop",
        Cli => "cli",
        Ios => "ios",
        Test => "test",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClientInfo {
    pub kind: ClientKind,
    pub name: String,
    pub version: String,
}

/// How the client proves who it is, tagged by `type`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum AuthParams {
    /// Same-user peer credentials only; yields the read-only `cli` principal.
    #[serde(rename = "peer")]
    Peer,
    /// One-time key handed to the host over an inherited fd (test builds only).
    #[serde(rename = "harness")]
    Harness { token: String },
    #[serde(untagged)]
    Unknown(crate::events::UnknownTagged),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InitializeParams {
    pub protocol: VersionRange,
    pub client: ClientInfo,
    #[serde(default)]
    pub features: Vec<String>,
    pub auth: AuthParams,
    /// Requested send-time coalescing window. The host clamps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coalesce_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HostInfo {
    pub host_id: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PrincipalInfo {
    pub id: String,
    pub kind: PrincipalKind,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Limits {
    pub max_frame_bytes: u32,
    pub coalesce_ms: u32,
    /// Per-connection live event backlog above which a stream is reset.
    pub max_queue_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InitializeResult {
    pub protocol: u32,
    pub host: HostInfo,
    pub principal: PrincipalInfo,
    pub features: Vec<String>,
    pub limits: Limits,
}

pub enum Initialize {}
impl Method for Initialize {
    const NAME: &'static str = "initialize";
    type Params = InitializeParams;
    type Result = InitializeResult;
}

// ---------- streams ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SubscribeParams {
    /// `index` or `session/<session_id>`.
    pub stream: String,
    /// Last seq the client has applied. Omit to start from a snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_seq: Option<u64>,
}

open_enum! {
    pub enum SubscribeMode {
        /// Events after `after_seq` follow as `stream/events`.
        Replay => "replay",
        /// `stream/snapshot` pages (state at `at_seq`) follow, then events after `at_seq`.
        Snapshot => "snapshot",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SubscribeResult {
    pub stream: String,
    pub mode: SubscribeMode,
    /// Seq the snapshot reflects, or the replay starting point (`after_seq`).
    pub at_seq: u64,
    pub head_seq: u64,
}

pub enum Subscribe {}
impl Method for Subscribe {
    const NAME: &'static str = "stream/subscribe";
    type Params = SubscribeParams;
    type Result = SubscribeResult;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StreamEventsParams {
    pub stream: String,
    pub events: Vec<Event>,
}

pub enum StreamEvents {}
impl Notification for StreamEvents {
    const NAME: &'static str = "stream/events";
    type Params = StreamEventsParams;
}

/// One page of a snapshot. Pages are ordered; applying `ops` in order to an
/// empty view yields the stream state at `at_seq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StreamSnapshotParams {
    pub stream: String,
    pub at_seq: u64,
    pub page: u32,
    pub last: bool,
    pub ops: Vec<EventBody>,
}

pub enum StreamSnapshot {}
impl Notification for StreamSnapshot {
    const NAME: &'static str = "stream/snapshot";
    type Params = StreamSnapshotParams;
}

open_enum! {
    pub enum ResetReason {
        /// The connection's send queue exceeded `limits.max_queue_bytes`.
        SlowConsumer => "slow_consumer",
        /// The stream was rebuilt (e.g. after host recovery).
        Rebuilt => "rebuilt",
    }
}

/// The host dropped this subscription; resubscribe with your cursor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StreamResetParams {
    pub stream: String,
    pub reason: ResetReason,
}

pub enum StreamReset {}
impl Notification for StreamReset {
    const NAME: &'static str = "stream/reset";
    type Params = StreamResetParams;
}

// ---------- commands ----------

/// Fields every command carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CommandMeta {
    /// Client-generated UUIDv7. Retries reuse it; the host replays the first result.
    pub command_id: String,
    /// Reserved for device signatures (M3/M4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation: Option<Value>,
}

impl CommandMeta {
    pub fn new(command_id: impl Into<String>) -> Self {
        Self { command_id: command_id.into(), attestation: None }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionCreateParams {
    #[serde(flatten)]
    pub meta: CommandMeta,
    /// Agent name from the host configuration (e.g. `claude`, `fake`).
    pub agent: String,
    pub cwd: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionCreateResult {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub replayed: bool,
}

pub enum SessionCreate {}
impl Method for SessionCreate {
    const NAME: &'static str = "session/create";
    type Params = SessionCreateParams;
    type Result = SessionCreateResult;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionSendParams {
    #[serde(flatten)]
    pub meta: CommandMeta,
    pub session_id: String,
    pub text: String,
    /// Steer precondition: only accepted while this turn is running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_turn_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionSendResult {
    pub turn_id: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub replayed: bool,
}

pub enum SessionSend {}
impl Method for SessionSend {
    const NAME: &'static str = "session/send";
    type Params = SessionSendParams;
    type Result = SessionSendResult;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionInterruptParams {
    #[serde(flatten)]
    pub meta: CommandMeta,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_turn_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionInterruptResult {
    /// The turn that was asked to stop, if one was running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub replayed: bool,
}

pub enum SessionInterrupt {}
impl Method for SessionInterrupt {
    const NAME: &'static str = "session/interrupt";
    type Params = SessionInterruptParams;
    type Result = SessionInterruptResult;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InteractionRespondParams {
    #[serde(flatten)]
    pub meta: CommandMeta,
    pub session_id: String,
    pub request_id: String,
    /// The `digest` from `interaction.requested`, echoed verbatim.
    pub request_digest: String,
    pub outcome: InteractionOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InteractionRespondResult {
    pub request_id: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub replayed: bool,
}

pub enum InteractionRespond {}
impl Method for InteractionRespond {
    const NAME: &'static str = "interaction/respond";
    type Params = InteractionRespondParams;
    type Result = InteractionRespondResult;
}

// ---------- content ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BlobGetParams {
    pub hash: String,
    #[serde(default)]
    pub offset: u64,
    /// Bytes to read; omit for "to the end".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u64>,
}

/// Sent after the `0x02` chunk frames that carry the requested range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BlobGetResult {
    pub hash: String,
    pub size: u64,
    pub offset: u64,
    pub length: u64,
    pub chunks: u32,
}

pub enum BlobGet {}
impl Method for BlobGet {
    const NAME: &'static str = "blob/get";
    type Params = BlobGetParams;
    type Result = BlobGetResult;
}

/// Scope names used by the host's policy check.
pub mod scopes {
    pub const READ: &str = "read";
    pub const WRITE: &str = "write";
    pub const APPROVE: &str = "approve";
    pub const ALL: &str = "*";
}
