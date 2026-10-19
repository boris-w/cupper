//! Event upcasters and the version of checkpointed views (design 5.4.6,
//! ADR-06, NFR-10).
//!
//! An event's `v` is the version of its type's `data`. The host reads events
//! of older versions from the log, from checkpoints and from turn snapshots,
//! and [`Upcasters::upcast`] brings their `data` to the current version, one
//! version at a time, before anything uses or sends them: clients only ever
//! see current versions.
//!
//! - Every event type starts at [`FIRST_VERSION`]. An [`Upcaster`] takes one
//!   type from one version to the next, and a type's chain of upcasters makes
//!   its current version.
//! - Every upcaster has a unit test from an old sample to a new one: the
//!   tests fail for an entry of [`UPCASTERS`] without one.
//! - A checkpoint stores a window view, versioned on its own by
//!   [`VIEW_VERSION`]; see [`checkpoint_fate`].

use serde_json::Value;

/// The version every event type starts at.
pub const FIRST_VERSION: u32 = 1;

/// Takes the `data` of `event_type` events from version `from` to `from + 1`.
#[derive(Debug, Clone, Copy)]
pub struct Upcaster {
    pub event_type: &'static str,
    pub from: u32,
    /// `Err` says what the old `data` lacks.
    pub upgrade: fn(Value) -> Result<Value, &'static str>,
}

/// This host's upcasters. None yet: every event type is still at v1.
pub const UPCASTERS: &[Upcaster] = &[];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UpcastError {
    /// `v` is 0, or newer than the current version.
    #[error("`{event_type}` has no v{v}; the current version is v{current}")]
    UnknownVersion {
        event_type: String,
        v: u32,
        current: u32,
    },
    #[error("`{event_type}` v{from} cannot be upgraded: {reason}")]
    Malformed {
        event_type: String,
        from: u32,
        reason: &'static str,
    },
}

/// A set of upcasters whose chains are complete.
#[derive(Debug, Clone, Copy)]
pub struct Upcasters {
    steps: &'static [Upcaster],
}

impl Upcasters {
    /// Panics unless the upcasters of each type run from [`FIRST_VERSION`]
    /// without gaps or repeats, in any order.
    pub fn new(steps: &'static [Upcaster]) -> Self {
        for step in steps {
            let mut chain: Vec<u32> = steps
                .iter()
                .filter(|other| other.event_type == step.event_type)
                .map(|other| other.from)
                .collect();
            chain.sort_unstable();
            let complete: Vec<u32> = (FIRST_VERSION..).take(chain.len()).collect();
            assert_eq!(chain, complete, "the upcasters of `{}`", step.event_type);
        }
        Self { steps }
    }

    /// [`UPCASTERS`].
    pub fn host() -> Self {
        Self::new(UPCASTERS)
    }

    /// The version `event_type` events are written at.
    pub fn current(&self, event_type: &str) -> u32 {
        self.chain(event_type)
            .map(|step| step.from + 1)
            .max()
            .unwrap_or(FIRST_VERSION)
    }

    /// The `data` of an `event_type` event at version `v`, brought to the
    /// current version.
    pub fn upcast(&self, event_type: &str, v: u32, mut data: Value) -> Result<Value, UpcastError> {
        let current = self.current(event_type);
        if !(FIRST_VERSION..=current).contains(&v) {
            return Err(UpcastError::UnknownVersion {
                event_type: event_type.to_owned(),
                v,
                current,
            });
        }
        for from in v..current {
            let step = self
                .chain(event_type)
                .find(|step| step.from == from)
                .expect("`new` checked the chain");
            data = (step.upgrade)(data).map_err(|reason| UpcastError::Malformed {
                event_type: event_type.to_owned(),
                from,
                reason,
            })?;
        }
        Ok(data)
    }

    fn chain(&self, event_type: &str) -> impl Iterator<Item = &Upcaster> {
        self.steps
            .iter()
            .filter(move |step| step.event_type == event_type)
    }
}

/// The version of the window view in `checkpoints.view`, the JSON of the
/// protocol's index or session view (T014). Goes up with every change to
/// that JSON.
pub const VIEW_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointFate {
    Use,
    /// Rebuild the view from the latest 20 turn snapshots and the events
    /// after them instead (T026).
    Discard,
}

/// What to do with a checkpoint whose view has version `view_v`: any version
/// other than [`VIEW_VERSION`], older or newer, is discarded.
pub fn checkpoint_fate(view_v: u32) -> CheckpointFate {
    if view_v == VIEW_VERSION {
        CheckpointFate::Use
    } else {
        CheckpointFate::Discard
    }
}

#[cfg(test)]
mod tests {
    use std::panic::catch_unwind;

    use serde_json::json;

    use super::*;

