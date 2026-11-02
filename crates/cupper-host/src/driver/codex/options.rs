//! Codex's option table (design 5.3.6, 5.6.4): what each permission option
//! of codex-acp 2.2.2 does, by the scene its request is in. The scene comes
//! from the tool call: `edit` changes files, `execute` runs a command, and a
//! `plan-review:` call confirms a plan. Codex has one way to say no, `cancel`,
//! and it ends the turn (Codex #556), as does `revise_plan` (T102).
//!
//! An option the table does not have falls back on its kind
//! ([`crate::driver::options`]), a reject ending the turn. A persistent
//! option names the command prefix it would allow, from its text or the
//! command.

use agent_client_protocol::schema::v1::{PermissionOption, RequestPermissionRequest};
use cupper_protocol::interaction::{EffectScope, InteractionOption, OptionEffect, OptionKind};
use cupper_protocol::rpc::Scope;

use crate::driver::extract::wire;
use crate::driver::options::{self, Row, effect, kind};

/// What a permission request is about, for its options' effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scene {
    FileChange,
    Command,
    /// A tool of another kind: its options go by their kind.
    Other,
    /// The plan review that ends a turn in plan mode.
    PlanConfirm,
}

impl Scene {
    pub fn of(request: &RequestPermissionRequest) -> Scene {
        let call = &request.tool_call;
        if call.tool_call_id.to_string().starts_with("plan-review:") {
            return Scene::PlanConfirm;
        }
        match wire(&call.fields.kind).as_str() {
            "edit" => Scene::FileChange,
            "execute" => Scene::Command,
            _ => Scene::Other,
        }
    }
}

/// The option as the protocol carries it; `command` is the request's command.
pub fn option(scene: Scene, option: &PermissionOption, command: Option<&str>) -> InteractionOption {
    let mut built = options::option(option, |id, kind| row(scene, id, kind), true);
    if built.effect.persistent && scene != Scene::PlanConfirm {
        built.effect.rule = Some(options::rule(prefix(&option.name, command)));
    }
    built
}

/// The options of `request` the table does not have, named with their
/// scene: GOLD-A checks that a recording has none.
pub fn unlisted(request: &RequestPermissionRequest) -> Vec<String> {
    let scene = Scene::of(request);
    let listed = |o: &&PermissionOption| row(scene, &o.option_id.to_string(), &kind(o)).is_some();
    let missing = request.options.iter().filter(|o| !listed(o));
    missing
        .map(|o| format!("option {} in {scene:?}", o.option_id))
        .collect()
}

/// The table, from SP-1's recordings and T102 (5.3.6). The kind must match
/// as well.
fn row(scene: Scene, id: &str, kind: &OptionKind) -> Option<Row> {
    use {EffectScope as S, OptionKind as K, Scene::*, Scope::*};
    let ending = |scope| {
        (
            OptionEffect {
                ends_turn: Some(true),
                ..effect(scope, false)
            },
            Approve,
        )
    };
    Some(match (scene, id, kind) {
        (FileChange | Command, "allow_once", K::AllowOnce) => (effect(S::Once, false), Approve),
        (FileChange, "allow_for_session", K::AllowAlways) => {
            (effect(S::Session, false), Privileged)
        }
        // Writes Codex's exec policy.
        (Command, "accept_execpolicy_amendment", K::AllowAlways) => {
            (effect(S::Persistent, true), Privileged)
        }
        (FileChange | Command, "cancel", K::RejectOnce) => ending(S::Reject),
        // Goes on implementing in the same prompt.
        (PlanConfirm, "implement_plan", K::AllowOnce) => (effect(S::PlanAccept, false), Approve),
        (PlanConfirm, "revise_plan", K::RejectOnce) => ending(S::PlanRevise),
        _ => return None,
    })
}

/// The prefix a persistent option allows: from Codex's text ("don't ask again
/// for commands that start with `<prefix>`"), else the command's first word.
fn prefix(name: &str, command: Option<&str>) -> Option<String> {
    let quoted = (name.split_once("commands that start with `"))
        .and_then(|(_, rest)| rest.strip_suffix('`'));
    let first = || command?.split_whitespace().next();
    quoted
        .or_else(first)
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// One case per line: `scene option kind => scope persistent ends_turn
    /// requires prefix_risk`, `-` for none.
    const EFFECTS: &str = "
        FileChange allow_once allow_once => once false false approve -
        FileChange allow_for_session allow_always => session false false privileged -
        FileChange cancel reject_once => reject false true approve -
        Command allow_once allow_once => once false false approve -
        Command accept_execpolicy_amendment allow_always => persistent true false privileged normal
        Command cancel reject_once => reject false true approve -
        PlanConfirm implement_plan allow_once => plan_accept false false approve -
        PlanConfirm revise_plan reject_once => plan_revise false true approve -
        Command allow_for_session allow_always => persistent true false privileged normal
        Command decline reject_once => reject false true approve -
        Other cancel reject_once => reject false true approve -
        FileChange allow_once allow_always => persistent true false privileged unknown
        Command allow_once allow_forever => unknown true - privileged normal";

    /// 5.3.6's Codex rows; options the table does not have, or whose kind
    /// does not match it, go by their kind, and a reject ends the turn.
    #[test]
    fn unit_option_map_codex_effects_by_scene() {
        use Scene::*;
        for case in EFFECTS.trim().lines().map(str::trim) {
            let (input, expected) = case.split_once(" => ").expect("a case");
            let [scene, id, kind] = input.split(' ').collect::<Vec<_>>()[..] else {
                panic!("{case}")
            };
            let scene = [FileChange, Command, Other, PlanConfirm]
                .into_iter()
                .find(|s| format!("{s:?}") == scene)
                .expect("a scene");
            let mut raw = json!({"optionId": id, "name": "Yes", "kind": kind});
            if serde_json::from_value::<PermissionOption>(raw.clone()).is_err() {
                raw["kind"] = json!("reject_once");
                raw["_meta"] = json!({"cupper": {"original": {"kind": kind}}});
            }
            let raw = serde_json::from_value(raw).expect("an option");
            let command = (scene == Command).then_some("ls -la");
            let built = serde_json::to_value(option(scene, &raw, command)).expect("json");
            let effect = &built["effect"];
            let got = [
                &effect["scope"],
                &effect["persistent"],
                &effect["ends_turn"],
                &built["requires"],
                &effect["rule"]["prefix_risk"],
            ];
            let got: Vec<String> = (got.iter())
                .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
                .map(|v| if v == "null" { "-".to_owned() } else { v })
                .collect();
            assert_eq!(got.join(" "), expected, "{case}");
        }
    }

    #[test]
    fn unit_option_map_codex_scene_and_prefix() {
        use Scene::*;
        let cases = [
            ("edit", "c", FileChange),
            ("execute", "c", Command),
            ("fetch", "c", Other),
        ];
        for (kind, id, want) in
            cases
                .into_iter()
                .chain([("switch_mode", "plan-review:p", PlanConfirm)])
        {
            let request = json!({"sessionId": "s", "toolCall": {"toolCallId": id, "kind": kind}, "options": []});
            let request = serde_json::from_value(request).expect("a request");
            assert_eq!(Scene::of(&request), want, "{kind}");
        }
        // Codex's text in SP-1 (s04b, s07).
        let named = "Yes, and don't ask again for commands that start with `printf d > d.txt`";
        assert_eq!(prefix(named, None).as_deref(), Some("printf d > d.txt"));
        assert_eq!(prefix("Yes", Some("bash -lc x")).as_deref(), Some("bash"));
        assert_eq!(prefix("Yes", None), None);
    }
}
