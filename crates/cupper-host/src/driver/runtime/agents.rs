//! `agents/capabilities`, `agents/refresh` and `agents/restart` (design
//! 5.6.6–5.6.8): the capability cache for the new-session page, the settings
//! page's "check again", which finds the CLI, checks its version and, while
//! the cache is empty, probes the agent, and its restart.

use std::fmt;
use std::sync::Arc;

use cupper_protocol::domain::AgentKind;
use cupper_protocol::error::{ErrorCode, INVALID_PARAMS, RpcError};
use cupper_protocol::methods::project::{
    AgentsCapabilities, AgentsCapabilitiesParams, AgentsCapabilitiesResult, AgentsRefresh,
    AgentsRefreshResult, AgentsRestart, AgentsRestartParams,
};
use cupper_protocol::rpc::Method;
use futures::FutureExt;
use futures::future::BoxFuture;
use serde_json::json;

use super::launch::{Agent, Launch};
use super::{AgentRuntime, cache};
use crate::gateway::{Call, Port, Reply, Routes};
use crate::store::{Readers, Writer};

/// Finds an agent's CLI and its launch again: the login shell, the locator
/// and the version check, as at startup.
pub type Find = Arc<dyn Fn(Agent) -> BoxFuture<'static, Launch> + Send + Sync>;

#[derive(Clone)]
pub struct Agents {
    runtimes: Vec<(Agent, AgentRuntime)>,
    readers: Readers,
    writer: Writer,
    find: Find,
}

impl fmt::Debug for Agents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Agents")
            .field("runtimes", &self.runtimes)
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
    pub fn new(
        runtimes: Vec<(Agent, AgentRuntime)>,
        readers: Readers,
        writer: Writer,
        find: Find,
    ) -> Self {
        Self {
            runtimes,
            readers,
            writer,
            find,
        }
    }

    pub fn route(self: &Arc<Self>, routes: &mut Routes) {
        routes.add::<AgentsCapabilities>(Arc::clone(self) as Arc<dyn Port>);
        routes.add::<AgentsRefresh>(Arc::clone(self) as Arc<dyn Port>);
        routes.add::<AgentsRestart>(Arc::clone(self) as Arc<dyn Port>);
    }

    fn runtime(&self, agent: &AgentKind) -> Result<(Agent, &AgentRuntime), RpcError> {
        let found = self
            .runtimes
            .iter()
            .find(|(known, _)| known.kind() == *agent);
        let found = found.map(|(known, runtime)| (*known, runtime));
        found.ok_or_else(|| unavailable("the agent has no runtime"))
    }

    pub async fn capabilities(
        &self,
        agent: &AgentKind,
    ) -> Result<AgentsCapabilitiesResult, RpcError> {
        self.runtime(agent)?;
        let cached = cache::read(&self.readers, agent).await.map_err(internal)?;
        Ok(AgentsCapabilitiesResult { cached })
    }

    /// The next adapter starts with what is found now; a cache of other
    /// versions goes, and an empty one is filled by a probe.
    pub async fn refresh(&self, agent: &AgentKind) -> Result<AgentsRefreshResult, RpcError> {
        let (known, runtime) = self.runtime(agent)?;
        let launch = (self.find)(known).await;
        let invalidated = cache::invalidate(&self.writer, agent, &launch.versions);
        invalidated.await.map_err(internal)?;
        let available = launch.unavailable.is_none();
        runtime.relaunch(launch);
        let cached = cache::read(&self.readers, agent).await.map_err(internal)?;
        if available && cached.is_none() {
            // A failure shows in the status; the cache stays empty.
            let _ = runtime.probe().await;
        }
        let status = runtime.status().await;
        let status = status.ok_or_else(|| unavailable("the agent's runtime stopped"))?;
        Ok(AgentsRefreshResult { status })
    }
}

impl Port for Agents {
    fn deliver(&self, call: Call) -> Reply {
        let agents = self.clone();
        async move {
            if call.method == AgentsRestart::NAME {
                let ticket = call.ticket.expect("agents/restart is a command");
                let params = serde_json::from_value::<AgentsRestartParams>(call.params);
                let params = params.map_err(|err| RpcError::new(INVALID_PARAMS, err.to_string()));
                let found = params.and_then(|p| Ok(agents.runtime(&p.agent)?.1.clone()));
                let runtime = match found {
                    Ok(runtime) => runtime,
                    Err(err) => {
                        ticket.reject(err.clone());
                        return Err(err);
                    }
                };
                ticket.complete(Ok(json!({}))).commit().await?;
                runtime.restart();
                return Ok(json!({}));
            }
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