    /// A test-only event type in three versions:
    /// v1 `{"text"}`, v2 `{"body": {"text"}}`, v3 `{"body": {"text", "format"}}`.
    /// Listed out of order on purpose.
    const NOTE: &[Upcaster] = &[
        Upcaster {
            event_type: "test.note",
            from: 2,
            upgrade: add_format,
        },
        Upcaster {
            event_type: "test.note",
            from: 1,
            upgrade: nest_text,
        },
    ];

    fn nest_text(data: Value) -> Result<Value, &'static str> {
        let text = data.get("text").ok_or("no `text`")?;
        Ok(json!({ "body": { "text": text } }))
    }

    fn add_format(mut data: Value) -> Result<Value, &'static str> {
        let body = data.get_mut("body").and_then(Value::as_object_mut);
        body.ok_or("no `body` object")?
            .insert("format".to_owned(), json!("plain"));
        Ok(data)
    }

    /// `(event_type, from, old sample, new sample)` for every upcaster, the
    /// host's and the tests'.
    const SAMPLES: &[(&str, u32, &str, &str)] = &[
        (
            "test.note",
            1,
            r#"{"text": "hi"}"#,
            r#"{"body": {"text": "hi"}}"#,
        ),
        (
            "test.note",
            2,
            r#"{"body": {"text": "hi"}}"#,
            r#"{"body": {"text": "hi", "format": "plain"}}"#,
        ),
    ];

    fn sample(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn upcast_every_upcaster_turns_its_old_sample_into_its_new_one() {
        for step in UPCASTERS.iter().chain(NOTE) {
            let samples: Vec<_> = SAMPLES
                .iter()
                .filter(|(event_type, from, ..)| {
                    (*event_type, *from) == (step.event_type, step.from)
                })
                .collect();
            assert!(
                !samples.is_empty(),
                "`{}` v{} has no sample",
                step.event_type,
                step.from
            );
            for (_, _, old, new) in samples {
                assert_eq!((step.upgrade)(sample(old)), Ok(sample(new)), "{old}");
            }
        }
    }

    #[test]
    fn upcast_brings_old_events_to_the_current_version_step_by_step() {
        let upcasters = Upcasters::new(NOTE);
        assert_eq!(upcasters.current("test.note"), 3);
        let v3 = sample(r#"{"body": {"text": "hi", "format": "plain"}}"#);
        for (v, old) in [(1, r#"{"text": "hi"}"#), (2, r#"{"body": {"text": "hi"}}"#)] {
            assert_eq!(
                upcasters.upcast("test.note", v, sample(old)),
                Ok(v3.clone())
            );
        }
        assert_eq!(upcasters.upcast("test.note", 3, v3.clone()), Ok(v3.clone()));
        // A type without upcasters is at its first version.
        assert_eq!(upcasters.current("test.other"), FIRST_VERSION);
        assert_eq!(upcasters.upcast("test.other", 1, v3.clone()), Ok(v3));
    }

    #[test]
    fn upcast_reports_versions_it_does_not_know_and_data_it_cannot_upgrade() {
        let upcasters = Upcasters::new(NOTE);
        let unknown = |event_type: &str, v, current| UpcastError::UnknownVersion {
            event_type: event_type.to_owned(),
            v,
            current,
        };
        let data = json!({});
        assert_eq!(
            upcasters.upcast("test.note", 0, data.clone()),
            Err(unknown("test.note", 0, 3))
        );
        assert_eq!(
            upcasters.upcast("test.note", 4, data.clone()),
            Err(unknown("test.note", 4, 3))
        );
        assert_eq!(
            upcasters.upcast("test.other", 2, data.clone()),
            Err(unknown("test.other", 2, 1))
        );
        assert_eq!(
            upcasters.upcast("test.note", 1, json!({"body": "hi"})),
            Err(UpcastError::Malformed {
                event_type: "test.note".to_owned(),
                from: 1,
                reason: "no `text`",
            })
        );
    }

    #[test]
    fn upcast_chains_start_at_the_first_version_without_gaps() {
        const GAP: &[Upcaster] = &[NOTE[0]];
        const REPEAT: &[Upcaster] = &[NOTE[1], NOTE[1]];
        for steps in [GAP, REPEAT] {
            assert!(catch_unwind(|| Upcasters::new(steps)).is_err());
        }
        assert_eq!(Upcasters::host().current("session.created"), FIRST_VERSION);
    }

    #[test]
    fn upcast_checkpoint_of_another_view_version_is_discarded() {
        assert_eq!(checkpoint_fate(VIEW_VERSION), CheckpointFate::Use);
        for other in [VIEW_VERSION - 1, VIEW_VERSION + 1] {
            assert_eq!(checkpoint_fate(other), CheckpointFate::Discard, "{other}");
        }
    }
}
