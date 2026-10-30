//! Claude's option table (design 5.3.6, 5.6.4): what each permission option
//! of claude-agent-acp 0.91.0 does, by the scene its request is in. The scene
//! comes from the tool: Write, Edit, MultiEdit and NotebookEdit edit files,
//! every other tool is a command or the like, and ExitPlanMode's request is
//! a plan confirmation. The same `allow-with-updates` lasts for the session
//! on a file edit but writes a persistent rule on a command, so when the tool
//! is not known it counts as persistent (Q7).
//!
//! An option the table does not have falls back on its kind
//! ([`crate::driver::options`]). A persistent option names the command prefix
//! it would allow, from its text or the command.

use agent_client_protocol::schema::v1::{PermissionOption, RequestPermissionRequest};
use cupper_protocol::interaction::{EffectScope, InteractionOption, OptionEffect, OptionKind};
use cupper_protocol::rpc::Scope;
use serde_json::Value;

use super::capabilities::tier;
use crate::driver::extract::wire;
use crate::driver::options::{self, Row, effect, kind};

/// What a permission request is about, for its options' effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scene {
    FileEdit,
    /// Bash and every other tool but the file editors.
    Command,
    /// No tool name to tell by.
    UnknownTool,
    /// ExitPlanMode: `switch_mode` with `exit-plan-` options.
    PlanConfirm,
}

impl Scene {
    pub fn of(request: &RequestPermissionRequest) -> Scene {
        let (call, fields) = (&request.tool_call, &request.tool_call.fields);
        let exits_plan = (request.options.iter())
            .any(|option| option.option_id.to_string().starts_with("exit-plan-"));
        if wire(&fields.kind) == "switch_mode" && exits_plan {
            return Scene::PlanConfirm;
        }
        let meta = call.meta.as_ref().and_then(|meta| meta.get("claudeCode"));
        let name = meta
            .and_then(|meta| meta.get("toolName"))
            .and_then(Value::as_str);
        match name.or(fields.name.as_deref()) {
            Some("Write" | "Edit" | "MultiEdit" | "NotebookEdit") => Scene::FileEdit,
            Some(_) => Scene::Command,
            None => Scene::UnknownTool,
        }
    }
}

