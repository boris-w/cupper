//! Approval tiers (design 5.8.2): who may loosen a session to which tier.
//! Three ways change a tier, all by these rules: `session/create`'s
//! `config`, `session/set_config`, and an option with `mode_after`.

use cupper_protocol::domain::{PrincipalKind, Tier};
use cupper_protocol::interaction::OptionEffect;
use cupper_protocol::methods::handshake::Principal;
use cupper_protocol::rpc::Scope;

use super::Denial;

/// The order of a tier one would move to: one this version does not know
/// counts as `full_access`, like a mode the mapping lacks (design 5.6.5).
pub fn rank(tier: &Tier) -> u8 {
    match tier {
        Tier::Normal => 0,
        Tier::Elevated => 1,
        Tier::FullAccess | Tier::Unknown(_) => 2,
    }
}

/// The loosest tier `principal` may put a session in: `full_access` takes
/// `privileged` (the control connection, after the native confirmation),
/// `elevated` takes `approve`, and a remote device loosens nothing (SEC-9).
pub fn ceiling(principal: &Principal) -> Tier {
    let has = |scope| principal.scopes.contains(&scope);
    if principal.kind == PrincipalKind::RemoteDevice {
        Tier::Normal
    } else if has(Scope::Privileged) {
        Tier::FullAccess
    } else if has(Scope::Approve) {
        Tier::Elevated
    } else {
        Tier::Normal
    }
}

/// Moving a session from tier `from` to `to`. A new session (`from` is
/// `None`) and one whose tier is unknown start from `normal`. Tightening or
/// keeping the tier takes nothing beyond the method's scope.
pub fn tier_change(principal: &Principal, from: Option<&Tier>, to: &Tier) -> Result<(), Denial> {
    let from = match from {
        None | Some(Tier::Unknown(_)) => 0,
        Some(tier) => rank(tier),
    };
    let to = rank(to);
    if to <= from || to <= rank(&ceiling(principal)) {
        Ok(())
    } else if principal.kind == PrincipalKind::RemoteDevice {
        Err(Denial::Sec9)
    } else if to == rank(&Tier::Elevated) {
        Err(Denial::Scope(Scope::Approve))
    } else {
        Err(Denial::Scope(Scope::Privileged))
    }
}

