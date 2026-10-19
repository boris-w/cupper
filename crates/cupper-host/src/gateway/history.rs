//! `history/page` (design 5.3.3): the turns before the session's window,
//! newest first, from the turn snapshots. Without a cursor the page starts
//! before the oldest turn in the window, as the stream's view has it. A
//! page is at most `limits.max_history_page_bytes`, whatever the client
//! asks. A session stream without events (head 0) is `not_found` (design
//! 5.3.11, rulings before T033).

use cupper_protocol::error::{ErrorCode, RpcError};
use cupper_protocol::limits::V1;
use cupper_protocol::methods::stream::{HistoryCursor, HistoryPageParams, HistoryPageResult, Raw};
use cupper_protocol::sync::StreamId;
use serde_json::Value;

use super::dispatch::{internal, typed};
use super::routes::Source;
use crate::ledger::Outcome;
use crate::store::history;
use crate::store::streams::StreamView;

pub(super) async fn page(source: Source, params: Value) -> Outcome {
    let params: HistoryPageParams = typed(&params)?;
    let stream = StreamId::Session(params.session_id.clone());
    let handle = source.streams.open(&stream).await.map_err(internal)?;
    let (head, oldest) = handle.read(|view, head| (head, oldest_turn(view)));
    if head == 0 {
        let err = RpcError::app(ErrorCode::NotFound, format!("{stream} has no events"));
        return Err(err);
    }
    let cursor = params.cursor.unwrap_or(HistoryCursor {
        turn_no: oldest,
        op_index: None,
    });
    let most = V1.max_history_page_bytes;
    let max_bytes = params.max_bytes.map_or(most, |max| max.min(most)) as usize;
    let session = params.session_id;
    let read = source
        .readers
        .read(move |conn| Ok(history::page(conn, &session, cursor, max_bytes)));
    let page = read.await.map_err(internal)?.map_err(internal)?;
    let ops = page.ops.iter().map(Raw::encode);
    let result = HistoryPageResult {
        ops: ops.collect::<Result<_, _>>().map_err(internal)?,
        next: page.next,
    };
    serde_json::to_value(result).map_err(internal)
}

/// The `turn_no` of the oldest turn in the window. Without one, there is no
/// finished turn: the window always keeps the newest (T014).
fn oldest_turn(view: &StreamView) -> u32 {
    let oldest = match view {
        StreamView::Session(view) => view.turns.iter().map(|t| t.started.turn_no).min(),
        StreamView::Index(_) => None,
    };
    oldest.unwrap_or(u32::MAX)
}
