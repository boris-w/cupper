//! The `error` item (design 5.3.5, MS-5): a failed turn gets one, before
//! its end is settled. It names the failure, carries the agent's code and its
//! redacted words, and says what the user can do; T143 shows it.

use cupper_protocol::domain::FailureCode;
use cupper_protocol::item::{ErrorItem, Guidance, Item};

use crate::driver::api::{AgentError, AgentErrorForm};

/// What an `error` item keeps of an exited adapter's stderr: its last
/// characters, from the start of a line.
const STDERR_CHARS: usize = 2_000;

/// Why a turn failed, as its `error` item tells it.
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    pub code: FailureCode,
    pub agent_code: Option<String>,
    /// Truncated and redacted.
    pub raw: Option<String>,
}

impl Failure {
    /// An agent error, its message redacted by the session (design 5.11.2).
    pub fn of(error: &AgentError) -> Self {
        let agent_code = match error.form {
            AgentErrorForm::Rpc { agent_code } => Some(agent_code.to_string()),
            AgentErrorForm::Text | AgentErrorForm::Upstream => None,
        };
        let raw = Some(error.message.clone()).filter(|message| !message.is_empty());
        let code = error.class.failure_code();
        Self {
            code,
            agent_code,
            raw,
        }
    }

    /// The adapter exited under the turn: the end of its stderr, which was
    /// redacted as it was read (design 5.11.1).
    pub fn exited(stderr: &str) -> Self {
        let stderr = stderr.trim_end();
        let cut = stderr.char_indices().rev().nth(STDERR_CHARS - 1);
        let tail = cut.map_or(stderr, |(at, _)| {
            let (before, tail) = stderr.split_at(at);
            let whole = before.ends_with('\n');
            let rest = tail.split_once('\n').filter(|_| !whole);
            rest.map_or(tail, |(_, rest)| rest)
        });
        Self {
            code: FailureCode::AdapterExited,
            agent_code: None,
            raw: Some(tail.to_owned()).filter(|tail| !tail.is_empty()),
        }
    }

    pub fn item(self) -> Item {
        let (guidance, retryable) = guidance(&self.code);
        Item::Error(ErrorItem {
            code: self.code,
            agent_code: self.agent_code,
            retryable,
            guidance,
            raw: self.raw,
        })
    }
}

/// What the user can do about a failure, and whether sending the message
/// again may do by itself (`retryable`).
pub fn guidance(code: &FailureCode) -> (Guidance, bool) {
    use FailureCode as F;
    let guidance = match code {
        F::NotLoggedIn => Guidance::RunLoginInTerminal,
        F::CliTooOld => Guidance::UpgradeCli,
        F::CliMissing => Guidance::InstallCli,
        F::QuotaExhausted => Guidance::QuotaExhausted,
        F::ModelUnsupported => Guidance::ChangeModel,
        // Restarted too often, or no runtime: the agent's restart in settings.
        F::AdapterCrashed | F::RuntimeFailed => Guidance::RestartAgent,
        // The next prompt resumes the session, or restarts what died.
        F::SessionNotFound
        | F::ProcessExited
        | F::Restartable
        | F::AdapterExited
        | F::HostRestarted => Guidance::Retry,
        // Trusting the project is the trust flow's (design 5.2.6).
        F::ProjectUntrusted | F::Other | F::Unknown(_) => Guidance::None,
    };
    let retryable = guidance == Guidance::Retry;
    (guidance, retryable)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every known code in `KNOWN`'s order, then one this version does not know.
    const TABLE: &str = "not_logged_in run_login_in_terminal, cli_too_old upgrade_cli, \
        session_not_found retry, process_exited retry, restartable retry, \
        model_unsupported change_model, quota_exhausted quota_exhausted, other none, \
        adapter_exited retry, adapter_crashed restart_agent, cli_missing install_cli, \
        runtime_failed restart_agent, host_restarted retry, project_untrusted none, \
        newer_code none";

    #[test]
    fn it_error_item_guidance_of_every_failure_code() {
        let table: Vec<(&str, &str)> = TABLE
            .split(", ")
            .filter_map(|r| r.split_once(' '))
            .collect();
        let known: Vec<&str> = table.iter().map(|(code, _)| *code).collect();
        assert_eq!(known[..14], *FailureCode::KNOWN);
        for (code, expected) in table {
            let (guidance, retryable) = guidance(&FailureCode::from(code));
            assert_eq!(
                (guidance.as_str(), retryable),
                (expected, expected == "retry")
            );
        }
    }

    #[test]
    fn it_error_item_keeps_the_end_of_stderr() {
        // Whole lines of the end, unless one line is longer than the tail.
        let lines: String = (0..300).map(|n| format!("line {n:03}\n")).collect();
        let tail = Failure::exited(&lines).raw.unwrap();
        assert!(
            tail.starts_with("line 078\n") && tail.ends_with("line 299"),
            "{tail}"
        );
        let long = "x".repeat(STDERR_CHARS + 5);
        assert_eq!(Failure::exited(&long).raw.unwrap().len(), STDERR_CHARS);
        assert_eq!(Failure::exited("\n").raw, None);
    }
}
