//! Claude's requests (design 5.6.4): a permission request is a plan
//! confirmation when ExitPlanMode asks (`switch_mode` with `exit-plan-`
//! options), a permission otherwise; a form elicitation goes to the
//! interaction broker as it came (design 5.3.6).
//!
//! Options take their effect from their ACP kind alone, after the two
//! fallback rows of the option table in design 5.3.6: conservative, since
//! every `allow_always` counts as persistent and privileged. An unknown kind
//! (the tap's original, K19) is passed on and counts as the strictest
//! (security #5). The table by tool and option replaces this in T095.

use agent_client_protocol::schema::v1::{
    CreateElicitationRequest, ElicitationMode, ElicitationScope, PermissionOption,
    RequestPermissionRequest, ToolKind as AcpToolKind,
};
use cupper_protocol::interaction::{
    EffectScope, InteractionOption, InteractionRequest, OptionEffect, OptionKind,
    PermissionRequest, PermissionTool, PlanConfirmRequest,
};
use cupper_protocol::rpc::Scope;
use serde_json::Value;

use super::{ClaudeSession, command, content_text, tool_kind, wire};
use crate::driver::api::{AskRequest, FormAsk};
use crate::driver::tap::k19;

impl ClaudeSession {
    /// `session/request_permission`: the tool call it is about, and the request.
    pub fn permission(&self, request: &RequestPermissionRequest) -> (String, AskRequest) {
        let (call, fields) = (&request.tool_call, &request.tool_call.fields);
        let options = request.options.iter().map(option).collect();
        let exits_plan = (request.options.iter())
            .any(|option| option.option_id.to_string().starts_with("exit-plan-"));
        let raw_input = fields.raw_input.as_ref();
        let request = if fields.kind == Some(AcpToolKind::SwitchMode) && exits_plan {
            let plan = raw_input
                .and_then(|raw| raw.get("plan"))
                .and_then(Value::as_str);
            let plan = plan.or(self.last_plan()).unwrap_or_default();
            InteractionRequest::PlanConfirm(PlanConfirmRequest {
                plan: content_text(plan),
                options,
            })
        } else {
            let locations = fields.locations.as_deref().unwrap_or_default();
            InteractionRequest::Permission(PermissionRequest {
                title: fields.title.clone().unwrap_or_default(),
                tool: PermissionTool {
                    item_id: None,
                    tool_kind: tool_kind(fields.kind.unwrap_or_default()),
                    command: command(raw_input).map(|command| content_text(&command)),
                    paths: self.paths(locations),
                },
                options,
                detail: None,
                content_truncated: false,
            })
        };
        (
            call.tool_call_id.to_string(),
            AskRequest::Classified(request),
        )
    }
}

/// `elicitation/create` in form mode: the tool call it is about, and the form.
/// The URL mode is not in M1 (design 5.6.4).
pub fn elicitation(request: &CreateElicitationRequest) -> Option<(Option<String>, AskRequest)> {
    let ElicitationMode::Form(form) = &request.mode else {
        return None;
    };
    let tool_call_id = match &form.scope {
        ElicitationScope::Session(scope) => scope.tool_call_id.as_ref().map(ToString::to_string),
        _ => None,
    };
    let form = FormAsk {
        message: request.message.clone(),
        schema: serde_json::to_value(&form.requested_schema).ok()?,
    };
    Some((tool_call_id, AskRequest::Form(form)))
}

fn option(option: &PermissionOption) -> InteractionOption {
    let original = k19::original(option.meta.as_ref(), "kind").and_then(Value::as_str);
    let kind = OptionKind::from(original.unwrap_or(&wire(&option.kind)));
    use {EffectScope as S, OptionKind as K, Scope as R};
    let (scope, persistent, ends_turn, requires) = match &kind {
        K::AllowOnce => (S::Once, false, Some(false), R::Approve),
        K::AllowAlways => (S::Persistent, true, Some(false), R::Privileged),
        K::RejectOnce => (S::Reject, false, Some(false), R::Approve),
        K::RejectAlways => (S::Reject, true, Some(false), R::Approve),
        K::Unknown(_) => (S::Unclassified, true, None, R::Privileged),
    };
    InteractionOption {
        option_id: option.option_id.to_string(),
        name: option.name.clone(),
        kind,
        effect: OptionEffect {
            scope,
            persistent,
            ends_turn,
            mode_after: None,
            tier_after: None,
            rule: None,
        },
        requires,
    }
}
