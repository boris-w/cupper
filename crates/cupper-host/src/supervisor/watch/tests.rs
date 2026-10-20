//! UNIT-orphan-policy: the watch's rules as tables of what the supervisor
//! sees, minute by minute, and what it decides, on `ManualClock`.

use std::time::Duration;

use super::*;
use crate::pure::CoreDriver;

const MIN: Duration = Duration::from_secs(60);

#[derive(Clone, Copy)]
enum Step {
    /// Clients, running turns, pending interactions, queued messages.
    See(usize, u32, u32, u32),
    Installing,
    Orphan,
    Changed,
    Wait(u32),
}
use Step::*;

/// `idle` sees nothing to do and nobody connected.
const IDLE: Step = See(0, 0, 0, 0);

/// What the core decided, with the minute: `close <why>` or `exit <why>`.
fn run(steps: &[Step]) -> Vec<String> {
    let mut core = CoreDriver::new(WatchCore::new(&Durations::default()));
    let (mut minute, mut said) = (0, Vec::new());
    for step in steps {
        let input = |seen: Observed| Input::Observed(seen);
        let actions = match *step {
            See(clients, running_turns, pending_interactions, queued_messages) => {
                let activity = Activity {
                    running_turns,
                    pending_interactions,
                    queued_messages,
                };
                core.input(input(Observed {
                    clients,
                    activity,
                    installer_busy: false,
                }))
            }
            Installing => core.input(input(Observed {
                installer_busy: true,
                ..Observed::default()
            })),
            Orphan => core.input(Input::Orphaned),
            Changed => core.input(Input::BundleChanged),
            Wait(minutes) => {
                minute += minutes;
                core.advance(MIN * minutes)
            }
        };
        said.extend(actions.into_iter().filter_map(|action| match action {
            Action::Close(why) => Some(format!("{minute} close {why:?}")),
            Action::Exit(why) => Some(format!("{minute} exit {why:?}")),
            Action::Timer(_) => None,
        }));
    }
    said
}

#[test]
fn unit_orphan_policy_idle_ten_minutes_and_any_activity_starts_over() {
    let cases: &[(&str, &[Step], &[&str])] = &[
        ("idle", &[IDLE, Wait(9), Wait(1)], &["10 exit Idle"]),
        (
            "not before a first look",
            &[Wait(20), IDLE, Wait(10)],
            &["30 exit Idle"],
        ),
        (
            "a client",
            &[IDLE, Wait(6), See(1, 0, 0, 0), Wait(6), IDLE, Wait(9)],
            &[],
        ),
        (
            "a turn",
            &[IDLE, Wait(6), See(0, 1, 0, 0), IDLE, Wait(10)],
            &["16 exit Idle"],
        ),
        ("a request", &[IDLE, Wait(9), See(0, 0, 1, 0), Wait(5)], &[]),
        (
            "a queued message",
            &[IDLE, Wait(9), See(0, 0, 0, 1), Wait(5)],
            &[],
        ),
        (
            "the installer",
            &[IDLE, Wait(9), Installing, Wait(5), IDLE, Wait(10)],
            &["24 exit Idle"],
        ),
    ];
    for (name, steps, expected) in cases {
        assert_eq!(run(steps), *expected, "{name}");
    }
}

#[test]
fn unit_orphan_policy_orphaned_refuses_and_waits_for_running_turns() {
    let cases: &[(&str, &[Step], &[&str])] = &[
        (
            "a running turn ends",
            &[
                See(1, 1, 1, 2),
                Orphan,
                Wait(30),
                See(1, 1, 0, 2),
                See(1, 0, 0, 2),
            ],
            &["0 close Orphaned", "30 exit Orphaned"],
        ),
        (
            "clients do not keep it",
            &[See(2, 0, 0, 0), Orphan, See(2, 0, 0, 0)],
            &["0 close Orphaned", "0 exit Orphaned"],
        ),
        // The turn may have started after that look.
        (
            "not on a look from before",
            &[See(0, 0, 0, 0), Orphan, See(0, 1, 0, 0)],
            &["0 close Orphaned"],
        ),
        (
            "once",
            &[See(0, 1, 0, 0), Orphan, Orphan],
            &["0 close Orphaned"],
        ),
    ];
    for (name, steps, expected) in cases {
        assert_eq!(run(steps), *expected, "{name}");
    }
}

#[test]
fn unit_orphan_policy_pending_upgrade_exits_once_nothing_is_left() {
    let cases: &[(&str, &[Step], &[&str])] = &[
        (
            "at the next look, clients or not",
            &[See(3, 0, 0, 0), Changed, See(3, 0, 0, 0)],
            &["0 close PendingUpgrade", "0 exit Upgrade"],
        ),
        (
            "after the turn, the request and the queue",
            &[
                See(1, 1, 1, 1),
                Changed,
                See(1, 0, 1, 1),
                See(1, 0, 0, 1),
                Wait(1),
                See(1, 0, 0, 0),
            ],
            &["0 close PendingUpgrade", "1 exit Upgrade"],
        ),
        (
            "before a first look",
            &[Changed, Wait(1), See(1, 0, 0, 0)],
            &["0 close PendingUpgrade", "1 exit Upgrade"],
        ),
    ];
    for (name, steps, expected) in cases {
        assert_eq!(run(steps), *expected, "{name}");
    }
}

#[test]
fn unit_orphan_policy_upgrade_comes_before_orphaned_before_idle() {
    let cases: &[(&str, &[Step], &[&str])] = &[
        (
            "an upgrade closes over the orphaned state",
            &[See(0, 1, 0, 0), Orphan, Changed, See(0, 0, 0, 0)],
            &[
                "0 close Orphaned",
                "0 close PendingUpgrade",
                "0 exit Upgrade",
            ],
        ),
        (
            "the orphaned state does not take back an upgrade",
            &[See(0, 1, 0, 0), Changed, Orphan],
            &["0 close PendingUpgrade"],
        ),
        // A pending request keeps the upgrade waiting, not the orphaned exit.
        (
            "orphaned exits first",
            &[See(0, 0, 1, 0), Changed, Orphan, See(0, 0, 1, 0)],
            &["0 close PendingUpgrade", "0 exit Orphaned"],
        ),
        (
            "idle, then nothing more",
            &[IDLE, Wait(10), Changed, Orphan, Wait(20)],
            &["10 exit Idle"],
        ),
    ];
    for (name, steps, expected) in cases {
        assert_eq!(run(steps), *expected, "{name}");
    }
}
