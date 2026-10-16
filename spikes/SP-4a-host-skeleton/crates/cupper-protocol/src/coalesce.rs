//! Send-time coalescing: merge contiguous mergeable events into one range event.
//!
//! The merged event keeps the last `seq` and records the first one in `from`, so
//! a reader's invariant `first_seq == previous seq + 1` still holds.

use crate::events::{Event, EventBody, ItemDelta};
use crate::text::escaped_len;

/// Merge `next` into `prev` when both are deltas of the same item, `next`
/// directly follows `prev`, and the merged text stays within `max_escaped`.
pub fn try_merge(prev: &Event, next: &Event, max_escaped: usize) -> Option<Event> {
    if prev.seq.checked_add(1) != Some(next.first_seq()) {
        return None;
    }
    let (EventBody::ItemDelta(a), EventBody::ItemDelta(b)) = (&prev.body, &next.body) else {
        return None;
    };
    if a.item_id != b.item_id || escaped_len(&a.text) + escaped_len(&b.text) > max_escaped {
        return None;
    }
    Some(Event {
        seq: next.seq,
        from: Some(prev.first_seq()),
        v: next.v,
        ts: next.ts,
        cause: if prev.cause == next.cause { next.cause.clone() } else { None },
        fallback: None,
        body: EventBody::ItemDelta(ItemDelta { item_id: a.item_id.clone(), text: format!("{}{}", a.text, b.text) }),
    })
}

/// Greedily coalesce a contiguous run of events.
pub fn coalesce(events: Vec<Event>, max_escaped: usize) -> Vec<Event> {
    let mut out: Vec<Event> = Vec::with_capacity(events.len());
    for e in events {
        if let Some(last) = out.last_mut()
            && let Some(merged) = try_merge(last, &e, max_escaped)
        {
            *last = merged;
            continue;
        }
        out.push(e);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{ItemCompleted, ItemKind, ItemStarted, ItemStatus, TurnStarted};
    use crate::sync::StreamCursor;
    use crate::text::MAX_MERGED_ESCAPED;
    use crate::view::SessionView;
    use proptest::prelude::*;

    #[derive(Debug, Clone)]
    enum Op {
        Start,
        Delta(usize, String),
        Complete(usize),
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            1 => Just(Op::Start),
            6 => (any::<usize>(), "\\PC{0,40}|[\"\\\\\\n\\x01]{0,10}").prop_map(|(i, s)| Op::Delta(i, s)),
            1 => any::<usize>().prop_map(Op::Complete),
        ]
    }

    /// Turn random ops into a valid event log with contiguous seqs starting at 1.
    fn build(ops: &[Op]) -> Vec<Event> {
        let mut bodies = vec![EventBody::TurnStarted(TurnStarted { turn_id: "t_1".into() })];
        let mut open: Vec<String> = Vec::new();
        let mut n = 0;
        for o in ops {
            match o {
                Op::Start => {
                    n += 1;
                    let id = format!("i_{n}");
                    bodies.push(EventBody::ItemStarted(ItemStarted {
                        item_id: id.clone(),
                        turn_id: "t_1".into(),
                        kind: ItemKind::AgentMessage,
                        title: None,
                        tool_kind: None,
                    }));
                    open.push(id);
                }
                Op::Delta(i, s) if !open.is_empty() => {
                    let id = open[i % open.len()].clone();
                    bodies.push(EventBody::ItemDelta(ItemDelta { item_id: id, text: s.clone() }));
                }
                Op::Complete(i) if !open.is_empty() => {
                    let id = open.remove(i % open.len());
                    bodies.push(EventBody::ItemCompleted(ItemCompleted { item_id: id, status: ItemStatus::Completed }));
                }
                _ => {}
            }
        }
        bodies
            .into_iter()
            .enumerate()
            .map(|(i, body)| Event { seq: i as u64 + 1, from: None, v: 1, ts: i as i64, cause: None, fallback: None, body })
            .collect()
    }

    proptest! {
        /// Applying coalesced events gives exactly the same state as applying them one by one,
        /// and coalesced events still satisfy the contiguity invariant.
        #[test]
        fn coalesced_apply_equals_individual_apply(ops in prop::collection::vec(op(), 0..200), cap in 6usize..200) {
            let events = build(&ops);
            let mut one_by_one = SessionView::default();
            for e in &events {
                one_by_one.apply(&e.body).unwrap();
            }
            let merged = coalesce(events.clone(), cap);
            let mut cursor = StreamCursor::at(0);
            let mut coalesced = SessionView::default();
            for e in &merged {
                cursor.accept(e).unwrap();
                coalesced.apply(&e.body).unwrap();
            }
            prop_assert_eq!(cursor.last_seq(), events.len() as u64);
            prop_assert_eq!(one_by_one, coalesced);
        }

        /// The default cap keeps every merged delta within its escaped budget.
        #[test]
        fn merged_text_respects_cap(ops in prop::collection::vec(op(), 0..200)) {
            for e in coalesce(build(&ops), MAX_MERGED_ESCAPED) {
                if let EventBody::ItemDelta(d) = &e.body {
                    prop_assert!(escaped_len(&d.text) <= MAX_MERGED_ESCAPED || e.from.is_none());
                }
            }
        }
    }
}
