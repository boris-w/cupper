//! UNIT-elevation-core: the pending request, the silence after denials,
//! expiry, grants and their end, on `ManualClock`.

use std::time::Duration;

use cupper_protocol::annotations::WirePath;
use serde_json::json;

use super::*;
use crate::pure::CoreDriver;

const MIN: Duration = Duration::from_secs(60);

fn core() -> CoreDriver<ElevationCore> {
    CoreDriver::new(ElevationCore::new(&Durations::default()))
}

fn conn(n: u8) -> ConnId {
    format!("c_{n}").parse().unwrap()
}

fn id(n: u8) -> ElevationId {
    format!("e_{n}").parse().unwrap()
}

fn ask(n: u8, scopes: &[Scope]) -> Input {
    let verified = VerifiedRequester {
        executable: WirePath("/usr/local/bin/cupper".into()),
        signing: None,
        pid: 42,
        parent_chain: Vec::new(),
        has_tty: true,
        origin_unverifiable: false,
    };
    let said = SelfReported {
        client_name: "cupper".into(),
        reason: None,
    };
    let (id, conn, scopes) = (id(n), conn(n), scopes.to_vec());
    let examined = Examined::Clear(verified);
    Input::Ask(Ask {
        id,
        conn,
        scopes,
        examined,
        said,
    })
}

fn respond(n: u8, grant: Option<&[Scope]>) -> Input {
    Input::Respond {
        id: id(n),
        grant: grant.map(<[Scope]>::to_vec),
    }
}

/// The actions, as short words, timers left out.
fn words(actions: &[Action]) -> Vec<String> {
    let word = |action: &Action| match action {
        Action::Refused {
            refusal: Refusal::Lineage { .. },
            ..
        } => Some("lineage".into()),
        Action::Refused {
            refusal: Refusal::RateLimited { retry_after },
            ..
        } => Some(format!("rate_limited {}s", retry_after.as_secs())),
        Action::Requested(p) => Some(format!("requested {}", p.id)),
        Action::Resolved { id, outcome, .. } => {
            Some(format!("{id} {}", json!(format!("{outcome:?}"))))
        }
        Action::Revoked(conn) => Some(format!("{conn} revoked")),
        Action::Responded(result) => Some(format!("responded {}", result.is_ok())),
        Action::Timer(_) => None,
    };
    actions.iter().filter_map(word).collect()
}

#[test]
fn unit_elevation_core_one_request_at_a_time_and_expiry() {
    let mut c = core();
    assert_eq!(words(&c.input(ask(1, &[Scope::Write]))), ["requested e_1"]);
    c.advance(MIN / 2);
    // The second waits for the first, at most until it expires.
    assert_eq!(
        words(&c.input(ask(2, &[Scope::Write]))),
        ["rate_limited 90s"]
    );
    assert_eq!(words(&c.advance(2 * MIN)), ["e_1 \"Expired\""]);
    assert!(c.core().pending(&id(1)).is_none());
    assert_eq!(words(&c.input(respond(1, None))), ["responded false"]);
    assert_eq!(words(&c.input(ask(2, &[Scope::Write]))), ["requested e_2"]);
    // A request from a connection that closes ends with it.
    assert_eq!(words(&c.input(Input::Closed(conn(2)))), ["e_2 \"Expired\""]);
    // The lineage check comes first.
    let mut lineage = ask(3, &[Scope::Write]);
    if let Input::Ask(ask) = &mut lineage {
        ask.examined = Examined::Descendant {
            signals: vec!["ancestor".into()],
        };
    }
    assert_eq!(words(&c.input(lineage)), ["lineage"]);
}

#[test]
fn unit_elevation_core_two_denials_silence_for_ten_minutes() {
    let mut c = core();
    // Denials more than 10 minutes apart do not add up.
    for (n, gap) in [(1, 11 * MIN), (2, MIN), (3, MIN)] {
        c.input(ask(n, &[Scope::Write]));
        assert_eq!(
            words(&c.input(respond(n, None))),
            ["responded true", &format!("e_{n} \"Denied\"")]
        );
        c.advance(gap);
    }
    // The second denial within 10 minutes was e_3, one minute ago.
    assert_eq!(
        words(&c.input(ask(4, &[Scope::Write]))),
        ["rate_limited 540s"]
    );
    c.advance(9 * MIN);
    assert_eq!(words(&c.input(ask(4, &[Scope::Write]))), ["requested e_4"]);
}

#[test]
fn unit_elevation_core_grants_what_was_asked_until_close_or_fifteen_minutes() {
    let mut c = core();
    c.input(ask(1, &[Scope::Write]));
    // `approve` was not asked for: it is not granted.
    let granted = words(&c.input(respond(1, Some(&[Scope::Write, Scope::Approve]))));
    let ends = "Granted { scopes: [Write], ends_in: 900s }";
    assert_eq!(granted, ["responded true", &format!("e_1 \"{ends}\"")]);
    c.advance(14 * MIN);
    assert_eq!(words(&c.advance(MIN)), ["c_1 revoked"]);
    // A closed connection loses its grant at once.
    c.input(ask(2, &[Scope::Write, Scope::Approve]));
    c.input(respond(2, Some(&[Scope::Approve])));
    assert_eq!(words(&c.input(Input::Closed(conn(2)))), ["c_2 revoked"]);
    assert!(c.advance(20 * MIN).is_empty(), "nothing left to end");
}
