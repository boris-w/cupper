//! The project trust gate (design 5.2.6): what an attach may load. T044
//! asks it before every attach and prompt, after scanning again, and pauses
//! a session whose project turns `stale` while it runs.

use cupper_protocol::domain::AgentKind;
use cupper_protocol::index::TrustState;

use super::Denial;

/// What an attach does with the project's own agent configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectConfig {
    Load,
    /// User-level settings only: Claude's `settingSources: ["user"]`.
    UserOnly,
}

/// An attach (new session, resume or the next prompt) of `agent` in a
/// project in state `trust`; `stale` counts as untrusted.
pub fn attach(agent: &AgentKind, trust: &TrustState) -> Result<ProjectConfig, Denial> {
    match (trust, agent) {
        (TrustState::Trusted, _) => Ok(ProjectConfig::Load),
        (_, AgentKind::Claude) => Ok(ProjectConfig::UserOnly),
        // codex-acp cannot leave the project configuration out.
        _ => Err(Denial::ProjectUntrusted),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_policy_matrix_trust_gate() {
        let (claude, codex) = (AgentKind::Claude, AgentKind::Codex);
        assert_eq!(
            attach(&claude, &TrustState::Trusted),
            Ok(ProjectConfig::Load)
        );
        assert_eq!(
            attach(&codex, &TrustState::Trusted),
            Ok(ProjectConfig::Load)
        );
        let untrusted = [
            TrustState::Untrusted,
            TrustState::Stale,
            TrustState::Unknown("revoked".into()),
        ];
        for trust in untrusted {
            assert_eq!(
                attach(&claude, &trust),
                Ok(ProjectConfig::UserOnly),
                "{trust}"
            );
            assert_eq!(
                attach(&codex, &trust),
                Err(Denial::ProjectUntrusted),
                "{trust}"
            );
            let other = AgentKind::Unknown("gemini".into());
            assert_eq!(
                attach(&other, &trust),
                Err(Denial::ProjectUntrusted),
                "{trust}"
            );
        }
    }
}
