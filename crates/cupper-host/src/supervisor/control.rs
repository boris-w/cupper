//! `host/status` and `host/exit` (design 5.2.13, 5.3.4): what quitting would
//! interrupt, and the orderly exit the desktop asks for (NFR-8).

use std::sync::{Arc, OnceLock, Weak};

use cupper_protocol::error::{ErrorCode, ErrorReason, INVALID_PARAMS, RpcError};
use cupper_protocol::methods::handshake::{HostExit, HostExitParams, HostStatus, HostStatusResult};
use cupper_protocol::rpc::Method;
use futures::FutureExt;
use serde_json::json;
use tokio::sync::mpsc;

use super::watch::Admission;
use super::{Activity, Exit, Part};
use crate::gateway::{Call, Gateway, Port, Reply, Routes};

#[derive(Debug)]
pub struct Control {
    /// The gateway whose routes hold this port, set once it exists.
    gateway: OnceLock<Weak<Gateway>>,
    supervisor: mpsc::UnboundedSender<Exit>,
    parts: Arc<[Arc<dyn Part>]>,
    admission: Arc<Admission>,
}

impl Control {
    pub(crate) fn new(
        supervisor: mpsc::UnboundedSender<Exit>,
        parts: Arc<[Arc<dyn Part>]>,
        admission: Arc<Admission>,
    ) -> Self {
        Self {
            gateway: OnceLock::new(),
            supervisor,
            parts,
            admission,
        }
    }

    pub(crate) fn route(self: &Arc<Self>, routes: &mut Routes) {
        routes.add::<HostStatus>(Arc::clone(self) as Arc<dyn Port>);
        routes.add::<HostExit>(Arc::clone(self) as Arc<dyn Port>);
    }

    pub(super) fn serve_for(&self, gateway: &Arc<Gateway>) {
        let _ = self.gateway.set(Arc::downgrade(gateway));
    }

    fn activity(&self) -> Activity {
        self.parts.iter().map(|part| part.activity()).sum()
    }

    fn status(&self) -> HostStatusResult {
        let activity = self.activity();
        let gateway = self.gateway.get().and_then(Weak::upgrade);
        HostStatusResult {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            pending_upgrade: self.admission.pending_upgrade(),
            running_turns: activity.running_turns,
            pending_interactions: activity.pending_interactions,
            clients: gateway.map(|gateway| gateway.clients()).unwrap_or_default(),
            pid: cfg!(feature = "test-harness").then(std::process::id),
        }
    }

    /// Answers `host/exit` once its outcome is recorded, then has the
    /// supervisor exit. The answer and `host/shutdown` both go out before the
    /// connection closes, in either order (ruled before T037).
    fn exit(&self, call: Call) -> Reply {
        let ticket = call.ticket.expect("host/exit is a command");
        let running = self.activity().running_turns;
        let supervisor = self.supervisor.clone();
        async move {
            let params = serde_json::from_value::<HostExitParams>(call.params)
                .map_err(|err| RpcError::new(INVALID_PARAMS, err.to_string()));
            let force = match params.and_then(|params| refuse_running(params.force, running)) {
                Ok(force) => force,
                Err(err) => {
                    ticket.reject(err.clone());
                    return Err(err);
                }
            };
            ticket.complete(Ok(json!({}))).commit().await?;
            let _ = supervisor.send(Exit::Requested { force });
            Ok(json!({}))
        }
        .boxed()
    }
}

impl Port for Control {
    fn deliver(&self, call: Call) -> Reply {
        if call.method == HostStatus::NAME {
            let status = serde_json::to_value(self.status()).expect("a status serializes");
            return futures::future::ready(Ok(status)).boxed();
        }
        self.exit(call)
    }
}

/// `host/exit` without `force` while turns run is `conflict/turn_running`;
/// the desktop confirms with the user and sends it again with `force`.
fn refuse_running(force: bool, running_turns: u32) -> Result<bool, RpcError> {
    if force || running_turns == 0 {
        return Ok(force);
    }
    let message = format!("{running_turns} turns are running");
    Err(RpcError::app(ErrorCode::Conflict, message).with_reason(ErrorReason::TurnRunning))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_lifecycle_host_exit_needs_force_while_turns_run() {
        assert!(!refuse_running(false, 0).unwrap());
        assert!(refuse_running(true, 2).unwrap());
        let err = refuse_running(false, 2).unwrap_err();
        assert_eq!(err.app_code(), Some(&ErrorCode::Conflict));
        assert_eq!(err.reason(), Some(&ErrorReason::TurnRunning));
    }
}
