//! Real input (design 5.7.8) and answering a request (design 5.2.7): what
//! the window's commands must show, and who may choose which option.

use std::time::Duration;

use cupper_protocol::domain::Tier;
use cupper_protocol::interaction::{
    EffectScope, InteractionOption, InteractionOutcome, InteractionRequest, OptionKind,
};
use cupper_protocol::methods::git::GitApplyParams;
use cupper_protocol::methods::handshake::Principal;
use cupper_protocol::methods::interaction::{
    InputEvidence, InputKey, InputKind, InteractionRespondParams,
};
use cupper_protocol::rpc::Scope;

use super::{Denial, is_window, option_tier, require, tier_change};
use crate::clock::MonoTime;

/// The shortest time from a request to the window's answer: ADR-03's input
/// guard, kept by the host rather than the renderer (design 5.2.5).
pub const MIN_ANSWER_DELAY: Duration = Duration::from_millis(400);

/// What a piece of input evidence can stand for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Activation {
    /// A click, a native confirmation, Enter, Space or ⌘Enter.
    Plain,
    Escape,
    /// A digit key of the request panel, 1 to 9.
    Digit(u8),
}

/// Reads the evidence the main process wrote; `key` goes with `kind: key`
/// only, `digit` with `key: digit` only.
fn activation(input: Option<&InputEvidence>) -> Result<Activation, Denial> {
    use InputKey::{CmdEnter, Digit, Enter, Escape, Space};
    let input = input.ok_or(Denial::UserActivation)?;
    match (&input.kind, &input.key, input.digit) {
        (InputKind::Click | InputKind::NativeConfirm, None, None) => Ok(Activation::Plain),
        (InputKind::Key, Some(Enter | Space | CmdEnter), None) => Ok(Activation::Plain),
        (InputKind::Key, Some(Escape), None) => Ok(Activation::Escape),
        (InputKind::Key, Some(Digit), Some(digit)) => Ok(Activation::Digit(digit.into())),
        _ => Err(Denial::UserActivation),
    }
}

/// `git/revert` from the window takes a click or a confirming key; Esc and
/// the digit keys never revert. Other principals send no evidence.
pub fn git_revert(principal: &Principal, params: &GitApplyParams) -> Result<(), Denial> {
    if is_window(principal) && activation(params.input.as_ref())? != Activation::Plain {
        return Err(Denial::UserActivation);
    }
    Ok(())
}

/// A pending request as the interaction broker holds it.
#[derive(Debug, Clone, Copy)]
pub struct Pending<'a> {
    pub request: &'a InteractionRequest,
    /// When the broker took the request in, on the host's clock.
    pub entered: MonoTime,
    /// The session's tier, for an option with `mode_after`; `None` while unknown.
    pub tier: Option<&'a Tier>,
}

/// `interaction/respond`, once the broker has found the request pending and
/// the digest matching (design 5.2.7): the chosen option exists and the
/// principal may choose it, its `mode_after` follows the tier rules, and an
/// answer from the window shows fitting input evidence and comes at least
/// [`MIN_ANSWER_DELAY`] after the request, by `now` on the same clock.
pub fn respond(
    principal: &Principal,
    pending: &Pending<'_>,
    params: &InteractionRespondParams,
    now: MonoTime,
) -> Result<(), Denial> {
    let chosen = match &params.outcome {
        InteractionOutcome::Selected(selected) => {
            Some(choose(pending.request, &selected.option_id)?)
        }
        _ => None,
    };
    if let Some((_, option)) = chosen {
        require(principal, &requires(option))?;
        if let Some(tier) = option_tier(&option.effect) {
            tier_change(principal, pending.tier, &tier)?;
        }
    }
    if !is_window(principal) {
        return Ok(());
    }
    let fits = match activation(params.input.as_ref())? {
        Activation::Plain => true,
        // Esc only says no: `dismissed`, or an option that rejects.
        Activation::Escape => match chosen {
            Some((_, option)) => {
                matches!(
                    option.effect.scope,
                    EffectScope::Reject | EffectScope::PlanRevise
                )
            }
            None => params.outcome == InteractionOutcome::Dismissed,
        },
        // A digit picks the option at that number in `options`, privileged
        // and unknown ones counted, or a choice of the question on screen.
        Activation::Digit(digit) => match (pending.request, chosen, &params.outcome) {
            (
                InteractionRequest::Permission(_) | InteractionRequest::PlanConfirm(_),
                Some((index, _)),
                _,
            ) => usize::from(digit) == index + 1,
            (InteractionRequest::Question(_), None, InteractionOutcome::Answer(_)) => true,
            _ => false,
        },
    };
    if !fits {
        return Err(Denial::UserActivation);
    }
    if now.saturating_duration_since(pending.entered) < MIN_ANSWER_DELAY {
        return Err(Denial::TooEarly);
    }
    Ok(())
}