/// The option as the protocol carries it; `command` is the request's command.
pub fn option(scene: Scene, option: &PermissionOption, command: Option<&str>) -> InteractionOption {
    let mut built = options::option(option, |id, kind| row(scene, id, kind), false);
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

/// The table, from SP-1's recordings (5.3.6). The kind must match as well.
fn row(scene: Scene, id: &str, kind: &OptionKind) -> Option<Row> {
    use {EffectScope as S, OptionKind as K, Scene::*, Scope::*};
    let switch = |scope, mode: &str| OptionEffect {
        mode_after: Some(mode.to_owned()),
        tier_after: Some(tier(mode)),
        ..effect(scope, false)
    };
    Some(match (scene, id, kind) {
        (FileEdit | Command | UnknownTool, "allow-once", K::AllowOnce) => {
            (effect(S::Once, false), Approve)
        }
        (FileEdit, "allow-with-updates", K::AllowAlways) => (effect(S::Session, false), Privileged),
        (Command | UnknownTool, "allow-with-updates", K::AllowAlways) => {
            (effect(S::Persistent, true), Privileged)
        }
        (FileEdit | Command | UnknownTool, "reject", K::RejectOnce) => {
            (effect(S::Reject, false), Approve)
        }
        (PlanConfirm, "exit-plan-default", K::AllowOnce) => {
            (switch(S::PlanAccept, "default"), Approve)
        }
        (PlanConfirm, "exit-plan-auto" | "exit-plan-clear-auto", K::AllowAlways) => {
            (switch(S::ModeSwitch, "auto"), Privileged)
        }
        (PlanConfirm, "exit-plan-bypass", K::AllowAlways) => {
            (switch(S::ModeSwitch, "bypassPermissions"), Privileged)
        }
        // The real CLI ends the turn as `cancelled` (T066, 2.1.296).
        (PlanConfirm, "reject", K::RejectOnce) => {
            let mut revise = effect(S::PlanRevise, false);
            revise.ends_turn = Some(true);
            (revise, Approve)
        }
        _ => return None,
    })
}

/// The prefix a persistent option allows: from Claude's text ("don't ask
/// again for <prefix> commands", "allow access to <dir> and <prefix> *
/// commands"), else the command's first word.
fn prefix(name: &str, command: Option<&str>) -> Option<String> {
    let after = |marker: &str| name.split_once(marker).map(|(_, rest)| rest);
    let access = after("allow access to ").and_then(|rest| rest.split_once(" and "));
    let quoted = (after("don't ask again for ").or(access.map(|(_, rest)| rest)))
        .and_then(|rest| rest.strip_suffix(" commands"))
        .map(|prefix| prefix.trim_end_matches(" *").trim_end_matches(":*"));
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

    /// The option as the table makes it; an unknown kind arrives the way
    /// the tap rewrites it (K19).
    fn built(
        scene: Scene,
        id: &str,
        kind: &str,
        name: &str,
        cmd: Option<&str>,
    ) -> InteractionOption {
        let mut option = json!({"optionId": id, "name": name, "kind": kind});
        if serde_json::from_value::<PermissionOption>(option.clone()).is_err() {
            option["kind"] = json!("reject_once");
            option["_meta"] = json!({"cupper": {"original": {"kind": kind}}});
        }
        let option = serde_json::from_value(option).expect("an option");
        super::option(scene, &option, cmd)
    }

    /// One case per line: `scene option kind => scope persistent requires
    /// mode_after tier_after prefix_risk`, `-` for none.
    const EFFECTS: &str = "
        FileEdit allow-once allow_once => once false approve - - -
        FileEdit allow-with-updates allow_always => session false privileged - - -
        Command allow-with-updates allow_always => persistent true privileged - - unknown
        UnknownTool allow-with-updates allow_always => persistent true privileged - - unknown
        Command reject reject_once => reject false approve - - -
        PlanConfirm exit-plan-default allow_once => plan_accept false approve default normal -
        PlanConfirm exit-plan-auto allow_always => mode_switch false privileged auto elevated -
        PlanConfirm exit-plan-clear-auto allow_always => mode_switch false privileged auto elevated -
        PlanConfirm exit-plan-bypass allow_always => mode_switch false privileged bypassPermissions full_access -
        PlanConfirm reject reject_once => plan_revise false approve - - -
        FileEdit allow-once allow_always => persistent true privileged - - unknown
        Command deny-forever reject_always => reject true approve - - unknown
        Command allow-once allow_forever => unknown true privileged - - unknown";

    /// Q7: `allow-with-updates` is for the session on edits, persistent on
    /// commands and wherever the tool is unknown. Options the table does not
    /// have, or whose kind does not match it, go by their kind.
    #[test]
    fn unit_option_map_claude_effects_by_scene() {
        use Scene::*;
        for case in EFFECTS.trim().lines().map(str::trim) {
            let (input, expected) = case.split_once(" => ").expect("a case");
            let [scene, id, kind] = input.split(' ').collect::<Vec<_>>()[..] else {
                panic!("{case}")
            };
            let scene = [FileEdit, Command, UnknownTool, PlanConfirm]
                .into_iter()
                .find(|s| format!("{s:?}") == scene)
                .expect("a scene");
            let option = serde_json::to_value(built(scene, id, kind, "", None)).expect("json");
            let effect = &option["effect"];
            let got = [
                &effect["scope"],
                &effect["persistent"],
                &option["requires"],
                &effect["mode_after"],
                &effect["tier_after"],
                &effect["rule"]["prefix_risk"],
            ];
            let got: Vec<String> = got
                .iter()
                .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
                .map(|v| if v == "null" { "-".to_owned() } else { v })
                .collect();
            assert_eq!(got.join(" "), expected, "{case}");
        }
    }

    /// T066 (2.1.296): rejecting a plan ends the turn as `cancelled`; it is
    /// still `plan_revise`. Every other option, and a reject elsewhere,
    /// leaves the turn running.
    #[test]
    fn unit_claude_plan_reject_ends_turn_only_the_plan_s_reject() {
        use Scene::*;
        let ends = |scene, id, kind| {
            let InteractionOption {
                effect, requires, ..
            } = built(scene, id, kind, "", None);
            (effect.ends_turn, json!(effect.scope), requires)
        };
        let plan_revise = (Some(true), json!("plan_revise"), Scope::Approve);
        assert_eq!(ends(PlanConfirm, "reject", "reject_once"), plan_revise);
        for (id, kind) in [
            ("exit-plan-default", "allow_once"),
            ("exit-plan-auto", "allow_always"),
            ("exit-plan-clear-auto", "allow_always"),
            ("exit-plan-bypass", "allow_always"),
        ] {
            assert_eq!(ends(PlanConfirm, id, kind).0, Some(false), "{id}");
        }
        for scene in [FileEdit, Command, UnknownTool] {
            let reject = (Some(false), json!("reject"), Scope::Approve);
            assert_eq!(ends(scene, "reject", "reject_once"), reject, "{scene:?}");
        }
    }

    #[test]
    fn unit_option_map_claude_scene_from_the_tool() {
        let scene = |tool_call: Value, option: &str| {
            let request = json!({"sessionId": "s", "toolCall": tool_call,
                "options": [{"optionId": option, "name": "", "kind": "allow_once"}]});
            Scene::of(&serde_json::from_value(request).expect("a request"))
        };
        let named = |name| json!({"toolCallId": "t", "name": name});
        assert_eq!(scene(named("Write"), "allow-once"), Scene::FileEdit);
        assert_eq!(scene(named("NotebookEdit"), "allow-once"), Scene::FileEdit);
        assert_eq!(scene(named("Bash"), "allow-once"), Scene::Command);
        let meta = json!({"toolCallId": "t", "_meta": {"claudeCode": {"toolName": "Edit"}}});
        assert_eq!(scene(meta, "allow-once"), Scene::FileEdit);
        assert_eq!(
            scene(json!({"toolCallId": "t"}), "allow-once"),
            Scene::UnknownTool
        );
        let plan = json!({"toolCallId": "t", "kind": "switch_mode", "name": "ExitPlanMode"});
        assert_eq!(scene(plan.clone(), "exit-plan-default"), Scene::PlanConfirm);
        assert_eq!(scene(plan, "allow-once"), Scene::Command);
    }

    #[test]
    fn unit_option_map_claude_prefix_and_its_risk() {
        let rule = |name: &str, command| {
            let option = built(
                Scene::Command,
                "allow-with-updates",
                "allow_always",
                name,
                command,
            );
            let rule = option.effect.rule.expect("a persistent option's rule");
            (rule.prefix, json!(rule.prefix_risk))
        };
        // Claude's texts in SP-1 (s03, s04b).
        let sh = "Yes, and don't ask again for sh -c 'echo out; exit 3' commands";
        assert_eq!(
            rule(sh, None),
            (
                Some("sh -c 'echo out; exit 3'".into()),
                json!("interpreter")
            )
        );
        let printf = "Yes, and allow access to claude/ and printf d * commands";
        assert_eq!(
            rule(printf, None),
            (Some("printf d".into()), json!("normal"))
        );
        // Else the command's first word.
        assert_eq!(
            rule("Yes", Some("python3 x.py")),
            (Some("python3".into()), json!("interpreter"))
        );
        assert_eq!(
            rule("Yes", Some("/usr/bin/env node")),
            (Some("/usr/bin/env".into()), json!("interpreter"))
        );
        assert_eq!(
            rule("Yes", Some("cargo test")),
            (Some("cargo".into()), json!("normal"))
        );
        assert_eq!(rule("Yes", None), (None, json!("unknown")));
    }
}
