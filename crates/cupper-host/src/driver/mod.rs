//! The agent driver (design 5.6). So far it is the wire to an adapter
//! instance: the [`transport`] it runs over and the [`tap`] every line passes
//! through.

pub mod tap;
pub mod transport;
