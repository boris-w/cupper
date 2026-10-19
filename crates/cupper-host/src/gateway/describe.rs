//! `policy/describe` (design 5.7.8, 5.8.3): what a command the main process
//! confirms natively would do. The facts a description reads of the host's
//! state come from the session port (design 5.3.11, rulings before T033).

use cupper_protocol::methods::admin::PolicyDescribeParams;
use futures::future::{self, BoxFuture};
use serde_json::Value;

use super::dispatch::{Reply, SessionPort, typed};
use crate::policy::{self, Confirmable, Facts};

/// The description of the command in `params`, which are valid
/// `policy/describe` params.
pub(super) fn describe(sessions: &dyn SessionPort, params: &Value) -> Reply {
    let command =
        typed::<PolicyDescribeParams>(params).and_then(|params| Ok(Confirmable::parse(&params)?));
    let command = match command {
        Ok(command) => command,
        Err(err) => return Box::pin(future::ready(Err(err))),
    };
    let facts: BoxFuture<'static, _> = match command {
        Confirmable::Create(_)
        | Confirmable::SetConfig(_)
        | Confirmable::Respond(_)
        | Confirmable::Exit(_)
        | Confirmable::Upgrade(_) => sessions.facts(&command),
        _ => Box::pin(future::ready(Ok(Facts::default()))),
    };
    Box::pin(async move {
        let described = policy::describe(&command, &facts.await?)?;
        Ok(serde_json::to_value(described).expect("a description serializes"))
    })
}
