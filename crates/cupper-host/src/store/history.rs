//! `history/page` reads (design 5.3.3): the turns before the window, newest
//! first, one turn snapshot per page. A turn larger than a page comes in
//! pages of its ops, front to back, each starting at `op_index`.

use cupper_protocol::event::EventBody;
use cupper_protocol::ids::SessionId;
use cupper_protocol::view::paginate;
use rusqlite::Connection;

use super::log;
use super::streams::StreamError;

/// Where a page starts; `history/page`'s cursor. Without `op_index`: the
/// turn before `turn_no`. With it: turn `turn_no` from that op on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub turn_no: u32,
    pub op_index: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Page {
    pub ops: Vec<EventBody>,
    /// `None` once the first turn is served.
    pub next: Option<Cursor>,
}

/// The page of `session` at `cursor`, its ops' JSON array within `max_bytes`
/// unless a single op is larger.
pub fn page(
    conn: &Connection,
    session: &SessionId,
    cursor: Cursor,
    max_bytes: usize,
) -> Result<Page, StreamError> {
    let (turn_no, from) = match cursor.op_index {
        Some(op_index) => (cursor.turn_no, op_index as usize),
        None => match log::turn_before(conn, session, cursor.turn_no)? {
            Some(turn_no) => (turn_no, 0),
            None => return Ok(Page::default()),
        },
    };
    let Some(ops) = log::turn_ops(conn, session, turn_no)? else {
        return Ok(Page::default());
    };
    let from = from.min(ops.len());
    let end = from
        + paginate(&ops[from..], max_bytes)
            .first()
            .map_or(0, |page| page.end);
    let next = if end < ops.len() {
        let op_index = u32::try_from(end).expect("a turn has fewer than 2^32 ops");
        Some(Cursor {
            turn_no,
            op_index: Some(op_index),
        })
    } else {
        let earlier = log::turn_before(conn, session, turn_no)?;
        earlier.map(|_| Cursor {
            turn_no,
            op_index: None,
        })
    };
    let ops = ops[from..end].to_vec();
    Ok(Page { ops, next })
}

#[cfg(test)]
mod tests {
    use cupper_protocol::view::{OpLimits, SessionView, View};

    use super::*;
    use crate::store::log::TurnSnapshot;
    use crate::store::testutil::{end, host_db, item, turn};

    #[test]
    fn store_history_pages_turns_newest_first_and_big_turns_by_op_index() {
        let conn = host_db();
        let session = "s_1".parse().unwrap();
        let mut view = SessionView::default();
        for n in 1..=3 {
            let items = (0..if n == 2 { 5 } else { 1 }).map(|i| item(&format!("i_{n}x{i}"), n));
            for body in [turn(n)].into_iter().chain(items).chain([end(n)]) {
                view.apply(&body).unwrap();
            }
            let id = format!("t_{n}").parse().unwrap();
            TurnSnapshot::of(&session, &view, &id)
                .unwrap()
                .unwrap()
                .insert(&conn)
                .unwrap();
        }
        let at = |turn_no, op_index| Cursor { turn_no, op_index };
        let page = |cursor| super::page(&conn, &session, cursor, 400).unwrap();
        let newest = page(at(4, None));
        assert_eq!((newest.ops.len(), newest.next), (3, Some(at(3, None))));

        // Turn 2 has 7 ops, about 150 bytes each: it comes in pages.
        let (mut ops, mut cursor) = (Vec::new(), at(3, None));
        while cursor.turn_no == 3 || cursor.op_index.is_some() {
            let next = page(cursor);
            assert!(next.ops.len() < 7, "{next:?}");
            ops.extend(next.ops);
            cursor = next.next.unwrap();
        }
        assert_eq!((ops.len(), cursor), (7, at(2, None)));
        assert_eq!(
            Some(ops),
            view.turn_ops(&"t_2".parse().unwrap(), &OpLimits::V1)
        );
        assert_eq!(page(at(2, None)).next, None, "turn 1 is the first");
        assert_eq!(page(at(1, None)), Page::default());
    }
}
