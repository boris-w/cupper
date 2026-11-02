//! What the agents' option tables share (design 5.3.6, 5.6.4): an option the
//! table does not have falls back on its ACP kind (5.3.6's last two rows):
//! `allow_always` is persistent and privileged; an unknown kind, the tap's
//! original (K19), is the strictest (security #5). A persistent option names
//! the command prefix it would allow, and how far that reaches.

use agent_client_protocol::schema::v1::PermissionOption;
use cupper_protocol::interaction::{
    EffectRule, EffectScope, InteractionOption, OptionEffect, OptionKind, PrefixRisk,
};
use cupper_protocol::rpc::Scope;
use serde_json::Value;

use super::extract::wire;
use super::tap::k19;

/// An option's effect and the scope choosing it needs, as a table has it.
pub type Row = (OptionEffect, Scope);

/// The option as the protocol carries it: its effect from the agent's table
/// (`row`, given the option's ID and kind), else from its kind; a reject of
/// an agent whose rejects end the turn (Codex #556) says so.
pub fn option(
    option: &PermissionOption,
    row: impl FnOnce(&str, &OptionKind) -> Option<Row>,
    reject_ends_turn: bool,
) -> InteractionOption {
    let (option_id, kind) = (option.option_id.to_string(), kind(option));
    let (effect, requires) =
        row(&option_id, &kind).unwrap_or_else(|| by_kind(&kind, reject_ends_turn));
    InteractionOption {
        option_id,
        name: option.name.clone(),
        kind,
        effect,
        requires,
    }
}

/// The agent's kind: the tap's original when it rewrote one (K19).
pub fn kind(option: &PermissionOption) -> OptionKind {
    let original = k19::original(option.meta.as_ref(), "kind").and_then(Value::as_str);
    OptionKind::from(original.unwrap_or(&wire(&option.kind)))
}

/// An effect that does not end the turn and switches no mode.
pub fn effect(scope: EffectScope, persistent: bool) -> OptionEffect {
    OptionEffect {
        scope,
        persistent,
        ends_turn: Some(false),
        mode_after: None,
        tier_after: None,
        rule: None,
    }
}

/// 5.3.6's fallback for options the table does not have.
fn by_kind(kind: &OptionKind, reject_ends_turn: bool) -> Row {
    use {EffectScope as S, OptionKind as K, Scope::*};
    let reject = |persistent| OptionEffect {
        ends_turn: Some(reject_ends_turn),
        ..effect(S::Reject, persistent)
    };
    match kind {
        K::AllowOnce => (effect(S::Once, false), Approve),
        K::AllowAlways => (effect(S::Persistent, true), Privileged),
        K::RejectOnce => (reject(false), Approve),
        K::RejectAlways => (reject(true), Approve),
        K::Unknown(_) => {
            let ends_turn = None;
            (
                OptionEffect {
                    ends_turn,
                    ..effect(S::Unclassified, true)
                },
                Privileged,
            )
        }
    }
}

/// The rule of a persistent option that allows `prefix`: how far it reaches
/// by the prefix's first word.
pub fn rule(prefix: Option<String>) -> EffectRule {
    let prefix_risk = match prefix.as_deref().and_then(|p| p.split_whitespace().next()) {
        Some(word) if interpreter(word) => PrefixRisk::Interpreter,
        Some(_) => PrefixRisk::Normal,
        None => PrefixRisk::Unparsed,
    };
    let (text, location) = (None, None);
    EffectRule {
        text,
        location,
        prefix,
        prefix_risk,
    }
}

/// A program that runs whatever follows it: allowing it allows anything.
fn interpreter(word: &str) -> bool {
    const INTERPRETERS: &[&str] = &[
        "bash",
        "sh",
        "zsh",
        "fish",
        "dash",
        "ksh",
        "csh",
        "node",
        "deno",
        "bun",
        "ruby",
        "perl",
        "php",
        "lua",
        "env",
        "sudo",
        "xargs",
        "exec",
        "eval",
        "nohup",
        "nice",
        "time",
        "timeout",
        "npx",
        "uvx",
        "osascript",
    ];
    let program = word.rsplit('/').next().unwrap_or(word);
    INTERPRETERS.contains(&program) || program.starts_with("python")
}