/// The tier choosing an option puts the session in: its `tier_after`, or
/// `full_access` for a `mode_after` the mapping does not know (design 5.6.5).
pub fn option_tier(effect: &OptionEffect) -> Option<Tier> {
    match (&effect.tier_after, &effect.mode_after) {
        (Some(tier), _) => Some(tier.clone()),
        (None, Some(_)) => Some(Tier::FullAccess),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::clock::MonoTime;
    use crate::policy::testkit::{column, option, permission, respond_params, selected, with_mode};
    use crate::policy::{MIN_ANSWER_DELAY, Pending, respond};

    /// The columns of the tier table of 5.8.2.
    const COLUMNS: [&str; 6] = [
        "UI control",
        "UI window",
        "cli + write",
        "cli + write + approve",
        "test_harness",
        "remote_device",
    ];

    /// A cell: `Y` allowed (`C`: after the main process's native
    /// confirmation); refused for lack of `approve` (`A`) or `privileged`
    /// (`P`); `S` by SEC-9.
    fn expected(cell: char) -> Result<(), Denial> {
        match cell {
            'Y' | 'C' => Ok(()),
            'A' => Err(Denial::Scope(Scope::Approve)),
            'P' => Err(Denial::Scope(Scope::Privileged)),
            'S' => Err(Denial::Sec9),
            _ => panic!("no cell {cell}"),
        }
    }

    fn check(from: Option<&Tier>, to: &Tier, cells: &str) {
        for (name, cell) in COLUMNS.into_iter().zip(cells.chars()) {
            let got = tier_change(&column(name), from, to);
            assert_eq!(got, expected(cell), "{from:?} -> {to:?}: {name}");
        }
    }

    #[test]
    fn unit_tier_rules_tighten_or_keep_is_open_to_all() {
        let (normal, elevated, full) = (Tier::Normal, Tier::Elevated, Tier::FullAccess);
        let changes = [(&elevated, &normal), (&full, &normal), (&full, &elevated)];
        let kept = [(&normal, &normal), (&elevated, &elevated), (&full, &full)];
        for (from, to) in changes.into_iter().chain(kept) {
            check(Some(from), to, "YYYYYY");
        }
        check(None, &normal, "YYYYYY");
    }

    #[test]
    fn unit_tier_rules_loosen_to_elevated_takes_approve() {
        check(Some(&Tier::Normal), &Tier::Elevated, "YYAYYS");
        check(None, &Tier::Elevated, "YYAYYS");
    }

    #[test]
    fn unit_tier_rules_loosen_to_full_access_takes_privileged() {
        check(Some(&Tier::Normal), &Tier::FullAccess, "CPPPYS");
        check(Some(&Tier::Elevated), &Tier::FullAccess, "CPPPYS");
        check(None, &Tier::FullAccess, "CPPPYS");
    }

    #[test]
    fn unit_tier_rules_unknown_tiers_are_the_strict_reading() {
        // An unknown target is full access; an unknown current tier is normal.
        let unknown = Tier::Unknown("yolo".into());
        check(Some(&Tier::Normal), &unknown, "CPPPYS");
        check(Some(&unknown), &Tier::Elevated, "YYAYYS");
        check(Some(&unknown), &Tier::Normal, "YYYYYY");
    }

    #[test]
    fn unit_tier_rules_phone_keeps_but_never_loosens() {
        // SEC-9: a session already elevated stays usable; nothing loosens,
        // a new session from the phone (MB-9) included.
        let (normal, elevated, full) = (Tier::Normal, Tier::Elevated, Tier::FullAccess);
        let remote = column("remote_device");
        assert_eq!(ceiling(&remote), Tier::Normal);
        for (from, to, expected) in [
            (Some(&elevated), &elevated, Ok(())),
            (Some(&elevated), &normal, Ok(())),
            (Some(&elevated), &full, Err(Denial::Sec9)),
            (Some(&normal), &elevated, Err(Denial::Sec9)),
            (None, &elevated, Err(Denial::Sec9)),
        ] {
            assert_eq!(
                tier_change(&remote, from, to),
                expected,
                "{from:?} -> {to:?}"
            );
        }
    }

    #[test]
    fn unit_tier_rules_ceilings() {
        let ceilings = "full_access elevated normal elevated full_access normal";
        for (name, tier) in COLUMNS.into_iter().zip(ceilings.split(' ')) {
            assert_eq!(ceiling(&column(name)), Tier::from(tier), "{name}");
        }
        assert_eq!(ceiling(&column("cli + approve")), Tier::Elevated);
    }

    #[test]
    fn unit_tier_rules_mode_after() {
        let tier = |raw| option_tier(&serde_json::from_value::<OptionEffect>(raw).unwrap());
        assert_eq!(tier(json!({"scope": "once", "persistent": false})), None);
        let auto = json!({"scope": "mode_switch", "persistent": false,
            "mode_after": "auto", "tier_after": "elevated"});
        assert_eq!(tier(auto), Some(Tier::Elevated));
        // A mode the mapping lacks is full access.
        let unmapped = json!({"scope": "mode_switch", "persistent": false, "mode_after": "yolo"});
        assert_eq!(tier(unmapped), Some(Tier::FullAccess));

        // `approve` options that switch modes, so the tier rules alone
        // decide, answered by each principal that may answer.
        let modes = ["default", "auto", "bypass"];
        let request = permission(
            modes
                .into_iter()
                .zip(["normal", "elevated", "full_access"])
                .map(|(id, tier)| {
                    with_mode(option(id, "allow_once", "mode_switch", "approve"), id, tier)
                })
                .collect(),
        );
        let answerers = [
            "UI control",
            "UI window",
            "cli + approve",
            "test_harness",
            "remote_device",
        ];
        for (from, number, cells) in [
            (Tier::Normal, 1, "YYYYY"),
            (Tier::Normal, 2, "YYYYS"),
            (Tier::Normal, 3, "CPPYS"),
            (Tier::Elevated, 2, "YYYYY"),
            (Tier::Elevated, 3, "CPPYS"),
            (Tier::FullAccess, 3, "YYYYY"),
        ] {
            let pending = Pending {
                request: &request,
                entered: MonoTime::ZERO,
                tier: Some(&from),
            };
            let input = json!({"kind": "key", "key": "digit", "digit": number});
            let params = respond_params(selected(modes[number - 1]), Some(input));
            for (name, cell) in answerers.into_iter().zip(cells.chars()) {
                let got = respond(
                    &column(name),
                    &pending,
                    &params,
                    MonoTime::ZERO + MIN_ANSWER_DELAY,
                );
                assert_eq!(got, expected(cell), "{from:?} option {number}: {name}");
            }
        }
    }
}
