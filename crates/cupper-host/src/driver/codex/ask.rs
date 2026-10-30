//! Codex's requests (design 5.6.4): a permission request is a plan
//! confirmation when its tool call is a `plan-review:` (K12: the thread is
//! already idle by then), a permission otherwise. A form elicitation goes to
//! the interaction broker as it came ([`crate::driver::extract::elicitation`]):
//! the `<id>_note` beside each question with `isOther` stays a field of its
//! own, since Codex takes a note with any choice.

use agent_client_protocol::schema::v1::RequestPermissionRequest;
use cupper_protocol::interaction::{
    InteractionRequest, PermissionRequest, PermissionTool, PlanConfirmRequest,
};
use serde_json::Value;

use super::CodexSession;
use super::options::{self, Scene};
use crate::driver::api::AskRequest;
use crate::driver::extract::{command, content_text, tool_kind};

impl CodexSession {
    /// `session/request_permission`: the tool call it is about, and the request.
    pub fn permission(&self, request: &RequestPermissionRequest) -> (String, AskRequest) {
        let (call, fields) = (&request.tool_call, &request.tool_call.fields);
        let raw_input = fields.raw_input.as_ref();
        let (scene, command) = (Scene::of(request), command(raw_input));
        let options = (request.options.iter())
            .map(|option| options::option(scene, option, command.as_deref()))
            .collect();
        let request = match scene {
            Scene::PlanConfirm => {
                let plan = raw_input.and_then(|raw| raw.get("plan"));
                let plan = (plan.and_then(Value::as_str)).or(self.frames.last_plan());
                InteractionRequest::PlanConfirm(PlanConfirmRequest {
                    plan: content_text(plan.unwrap_or_default()),
                    options,
                })
            }
            _ => InteractionRequest::Permission(PermissionRequest {
                title: fields.title.clone().unwrap_or_default(),
                tool: PermissionTool {
                    item_id: None,
                    tool_kind: tool_kind(fields.kind.unwrap_or_default()),
                    command: command.map(|command| content_text(&command)),
                    paths: self
                        .frames
                        .paths(fields.locations.as_deref().unwrap_or_default()),
                },
                options,
                detail: None,
                content_truncated: false,
            }),
        };
        let tool_call_id = call.tool_call_id.to_string();
        (tool_call_id, AskRequest::Classified(request))
    }
}
