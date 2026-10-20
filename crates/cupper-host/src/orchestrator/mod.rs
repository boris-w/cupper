//! Session orchestration (design 5.2.6): the pure cores (the turn state
//! machine, the queue of messages it drains, the interaction broker, the
//! autonomous judge, and the coalescer that writes the agent's updates as
//! items, with their activity groups and the final answer), and the
//! [`session`] actors that run them.

pub mod autonomous;
pub mod coalescer;
pub mod grouping;
pub mod interaction;
#[cfg(test)]
mod props;
pub mod queue;
pub mod session;
#[cfg(test)]
mod session_props;
#[cfg(test)]
mod testkit;
pub mod turn;

use cupper_protocol::annotations::{DurationMs, TimestampMs};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::ItemId;
use cupper_protocol::item::{Item, ItemCompleted, ItemStarted, ItemStatus, Notice, NoticeCode};

use crate::clock::{IdGen, Now};

/// A notice without a turn: the view shows it in the running turn, or after
/// the last one (design 5.3.3).
pub(crate) fn notice(ids: &IdGen, now: Now, code: NoticeCode) -> [EventBody; 2] {
    let item_id = ItemId::from_uuid(ids.new_id_at(now.wall_ms).as_u128());
    let at = TimestampMs(now.wall_ms);
    let item = Item::Notice(Notice {
        code,
        params: Default::default(),
    });
    let started = ItemStarted {
        item_id: item_id.clone(),
        turn_id: None,
        started_at: at,
        group_id: None,
        item,
    };
    let completed = ItemCompleted {
        item_id,
        status: ItemStatus::Completed,
        ended_at: at,
        duration_ms: DurationMs(0),
        may_still_run: false,
        patch: Default::default(),
    };
    [
        EventBody::ItemStarted(started),
        EventBody::ItemCompleted(completed),
    ]
}
