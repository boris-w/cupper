//! The agent driver (design 5.6). So far it is the wire to an adapter
//! instance: the [`transport`] it runs over, the [`tap`] every line passes
//! through, and the [`acp`] connection on top. ACP types do not leave this
//! module (design 5.6).

pub mod acp;
pub mod api;
pub mod claude;
pub mod probe;
pub mod tap;
pub mod transport;
