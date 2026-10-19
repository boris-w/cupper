//! The gateway (design 5.2.4): the host socket, the connections on it, and
//! local authentication (design 5.8.1, ADR-05).

pub mod listen;
#[cfg(any(test, feature = "test-harness"))]
pub mod testkit;
