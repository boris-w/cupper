//! The test interface of `test-harness` builds (design 5.2.16, 5.9.9):
//! `test/clock_advance`, `test/run_maintenance` and `test/seed`, with scope
//! `test`, which only the `test_harness` principal has.
//!
//! `test/clock_advance` moves the manual clock deadline by deadline and,
//! after each, waits for the whole host to settle (rulings before T047). It
//! runs in a task of its own, outside the barrier: the reply's task waits on
//! it idle, so the barrier can settle while it waits.

use std::sync::Arc;
use std::time::Duration;

use cupper_protocol::error::{ErrorCode, INVALID_PARAMS, RpcError};
use cupper_protocol::methods::admin::{
    MaintenanceTask, TestClockAdvance, TestClockAdvanceParams, TestRunMaintenance,
    TestRunMaintenanceParams, TestSeed,
};
use cupper_protocol::rpc::Method;
use futures::FutureExt;
use serde::de::DeserializeOwned;
use serde_json::json;
use tokio::sync::oneshot;

use super::seed::Seeder;
use crate::clock::{Clock, ManualClock};
use crate::gateway::{Call, Port, Reply, Routes};
use crate::ledger::{Ledger, Outcome};
use crate::quiescence::Quiescence;
use crate::store::retention::{Cleanup, Maintenance};

#[derive(Debug)]
pub(super) struct Harness {
    /// `None` on the system clock, which nothing advances.
    pub clock: Option<ManualClock>,
    pub quiescence: Quiescence,
    pub maintenance: Arc<Maintenance>,
    pub ledger: Ledger,
    pub seeder: Seeder,
}

impl Harness {
    pub fn route(self: &Arc<Self>, routes: &mut Routes) {
        routes.add::<TestClockAdvance>(Arc::clone(self) as Arc<dyn Port>);
        routes.add::<TestRunMaintenance>(Arc::clone(self) as Arc<dyn Port>);
        routes.add::<TestSeed>(Arc::clone(self) as Arc<dyn Port>);
    }
}

impl Port for Harness {
    fn deliver(&self, call: Call) -> Reply {
        let (done, reply) = oneshot::channel::<Outcome>();
        let work = match call.method {
            TestClockAdvance::NAME => match (&self.clock, parse::<TestClockAdvanceParams>(&call)) {
                (Some(clock), Ok(params)) => {
                    let (clock, quiescence) = (clock.clone(), self.quiescence.clone());
                    let by = Duration::from_millis(params.ms.0);
                    async move { advance(&clock, &quiescence, by).await }.boxed()
                }
                (None, _) => {
                    let message = "the host runs on the system clock";
                    futures::future::ready(Err(RpcError::app(ErrorCode::Unsupported, message)))
                        .boxed()
                }
                (_, Err(err)) => futures::future::ready(Err(err)).boxed(),
            },
            TestRunMaintenance::NAME => {
                let (maintenance, ledger) = (Arc::clone(&self.maintenance), self.ledger.clone());
                let params = parse::<TestRunMaintenanceParams>(&call);
                async move { maintain(&maintenance, &ledger, params?.task).await }.boxed()
            }
            _ => self.seeder.seed(call.params),
        };
        // Outside the barrier, so that it can wait for the barrier; seeding
        // and maintenance hold it meanwhile.
        let hold = (call.method != TestClockAdvance::NAME).then(|| self.quiescence.hold());
        tokio::spawn(async move {
            let _ = done.send(work.await);
            drop(hold);
        });
        let stopped = || Err(RpcError::app(ErrorCode::Internal, "the test task stopped"));
        reply
            .map(move |outcome| outcome.unwrap_or_else(|_| stopped()))
            .boxed()
    }
}

/// Moves `clock` by `by`, stopping at every deadline on the way until what it
/// set off has settled; the timers at the target fire last.
async fn advance(clock: &ManualClock, quiescence: &Quiescence, by: Duration) -> Outcome {
    let target = clock.now_mono() + by;
    quiescence.settled().await;
    while let Some(next) = clock.next_deadline().filter(|next| *next <= target) {
        clock.advance_to(next.max(clock.now_mono()));
        quiescence.settled().await;
    }
    clock.advance_to(target);
    quiescence.settled().await;
    Ok(json!({}))
}

async fn maintain(maintenance: &Maintenance, ledger: &Ledger, task: MaintenanceTask) -> Outcome {
    let failed = |err: &dyn std::fmt::Display| RpcError::app(ErrorCode::Internal, err.to_string());
    let cleanup = match task {
        MaintenanceTask::Ledger => {
            ledger.sweep().await.map_err(|err| failed(&err))?;
            return Ok(json!({}));
        }
        MaintenanceTask::RawFrames => Cleanup::RawFrames,
        MaintenanceTask::Audit => Cleanup::Audit,
        MaintenanceTask::OrphanBlobs => Cleanup::OrphanBlobs,
        MaintenanceTask::Unknown(task) => {
            return Err(RpcError::new(INVALID_PARAMS, format!("no task `{task}`")));
        }
    };
    maintenance.run(cleanup).await.map_err(|err| failed(&err))?;
    Ok(json!({}))
}

fn parse<T: DeserializeOwned>(call: &Call) -> Result<T, RpcError> {
    T::deserialize(&call.params).map_err(|err| RpcError::new(INVALID_PARAMS, err.to_string()))
}
