//! What differs between the agents a runtime drives (ADR-07, ADR-09): where
//! the CLI is found and what its version is checked against (design 5.6.6),
//! and how its adapter's frames, errors and capabilities are read (design
//! 5.6.3, 5.6.7). The runtime itself is the same for every agent.

use std::path::{Path, PathBuf};

use agent_client_protocol::Error;
use agent_client_protocol::schema::v1::{
    InitializeResponse, PromptResponse, RequestPermissionRequest, SessionConfigOption,
    SessionUpdate,
};
use cupper_protocol::capabilities::{ConfigOption, SessionCapabilities, SlashCommand, Versions};
use cupper_protocol::domain::AgentKind;
use cupper_protocol::methods::admin::Settings;
use cupper_protocol::session::{StopReason, TurnUsage};

use crate::driver::api::{AgentError, AskRequest, DriverEvent};
use crate::driver::claude::{self, ClaudeSession};
use crate::driver::codex::{self, CodexSession};
use crate::driver::extract;
use crate::driver::locate::env_rules::{self, AgentVars};
use crate::driver::locate::resolve::{self, Layout};
use crate::manifest::{self, CliRange};

/// An agent with a runtime of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub fn kind(self) -> AgentKind {
        match self {
            Self::Claude => AgentKind::Claude,
            Self::Codex => AgentKind::Codex,
        }
    }

    /// The variables the host sets for its adapter.
    pub fn vars(self) -> AgentVars {
        match self {
            Self::Claude => env_rules::CLAUDE,
            Self::Codex => env_rules::CODEX,
        }
    }

    /// Where its CLI gets installed.
    pub fn layout(self) -> &'static Layout {
        match self {
            Self::Claude => &resolve::CLAUDE,
            Self::Codex => &resolve::CODEX,
        }
    }

    /// `agents.<agent>.cli_path`.
    pub fn cli_path(self, settings: &Settings) -> Option<&Path> {
        let path = match self {
            Self::Claude => &settings.claude_cli_path,
            Self::Codex => &settings.codex_cli_path,
        };
        path.0.as_ref().map(|path| Path::new(&path.0))
    }

    /// The CLI versions its pinned adapter was verified with.
    pub fn range(self) -> &'static CliRange {
        let cli = &manifest::embedded().cli;
        match self {
            Self::Claude => &cli.claude,
            Self::Codex => &cli.codex,
        }
    }

    /// Its pinned adapter's version.
    pub fn adapter_version(self) -> &'static str {
        let manifest = manifest::embedded();
        match self {
            Self::Claude => &manifest.claude_agent_acp.version,
            Self::Codex => &manifest.codex_acp.version,
        }
    }

    /// A JSON-RPC error of its adapter, classified.
    pub fn error(self, error: &Error) -> AgentError {
        match self {
            Self::Claude => claude::agent_error(error),
            Self::Codex => codex::agent_error(error),
        }
    }

    /// Its session's options as cupper's.
    pub fn config_options(self, options: &[SessionConfigOption]) -> Vec<ConfigOption> {
        match self {
            Self::Claude => claude::capabilities::config_options(options),
            Self::Codex => codex::capabilities::config_options(options),
        }
    }

    /// What a session can do on an adapter that answered `initialize` with
    /// `init`.
    pub fn capabilities(
        self,
        init: &InitializeResponse,
        versions: Versions,
        config: Vec<ConfigOption>,
        commands: Vec<SlashCommand>,
    ) -> SessionCapabilities {
        match self {
            Self::Claude => claude::capabilities::capabilities(init, versions, config, commands),
            Self::Codex => codex::capabilities::capabilities(init, versions, config, commands),
        }
    }
}

/// A session's extraction layer.
#[derive(Debug)]
pub enum Extractor {
    Claude(ClaudeSession),
    Codex(CodexSession),
}

impl Extractor {
    /// `cwd`: the workspace the session was opened in; `config`: its options.
    pub fn new(agent: Agent, cwd: PathBuf, config: Vec<ConfigOption>) -> Self {
        match agent {
            Agent::Claude => Self::Claude(ClaudeSession::new(cwd, config)),
            Agent::Codex => Self::Codex(CodexSession::new(cwd, config)),
        }
    }

    pub fn update(&mut self, update: &SessionUpdate) -> Vec<DriverEvent> {
        match self {
            Self::Claude(session) => session.update(update),
            Self::Codex(session) => session.update(update),
        }
    }

    pub fn permission(&self, request: &RequestPermissionRequest) -> (String, AskRequest) {
        match self {
            Self::Claude(session) => session.permission(request),
            Self::Codex(session) => session.permission(request),
        }
    }

    pub fn set_config(&mut self, options: &[SessionConfigOption]) -> DriverEvent {
        match self {
            Self::Claude(session) => session.set_config(options),
            Self::Codex(session) => session.set_config(options),
        }
    }

    pub fn config(&self) -> &[ConfigOption] {
        match self {
            Self::Claude(session) => session.config(),
            Self::Codex(session) => session.config(),
        }
    }

    /// A prompt's result; Codex fails one whose thread failed (K17).
    pub fn prompt_done(
        &mut self,
        response: &PromptResponse,
    ) -> Result<(StopReason, Option<TurnUsage>), AgentError> {
        match self {
            Self::Claude(_) => Ok(extract::prompt_done(response)),
            Self::Codex(session) => session.prompt_done(response),
        }
    }
}
