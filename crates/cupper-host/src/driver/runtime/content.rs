//! What sessions give the runtime, as ACP has it (design 5.6.2, T040): a
//! prompt's content as content blocks, and an answer to an agent's request as
//! that request's response.

use std::path::{Component, Path};

use agent_client_protocol::schema::v1::{
    ContentBlock, CreateElicitationResponse, ElicitationAcceptAction, ElicitationAction,
    ImageContent, RequestPermissionOutcome, RequestPermissionResponse, ResourceLink,
    SelectedPermissionOutcome, TextContent,
};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use cupper_protocol::capabilities::PromptCapabilities;
use cupper_protocol::domain::{PromptBlock, PromptContent, ReviewComment};
use cupper_protocol::interaction::InteractionOutcome;

use crate::driver::api::AskAnswer;
use crate::store::BlobStore;

/// `content` as ACP blocks: text as it is, a `text_blob` and an image read
/// from their blobs (the image in base64), a file reference as a
/// `resource_link` under `cwd`, a review comment laid out as text (GT-2).
/// Blocks of a type this host does not know are left out. An image the
/// agent does not take, or one over its limit, and a reference out of `cwd`
/// fail the prompt, saying why.
pub async fn blocks(
    content: &PromptContent,
    cwd: &Path,
    prompt: &PromptCapabilities,
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
            PromptBlock::Image(_) if !prompt.image => {
                return Err("the agent takes no images".into());
            }
            PromptBlock::Image(image) => {
                let data = read(&image.blob.hash).await?;
                let max = prompt.max_image_bytes;
                if data.len() > max as usize {
                    return Err(format!("an image is over the agent's {max} bytes"));
                }
                let data = STANDARD.encode(data);
                ContentBlock::Image(ImageContent::new(data, &image.blob.media_type))
            }
            PromptBlock::FileRef(file) => {
                let relative = Path::new(&file.path.0);
                let normal =
                    |c: Component<'_>| matches!(c, Component::Normal(_) | Component::CurDir);
                if file.path.0.is_empty() || !relative.components().all(normal) {
                    return Err(format!("{} is not in the workspace", file.path.0));
                }
                let path = cwd.join(relative);
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{Value, json};

    use super::*;
    use crate::clock::ManualClock;
    use crate::store::testutil::{TempDir, host_writer};

    fn caps(image: bool, max_image_bytes: u32) -> PromptCapabilities {
        PromptCapabilities {
            image,
            file_ref: true,
            max_image_bytes,
        }
    }

    #[tokio::test]
    async fn unit_prompt_mapping_each_block_as_acp_has_it() {
        let (dir, clock) = (TempDir::new("prompt-mapping"), ManualClock::new());
        let (writer, _thread) = host_writer(&dir, &clock);
        let blobs = BlobStore::new(dir.path(), writer, Arc::new(clock));
        let put = |bytes: &[u8], media: &str| blobs.put(bytes.to_vec(), media.into(), None);
        let long = json!(put(b"long text", "text/plain").await.unwrap());
        let image = json!(put(b"png", "image/png").await.unwrap());
        let content = |blocks: Value| serde_json::from_value::<PromptContent>(blocks).unwrap();
        let all = content(json!([
            {"type": "text", "text": "hi"},
            {"type": "text_blob", "blob": long},
            {"type": "image", "blob": image},
            {"type": "file_ref", "path": "src/a.rs"},
            {"type": "review_comment", "path": "src/a.rs", "line_start": 2, "line_end": 3,
             "side": "new", "quote": "x\ny", "comment": "fix it"},
            {"type": "video", "blob": image},
        ]));
        let cwd = Path::new("/ws");
        let mapped = blocks(&all, cwd, &caps(true, 3), &blobs).await.unwrap();
        let review = "Review comment on src/a.rs, lines 2-3 (new):\n> x\n> y\nfix it";
        let expected = json!([
            {"type": "text", "text": "hi"},
            {"type": "text", "text": "long text"},
            {"type": "image", "data": "cG5n", "mimeType": "image/png"},
            {"type": "resource_link", "name": "a.rs", "uri": "file:///ws/src/a.rs"},
            {"type": "text", "text": review},
        ]);
        assert_eq!(json!(mapped), expected, "the unknown block left out");

        // What the agent cannot take fails the prompt.
        let refused = [
            (
                json!([{"type": "image", "blob": image}]),
                caps(false, 3),
                "takes no images",
            ),
            (
                json!([{"type": "image", "blob": image}]),
                caps(true, 2),
                "over the agent's 2 bytes",
            ),
            (
                json!([{"type": "file_ref", "path": "../x"}]),
                caps(true, 3),
                "not in the workspace",
            ),
            (
                json!([{"type": "file_ref", "path": "/etc/hosts"}]),
                caps(true, 3),
                "not in",
            ),
        ];
        for (blocks_of, prompt, why) in refused {
            let error = blocks(&content(blocks_of), cwd, &prompt, &blobs)
                .await
                .unwrap_err();
            assert!(error.contains(why), "{error}");
        }
    }
}
