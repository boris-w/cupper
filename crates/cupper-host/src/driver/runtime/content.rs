//! What sessions give the runtime, as ACP has it (design 5.6.2, T040): a
//! prompt's content as content blocks, and an answer to an agent's request as
//! that request's response.

use std::path::Path;

use agent_client_protocol::schema::v1::{
    ContentBlock, CreateElicitationResponse, ElicitationAcceptAction, ElicitationAction,
    ImageContent, RequestPermissionOutcome, RequestPermissionResponse, ResourceLink,
    SelectedPermissionOutcome, TextContent,
};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use cupper_protocol::domain::{PromptBlock, PromptContent, ReviewComment};
use cupper_protocol::interaction::InteractionOutcome;

use crate::driver::api::AskAnswer;
use crate::store::BlobStore;

/// `content` as ACP blocks: text as it is, a `text_blob` and an image read
/// from their blobs (the image in base64), a file reference as a
/// `resource_link` under `cwd`, a review comment laid out as text (GT-2).
/// Blocks of a type this host does not know are left out.
pub async fn blocks(
    content: &PromptContent,
    cwd: &Path,
    blobs: &BlobStore,
) -> Result<Vec<ContentBlock>, String> {
    let mut out = Vec::new();
    for block in content {
        let read = |hash| async move { blobs.get(hash).await.map_err(|e| e.to_string()) };
        out.push(match block {
            PromptBlock::Text(text) => ContentBlock::Text(TextContent::new(&text.text)),
            PromptBlock::TextBlob(text) => {
                let bytes = read(&text.blob.hash).await?;
                ContentBlock::Text(TextContent::new(String::from_utf8_lossy(&bytes)))
            }
            PromptBlock::Image(image) => {
                let data = STANDARD.encode(read(&image.blob.hash).await?);
                ContentBlock::Image(ImageContent::new(data, &image.blob.media_type))
            }
            PromptBlock::FileRef(file) => {
                let path = cwd.join(&file.path.0);
                let name = file
                    .path
                    .0
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                let uri = format!("file://{}", path.display());
                ContentBlock::ResourceLink(ResourceLink::new(name, uri))
            }
            PromptBlock::ReviewComment(comment) => {
                ContentBlock::Text(TextContent::new(review(comment)))
            }
            PromptBlock::Unknown(_) => continue,
        });
    }
    Ok(out)
}

/// A review comment as the agent reads it: where, the quoted lines, then the
/// comment.
fn review(c: &ReviewComment) -> String {
    let quote: Vec<String> = c.quote.lines().map(|line| format!("> {line}")).collect();
    let (path, start, end, side) = (&c.path.0, c.line_start, c.line_end, &c.side);
    let quote = quote.join("\n");
    format!(
        "Review comment on {path}, lines {start}-{end} ({side}):\n{quote}\n{}",
        c.comment
    )
}

/// A permission's answer: the option chosen, or `cancelled`.
pub fn permission(answer: AskAnswer) -> RequestPermissionResponse {
    let outcome = match answer {
        AskAnswer::Outcome(InteractionOutcome::Selected(selected)) => {
            let selected = SelectedPermissionOutcome::new(selected.option_id);
            RequestPermissionOutcome::Selected(selected)
        }
        _ => RequestPermissionOutcome::Cancelled,
    };
    RequestPermissionResponse::new(outcome)
}

/// A form's answer: its fields accepted, `decline` for `dismissed`, `cancel`
/// otherwise (design 5.6.4).
pub fn elicitation(answer: AskAnswer) -> CreateElicitationResponse {
    let action = match answer {
        AskAnswer::Outcome(InteractionOutcome::Answer(answer)) => {
            let content = serde_json::to_value(answer.values).ok();
            let content = content.and_then(|values| serde_json::from_value(values).ok());
            let mut accept = ElicitationAcceptAction::new();
            accept.content = content;
            ElicitationAction::Accept(accept)
        }
        AskAnswer::Outcome(InteractionOutcome::Dismissed) => ElicitationAction::Decline,
        _ => ElicitationAction::Cancel,
    };
    CreateElicitationResponse::new(action)
}
