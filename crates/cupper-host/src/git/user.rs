//! What a failed `commit` or `push` of the user tier means (design 5.2.10,
//! "提交与推送"), read from git's own words, which `LC_ALL=C` keeps English;
//! and the end of its stderr without credentials, for the client to show.

use cupper_protocol::error::{ErrorCode, ErrorReason, RpcError};
use serde_json::json;

use super::UserOp;
use super::run::Output;
use crate::diag::redact;

/// How much of the end of git's stderr an error keeps.
const TAIL: usize = 2048;

/// The protocol's error for `op`'s failed `output`, the redacted end of its
/// stderr in `data.stderr`.
pub fn failed(op: UserOp, output: &Output) -> RpcError {
    let said =
        String::from_utf8_lossy(&[&output.stdout[..], &output.stderr[..]].concat()).into_owned();
    let reason = reason(op, &said, output.status.code());
    let start = output.stderr.len().saturating_sub(TAIL);
    let tail = String::from_utf8_lossy(&output.stderr[start..]);
    let stderr = redact::credentials(tail.trim_end()).into_owned();
    let what = match op {
        UserOp::Commit => "git commit failed",
        UserOp::Push => "git push failed",
    };
    let error = RpcError::app(ErrorCode::GitError, what);
    let error = reason.map_or(error.clone(), |reason| error.with_reason(reason));
    error.with_data(|data| drop(data.details.insert("stderr".into(), json!(stderr))))
}

/// Why `op` failed, by what git `said` and its exit `code`.
fn reason(op: UserOp, said: &str, code: Option<i32>) -> Option<ErrorReason> {
    let has = |words: &str| words.split('|').any(|word| said.contains(word));
    // gpg and x509 say the first; ssh signing goes through ssh-keygen.
    let signing = "failed to sign the data|ssh-keygen|user.signingkey";
    // With `GIT_TERMINAL_PROMPT=0`, git asks for no user name or password.
    let auth = "Authentication failed|Permission denied|terminal prompts disabled";
    Some(match op {
        _ if has("index.lock") => ErrorReason::LockHeld,
        UserOp::Commit if has("nothing to commit|added to commit") => ErrorReason::NothingToCommit,
        UserOp::Commit if has(signing) => ErrorReason::SigningFailed,
        // A hook tells why itself, and git exits 1; on its own errors, 128.
        UserOp::Commit if code == Some(1) => ErrorReason::HookFailed,
        UserOp::Push if has("rejected]|non-fast-forward|fetch first") => ErrorReason::Rejected,
        UserOp::Push if has(auth) => ErrorReason::AuthFailed,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `<op> <exit code> <reason or -> <what git said>`, `|` for a line end.
    const CASES: &str = "
        commit 1 nothing_to_commit On branch main|nothing to commit, working tree clean
        commit 128 signing_failed error: gpg failed to sign the data|fatal: failed to write commit object
        commit 128 signing_failed error: Couldn't load public key: ssh-keygen -Y sign
        commit 1 hook_failed lint: 3 problems
        commit 128 lock_held fatal: Unable to create '/r/.git/index.lock': File exists.
        commit 128 - fatal: unable to auto-detect email address
        push 1 rejected  ! [rejected]        main -> main (non-fast-forward)
        push 1 rejected  ! [remote rejected] main -> main (pre-receive hook declined)
        push 128 auth_failed git@host: Permission denied (publickey).
        push 128 auth_failed fatal: could not read Username for 'https://host': terminal prompts disabled
        push 128 - fatal: The current branch main has no upstream branch.";

    #[test]
    fn unit_git_user_failures_read_from_what_git_said() {
        for case in CASES.trim().lines() {
            let mut fields = case.trim().splitn(4, ' ');
            let mut field = || fields.next().unwrap();
            let op = if field() == "commit" {
                UserOp::Commit
            } else {
                UserOp::Push
            };
            let (code, want, said) = (field().parse().ok(), field(), field().replace('|', "\n"));
            let got = reason(op, &said, code).map_or("-".into(), |r| r.as_str().to_owned());
            assert_eq!(got, want, "{case}");
        }
    }
}