/// The option `option_id` of `request`, with its place in `options` from 0.
fn choose<'r>(
    request: &'r InteractionRequest,
    option_id: &str,
) -> Result<(usize, &'r InteractionOption), Denial> {
    let options = match request {
        InteractionRequest::Permission(request) => &request.options,
        InteractionRequest::PlanConfirm(request) => &request.options,
        _ => return Err(Denial::NoSuchOption),
    };
    options
        .iter()
        .enumerate()
        .find(|(_, option)| option.option_id == option_id)
        .ok_or(Denial::NoSuchOption)
}

/// The scope choosing `option` takes: its `requires`, but `privileged` for
/// an `allow_always` option and one whose kind or effect is unknown,
/// whatever the option table says (design 5.3.6).
pub fn requires(option: &InteractionOption) -> Scope {
    let kind = matches!(
        option.kind,
        OptionKind::AllowAlways | OptionKind::Unknown(_)
    );
    let effect = &option.effect.scope;
    let effect = matches!(effect, EffectScope::Unclassified | EffectScope::Unknown(_));
    if kind || effect || option.requires != Scope::Approve {
        Scope::Privileged
    } else {
        Scope::Approve
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::policy::testkit::{COLUMNS, column, option, permission, respond_params, selected};

    fn key(name: &str) -> Option<Value> {
        Some(json!({"kind": "key", "key": name}))
    }

    fn digit(n: u8) -> Option<Value> {
        Some(json!({"kind": "key", "key": "digit", "digit": n}))
    }

    fn click() -> Option<Value> {
        Some(json!({"kind": "click"}))
    }

    /// A permission request whose options are, by number: 1 privileged
    /// (`allow_always`), 2 of an unknown kind, 3 allow once, 4 reject.
    fn request() -> InteractionRequest {
        permission(vec![
            option("always", "allow_always", "persistent", "privileged"),
            option("mystery", "allow_maybe", "unknown", "privileged"),
            option("once", "allow_once", "once", "approve"),
            option("no", "reject_once", "reject", "approve"),
        ])
    }

    const LATER: MonoTime = MonoTime::from_origin(Duration::from_secs(10));

    fn answer(name: &str, outcome: InteractionOutcome, input: Option<Value>) -> Result<(), Denial> {
        let request = request();
        let pending = Pending {
            request: &request,
            entered: MonoTime::ZERO,
            tier: Some(&Tier::Normal),
        };
        respond(
            &column(name),
            &pending,
            &respond_params(outcome, input),
            LATER,
        )
    }

    /// Answers from the window: `(option chosen or None to dismiss, input, allowed)`.
    fn window(cases: &[(Option<&str>, Option<Value>, bool)]) {
        for (option_id, input, allowed) in cases {
            let outcome = option_id.map_or(InteractionOutcome::Dismissed, selected);
            let expected = if *allowed {
                Ok(())
            } else {
                Err(Denial::UserActivation)
            };
            let got = answer("UI window", outcome, input.clone());
            assert_eq!(got, expected, "{option_id:?} {input:?}");
        }
    }

    #[test]
    fn unit_policy_matrix_digit_is_the_number_in_options() {
        // Privileged and unknown options keep their numbers (RP-6). Numbered
        // among the approve options alone, "once" would be 1 and "no" 2.
        let mut cases = vec![
            (Some("once"), digit(3), true),
            (Some("no"), digit(4), true),
            (Some("no"), digit(2), false),
            // A digit never dismisses.
            (None, digit(4), false),
        ];
        cases.extend([1, 2, 4, 5, 9].map(|n| (Some("once"), digit(n), false)));
        window(&cases);
        // Other principals' evidence is ignored.
        assert_eq!(answer("UI control", selected("once"), digit(1)), Ok(()));
        assert_eq!(answer("cli + approve", selected("once"), None), Ok(()));
    }

    #[test]
    fn unit_policy_matrix_escape_only_says_no() {
        window(&[
            (Some("no"), key("escape"), true),
            (None, key("escape"), true),
            (Some("once"), key("escape"), false),
            (Some("once"), key("enter"), true),
            (Some("once"), key("space"), true),
            (Some("once"), key("cmd_enter"), true),
            (Some("once"), click(), true),
        ]);
    }

    #[test]
    fn unit_policy_matrix_window_needs_well_formed_evidence() {
        let malformed = [
            json!({"kind": "key"}),
            json!({"kind": "click", "key": "enter"}),
            json!({"kind": "key", "key": "enter", "digit": 3}),
            json!({"kind": "key", "key": "digit"}),
            json!({"kind": "click", "key": "digit", "digit": 3}),
            json!({"kind": "native_confirm", "digit": 3}),
            json!({"kind": "gaze"}),
            json!({"kind": "key", "key": "tab"}),
        ];
        let mut cases: Vec<_> = malformed
            .map(|input| (Some("once"), Some(input), false))
            .into();
        cases.push((Some("once"), None, false));
        cases.push((Some("once"), Some(json!({"kind": "native_confirm"})), true));
        window(&cases);
    }

    #[test]
    fn unit_policy_matrix_privileged_and_unknown_options_take_privileged() {
        for option_id in ["always", "mystery"] {
            for name in COLUMNS {
                let expected = match name {
                    "UI control" | "test_harness" => Ok(()),
                    _ => Err(Denial::Scope(Scope::Privileged)),
                };
                let got = answer(name, selected(option_id), click());
                assert_eq!(got, expected, "{option_id}: {name}");
            }
        }
        // Whatever the option table says about these.
        let option = |kind, scope| requires(&option("x", kind, scope, "approve"));
        assert_eq!(option("allow_always", "once"), Scope::Privileged);
        assert_eq!(option("allow_once", "unknown"), Scope::Privileged);
        assert_eq!(option("allow_once", "newer_scope"), Scope::Privileged);
        assert_eq!(option("allow_once", "once"), Scope::Approve);
        let missing = answer("UI control", selected("nope"), None);
        assert_eq!(missing, Err(Denial::NoSuchOption));
    }

    #[test]
    fn unit_policy_matrix_window_waits_400_ms() {
        let request = request();
        let entered = MonoTime::from_origin(Duration::from_secs(3));
        let pending = Pending {
            request: &request,
            entered,
            tier: Some(&Tier::Normal),
        };
        let params = respond_params(selected("once"), key("enter"));
        let at = |ms| entered + Duration::from_millis(ms);
        let window = column("UI window");
        for (ms, expected) in [
            (0, Err(Denial::TooEarly)),
            (399, Err(Denial::TooEarly)),
            (400, Ok(())),
        ] {
            assert_eq!(
                respond(&window, &pending, &params, at(ms)),
                expected,
                "{ms}"
            );
        }
        // Only the window waits.
        for name in [
            "UI control",
            "cli + approve",
            "test_harness",
            "remote_device",
        ] {
            let got = respond(&column(name), &pending, &params, at(0));
            assert_eq!(got, Ok(()), "{name}");
        }
        // Missing evidence is refused first, as 5.2.7 orders the checks.
        let bare = respond_params(selected("once"), None);
        let got = respond(&window, &pending, &bare, at(0));
        assert_eq!(got, Err(Denial::UserActivation));
    }

    #[test]
    fn unit_policy_matrix_question_answers() {
        let question: InteractionRequest = serde_json::from_value(json!({"type": "question",
            "prompt": "Which?", "unsupported_schema": false, "fields": [{"field_id": "q",
            "label": "Q", "type": "single_choice", "allow_custom": true, "required": true,
            "choices": [{"value": "a", "label": "A"}]}]}))
        .unwrap();
        let pending = Pending {
            request: &question,
            entered: MonoTime::ZERO,
            tier: None,
        };
        let answer = || -> InteractionOutcome {
            serde_json::from_value(json!({"type": "answer", "values": {"q": "a"}})).unwrap()
        };
        let refused = Err(Denial::UserActivation);
        for (outcome, input, expected) in [
            // The digit picks a choice of the question; the host cannot see which.
            (answer(), digit(1), Ok(())),
            (answer(), key("enter"), Ok(())),
            (answer(), key("escape"), refused.clone()),
            (answer(), None, refused),
            (InteractionOutcome::Dismissed, key("escape"), Ok(())),
            (selected("a"), key("enter"), Err(Denial::NoSuchOption)),
        ] {
            let params = respond_params(outcome, input.clone());
            let got = respond(&column("UI window"), &pending, &params, LATER);
            assert_eq!(got, expected, "{input:?}");
        }
    }

    #[test]
    fn unit_policy_matrix_revert_input() {
        let params = |input: Option<Value>| {
            let mut raw = json!({"command_id": "c-1", "session_id": "s_1",
                "diff_token": "dt_1", "target": {"all": true}});
            if let Some(input) = input {
                raw["input"] = input;
            }
            serde_json::from_value::<GitApplyParams>(raw).unwrap()
        };
        let refused = Err(Denial::UserActivation);
        for (input, expected) in [
            (click(), Ok(())),
            (key("enter"), Ok(())),
            (key("cmd_enter"), Ok(())),
            (None, refused.clone()),
            (key("escape"), refused.clone()),
            (digit(1), refused.clone()),
            (Some(json!({"kind": "key"})), refused),
        ] {
            let got = git_revert(&column("UI window"), &params(input.clone()));
            assert_eq!(got, expected, "{input:?}");
        }
        for name in ["UI control", "cli + write", "test_harness"] {
            assert_eq!(git_revert(&column(name), &params(None)), Ok(()), "{name}");
        }
    }
}
