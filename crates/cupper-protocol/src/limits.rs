//! Protocol limits (design 5.3.9). The host sends [`V1`] as `limits` in the
//! `initialize` result, and clients go by the values they receive.
//!
//! Text budgets count bytes of the JSON-escaped UTF-8 text, not characters:
//! the frame limit is in bytes, and 32,000 merged CJK characters would not fit
//! into one 64,000-byte frame.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Largest integer JavaScript represents exactly (2^53 - 1). Every integer in
/// the protocol stays within plus or minus this bound (ADR-04).
pub const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

const MIB: u32 = 1024 * 1024;
const HOUR_MS: u64 = 60 * 60 * 1000;

/// The limits of protocol v1.
pub const V1: Limits = Limits {
    max_frame_bytes: 64_000,
    max_delta_bytes: 16_000,
    max_merged_bytes: 32_000,
    max_inline_bytes: 16_000,
    max_label_chars: 1_000,
    max_coalesce_ms: 1_000,
    max_queue_bytes: 4 * MIB,
    max_inflight_requests: 64,
    max_subscriptions: 64,
    snapshot_turns: 20,
    snapshot_items: 2_000,
    max_history_page_bytes: 48_000,
    max_blob_bytes: 32 * MIB,
    max_pending_upload_bytes: 64 * MIB,
    max_image_bytes: 10 * MIB,
    max_queued_messages: 50,
    command_retention_ms: 48 * HOUR_MS,
    max_elevation_ms: HOUR_MS / 4,
    interrupt_fallback_ms: 10_000,
    steer_timeout_ms: 5_000,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Limits {
    /// A whole frame, length prefix included.
    pub max_frame_bytes: u32,
    /// Escaped text of one `item.delta` in the log.
    pub max_delta_bytes: u32,
    /// Escaped text of a range event made by send-time coalescing.
    pub max_merged_bytes: u32,
    /// Inline content above this escaped size becomes a blob reference.
    pub max_inline_bytes: u32,
    /// Agent-provided titles and names are cut to this many characters
    /// (never the content a permission request asks the user to approve).
    pub max_label_chars: u32,
    /// Upper bound of the send-time coalescing window a client may ask for.
    pub max_coalesce_ms: u32,
    /// Live event backlog per connection; beyond it the host resets the stream.
    pub max_queue_bytes: u32,
    /// Requests in flight per connection.
    pub max_inflight_requests: u32,
    /// Subscriptions per connection.
    pub max_subscriptions: u32,
    /// A session snapshot covers this many finished turns, or
    /// [`snapshot_items`](Self::snapshot_items) items, plus the running turn.
    pub snapshot_turns: u32,
    pub snapshot_items: u32,
    /// One `history/page` result.
    pub max_history_page_bytes: u32,
    /// One uploaded blob.
    pub max_blob_bytes: u32,
    /// Unfinished uploads of one connection together.
    pub max_pending_upload_bytes: u32,
    /// Images, unless the agent sets a lower limit.
    pub max_image_bytes: u32,
    /// Queued messages per session.
    pub max_queued_messages: u32,
    /// How long the host remembers the outcome of a command.
    pub command_retention_ms: u64,
    /// Longest elevation a CLI can be granted.
    pub max_elevation_ms: u64,
    /// After an interrupt, how long the host waits before it ends the turn itself.
    pub interrupt_fallback_ms: u64,
    /// How long a steer may take before the message falls back to the queue.
    pub steer_timeout_ms: u64,
}
