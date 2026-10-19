//! Methods and notifications (design 5.3.4): their parameters and results,
//! and what the policy layer and the schema need to know about each.
//!
//! **Declaring.** Each group of 5.3.4 has its own file. A method is a unit
//! type named after the method (`host/exit` is `HostExit`, its parameters
//! `HostExitParams`), declared with `method!` in one of three forms:
//!
//! ```text
//! method! {
//!     /// `host/exit`: ...
//!     request HostExit = "host/exit" (HostExitParams) -> Empty {
//!         scope: Privileged, idempotency: Command, milestone: M1a,
//!         errors: [Conflict / TurnRunning],
//!     }
//! }
//! method! { client_notification ClientPresence = "client/presence" (ClientPresenceParams) {
//!     scope: Read, idempotency: Natural, milestone: M1a } }
//! method! { host_notification HostHello = "host/hello" (HostHelloParams) { milestone: M1a } }
//! ```
//!
//! - `scope`: a [`Scope`](crate::rpc::Scope) variant, or `None` when any principal may call.
//! - `idempotency`: `Command` exactly when the parameters carry a `command_id`.
//! - `milestone`: the earliest one in the 5.3.4 table.
//! - `errors`: the table's "可能的错误" column, as `Code` or `Code / Reason`
//!   with the variants of [`ErrorCode`](crate::error::ErrorCode) and
//!   [`ErrorReason`](crate::error::ErrorReason).
//!
//! The group's `register` adds each method to the [`catalog`]; its test
//! module adds typical values (`samples`) and rows of the permission matrix
//! (see `catalog`'s tests), which check every method in the catalog.
//!
//! **Errors of every method**, not listed per method: `not_initialized`,
//! `forbidden/scope`, invalid params (JSON-RPC `-32602`), `rate_limited`,
//! `shutting_down` and `internal`; a command may also return
//! `idempotency_mismatch`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Declares a method or notification; see the [module docs](self).
macro_rules! method {
    (
        $(#[$meta:meta])*
        request $name:ident = $wire:literal ($params:ty) -> $result:ty {
            scope: $scope:ident,
            idempotency: $idempotency:ident,
            milestone: $milestone:ident,
            errors: [$($code:ident $(/ $reason:ident)?),* $(,)?] $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $name;

        impl $crate::rpc::Method for $name {
            const NAME: &'static str = $wire;
            const SCOPE: Option<$crate::rpc::Scope> = method!(@scope $scope);
            const IDEMPOTENCY: $crate::rpc::Idempotency =
                $crate::rpc::Idempotency::$idempotency;
            const MILESTONE: $crate::rpc::Milestone = $crate::rpc::Milestone::$milestone;
            const ERRORS: &'static [$crate::rpc::MethodError] = &[$(
                $crate::rpc::MethodError {
                    code: $crate::error::ErrorCode::$code,
                    reason: method!(@reason $($reason)?),
                }
            ),*];
            type Params = $params;
            type Result = $result;
        }
    };
    (
        $(#[$meta:meta])*
        client_notification $name:ident = $wire:literal ($params:ty) {
            scope: $scope:ident,
            idempotency: $idempotency:ident,
            milestone: $milestone:ident $(,)?
        }
    ) => {
        method!(@notification $(#[$meta])* $name = $wire ($params) $milestone);

        impl $crate::rpc::ClientNotification for $name {
            const SCOPE: Option<$crate::rpc::Scope> = method!(@scope $scope);
            const IDEMPOTENCY: $crate::rpc::Idempotency =
                $crate::rpc::Idempotency::$idempotency;
        }
    };
    (
        $(#[$meta:meta])*
        host_notification $name:ident = $wire:literal ($params:ty) {
            milestone: $milestone:ident $(,)?
        }
    ) => {
        method!(@notification $(#[$meta])* $name = $wire ($params) $milestone);

        impl $crate::rpc::HostNotification for $name {}
    };
    (@notification $(#[$meta:meta])* $name:ident = $wire:literal ($params:ty) $milestone:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $name;

        impl $crate::rpc::Notification for $name {
            const NAME: &'static str = $wire;
            const MILESTONE: $crate::rpc::Milestone = $crate::rpc::Milestone::$milestone;
            type Params = $params;
        }
    };
    (@scope None) => { None };
    (@scope $scope:ident) => { Some($crate::rpc::Scope::$scope) };
    (@reason) => { None };
    (@reason $reason:ident) => { Some($crate::error::ErrorReason::$reason) };
}

pub mod catalog;
pub mod handshake;
pub mod stream;
#[cfg(test)]
mod testkit;

/// `{}`: the parameters or result of a method that has none. A client may
/// also leave `params` out; the host reads that as `{}`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Empty {}
