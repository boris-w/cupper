//! Send-time coalescing (design 5.3.3, ADR-04): for a slow connection, merge
//! contiguous mergeable events into one range event.
//!
//! The merged event keeps the last `seq` and records the first in `from`, so a
//! reader's check `first_seq == previous seq + 1` still holds. Which bodies
//! merge, and how, is the body type's [`Mergeable`] rule; the event variants it
//! merges carry `x-mergeable: true` in the schema.

use crate::envelope::Event;

/// How an event body merges with the body of the event right after it.
pub trait Mergeable: Sized {
    /// `self` followed by `next` as one body, or `None` when the two do not
    /// merge or the merged text would exceed `max_escaped` escaped bytes.
    fn merge(&self, next: &Self, max_escaped: usize) -> Option<Self>;
}

/// Merge `next` into `prev` when it directly follows `prev`, has the same
/// version, and the bodies merge.
pub fn try_merge<B: Mergeable>(
    prev: &Event<B>,
    next: &Event<B>,
    max_escaped: usize,
) -> Option<Event<B>> {
    if prev.seq.checked_add(1) != Some(next.first_seq()) || prev.v != next.v {
        return None;
    }
    let body = prev.body.merge(&next.body, max_escaped)?;
    Some(Event {
        seq: next.seq,
        from: Some(prev.first_seq()),
        v: next.v,
        ts: next.ts,
        cause: if prev.cause == next.cause {
            next.cause.clone()
        } else {
            None
        },
        fallback: if prev.fallback == next.fallback {
            next.fallback.clone()
        } else {
            None
        },
        body,
    })
}

/// Greedily coalesce a contiguous run of events.
pub fn coalesce<B: Mergeable>(events: Vec<Event<B>>, max_escaped: usize) -> Vec<Event<B>> {
    let mut out: Vec<Event<B>> = Vec::with_capacity(events.len());
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
    use std::collections::BTreeMap;

    use proptest::prelude::*;

    use super::*;
    use crate::annotations::TimestampMs;
    use crate::sync::StreamCursor;
    use crate::text::escaped_len;

    #[derive(Debug, Clone, PartialEq)]
    enum Body {
        Delta { item: u8, text: String },
        Other,
    }

    impl Mergeable for Body {
        fn merge(&self, next: &Self, max_escaped: usize) -> Option<Self> {
            match (self, next) {
                (
                    Body::Delta { item, text: a },
                    Body::Delta {
                        item: other,
                        text: b,
                    },
                ) if item == other && escaped_len(a) + escaped_len(b) <= max_escaped => {
                    Some(Body::Delta {
                        item: *item,
                        text: format!("{a}{b}"),
                    })
                }
                _ => None,
            }
        }
    }

    fn body() -> impl Strategy<Value = Body> {
        prop_oneof![
            6 => (0u8..3, "\\PC{0,30}|[\"\\\\\\n\\x01]{0,8}")
                .prop_map(|(item, text)| Body::Delta { item, text }),
            1 => Just(Body::Other),
        ]
    }

    /// What a reader ends up with: the text of each item and the other events seen.
    fn apply(events: &[Event<Body>]) -> (BTreeMap<u8, String>, usize) {
        let mut texts = BTreeMap::new();
        let mut others = 0;
        for e in events {
            match &e.body {
                Body::Delta { item, text } => texts
                    .entry(*item)
                    .or_insert_with(String::new)
                    .push_str(text),
                Body::Other => others += 1,
            }
        }
        (texts, others)
    }

    proptest! {
        #[test]
        fn unit_proto_base_coalesce_keeps_content_and_contiguity(
            bodies in prop::collection::vec(body(), 0..200),
            cap in 6usize..200,
        ) {
            let events: Vec<Event<Body>> = bodies
                .into_iter()
                .zip(1..)
                .map(|(body, seq)| Event { seq, from: None, v: 1, ts: TimestampMs(seq), cause: None, fallback: None, body })
                .collect();
            let merged = coalesce(events.clone(), cap);
            let mut cursor = StreamCursor::at(0);
            for e in &merged {
                cursor.accept(e).unwrap();
                if let (Some(_), Body::Delta { text, .. }) = (e.from, &e.body) {
                    prop_assert!(escaped_len(text) <= cap);
                }
            }
            prop_assert_eq!(cursor.last_seq(), events.len() as u64);
            prop_assert_eq!(apply(&merged), apply(&events));
        }
    }
}
