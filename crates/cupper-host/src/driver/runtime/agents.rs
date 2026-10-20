//! `agents/capabilities` and `agents/refresh` (design 5.6.6, 5.6.7): the
//! capability cache for the new-session page, and the settings page's
//! "check again", which finds the CLI, checks its version and, while the
//! cache is empty, probes the agent. Only Claude has a runtime in M1a.

use std::fmt;
use std::sync::Arc;

use cupper_protocol::domain::AgentKind;
use cupper_protocol::error::{ErrorCode, INVALID_PARAMS, RpcError};
use cupper_protocol::methods::project::{
    AgentsCapabilities, AgentsCapabilitiesParams, AgentsCapabilitiesResult, AgentsRefresh,
    AgentsRefreshResult,
};
use cupper_protocol::rpc::Method;
use futures::FutureExt;
use futures::future::BoxFuture;

use super::launch::Launch;
use super::{AgentRuntime, cache};
use crate::gateway::{Call, Port, Reply, Routes};
use crate::store::{Readers, Writer};

/// Finds the agent's CLI and its launch again: the login shell, the locator
/// and the version check, as at startup.
pub type Find = Arc<dyn Fn() -> BoxFuture<'static, Launch> + Send + Sync>;

#[derive(Clone)]
pub struct Agents {
    claude: AgentRuntime,
    readers: Readers,
    writer: Writer,
    find: Find,
}

impl fmt::Debug for Agents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Agents")
            .field("claude", &self.claude)
            .finish_non_exhaustive()
    }
}

fn unavailable(message: &str) -> RpcError {
    RpcError::app(ErrorCode::Unavailable, message)
}

fn internal(err: impl fmt::Display) -> RpcError {
    RpcError::app(ErrorCode::Internal, err.to_string())
}

impl Agents {
    pub fn new(claude: AgentRuntime, readers: Readers, writer: Writer, find: Find) -> Self {
        Self {
            claude,
            readers,
            writer,
            find,
        }
    }

    pub fn route(self: &Arc<Self>, routes: &mut Routes) {
        routes.add::<AgentsCapabilities>(Arc::clone(self) as Arc<dyn Port>);
        routes.add::<AgentsRefresh>(Arc::clone(self) as Arc<dyn Port>);
    }

    fn check(agent: &AgentKind) -> Result<(), RpcError> {
        match agent {
            AgentKind::Claude => Ok(()),
            _ => Err(unavailable("only Claude runs in this version")),
        }
    }

    pub async fn capabilities(
        &self,
        agent: &AgentKind,
    ) -> Result<AgentsCapabilitiesResult, RpcError> {
        Self::check(agent)?;
        let cached = cache::read(&self.readers, agent).await.map_err(internal)?;
        Ok(AgentsCapabilitiesResult { cached })
    }

    /// The next adapter starts with what is found now; a cache of other
    /// versions goes, and an empty one is filled by a probe.
    pub async fn refresh(&self, agent: &AgentKind) -> Result<AgentsRefreshResult, RpcError> {
        Self::check(agent)?;
        let launch = (self.find)().await;
        let invalidated = cache::invalidate(&self.writer, agent, &launch.versions);
        invalidated.await.map_err(internal)?;
        let available = launch.unavailable.is_none();
        self.claude.relaunch(launch);
        let cached = cache::read(&self.readers, agent).await.map_err(internal)?;
        if available && cached.is_none() {
            // A failure shows in the status; the cache stays empty.
            let _ = self.claude.probe().await;
        }
        let status = self.claude.status().await;
        let status = status.ok_or_else(|| unavailable("the agent's runtime stopped"))?;
        Ok(AgentsRefreshResult { status })
    }
}

impl Port for Agents {
    fn deliver(&self, call: Call) -> Reply {
        let agents = self.clone();
        async move {
            let params = serde_json::from_value::<AgentsCapabilitiesParams>(call.params);
            let agent = params
                .map_err(|err| RpcError::new(INVALID_PARAMS, err.to_string()))?
                .agent;
            let result = match call.method {
                AgentsRefresh::NAME => serde_json::to_value(agents.refresh(&agent).await?),
                _ => serde_json::to_value(agents.capabilities(&agent).await?),
            };
            result.map_err(internal)
        }
        .boxed()
    }
}
