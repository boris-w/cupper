//! Claude's requests (design 5.6.4): a permission request is a plan
//! confirmation when ExitPlanMode asks (`switch_mode` with `exit-plan-`
//! options), a permission otherwise. A form elicitation goes to the
//! interaction broker as it came ([`crate::driver::extract::elicitation`]).
//!
//! Options take their effects from the option table ([`options`]).

use agent_client_protocol::schema::v1::RequestPermissionRequest;
use cupper_protocol::interaction::{
    InteractionRequest, PermissionRequest, PermissionTool, PlanConfirmRequest,
};
use serde_json::Value;

use super::ClaudeSession;
use super::options::{self, Scene};
use crate::driver::api::AskRequest;
use crate::driver::extract::{command, content_text, tool_kind};

impl ClaudeSession {
    /// `session/request_permission`: the tool call it is about, and the request.
    pub fn permission(&self, request: &RequestPermissionRequest) -> (String, AskRequest) {
        let (call, fields) = (&request.tool_call, &request.tool_call.fields);
        let raw_input = fields.raw_input.as_ref();
        let (scene, command) = (Scene::of(request), command(raw_input));
        let options = (request.options.iter())
            .map(|option| options::option(scene, option, command.as_deref()))
            .collect();
        let request = if scene == Scene::PlanConfirm {
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
                    command: command.map(|command| content_text(&command)),
                    paths: self.0.paths(locations),
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
