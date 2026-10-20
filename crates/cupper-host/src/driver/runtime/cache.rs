//! The capability cache (design 5.6.7): one `agent_capabilities` row per
//! agent, refreshed whenever a session attaches, its command list changes or
//! the agent is probed, so the new-session page shows models, modes and
//! commands before the first message. A row of another adapter or CLI
//! version is stale: the runtime deletes it as it starts.

use cupper_protocol::capabilities::{SessionCapabilities, Versions};
use cupper_protocol::domain::AgentKind;

use crate::store::{Durability, Receipt, Writer};

/// Stores `caps` as the agent's cached capabilities, read at `at`.
pub fn write(
    writer: &Writer,
    agent: &AgentKind,
    at: u64,
    caps: &SessionCapabilities,
) -> Receipt<()> {
    let agent = agent.to_string();
    let Versions { adapter, cli, .. } = caps.versions.clone();
    let json = serde_json::to_string(caps).expect("capabilities serialize");
    writer.submit(Durability::Normal, move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO agent_capabilities
                 (agent, adapter_version, cli_version, captured_at, json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (agent, adapter, cli, at, json),
        )
        .map(drop)
    })
}

/// Deletes the agent's row unless it is of `versions`.
pub fn invalidate(writer: &Writer, agent: &AgentKind, versions: &Versions) -> Receipt<()> {
    let agent = agent.to_string();
    let Versions { adapter, cli, .. } = versions.clone();
    writer.submit(Durability::Normal, move |conn| {
        conn.execute(
            "DELETE FROM agent_capabilities
             WHERE agent = ?1 AND (adapter_version != ?2 OR cli_version IS NOT ?3)",
            (agent, adapter, cli),
        )
        .map(drop)
    })
}
