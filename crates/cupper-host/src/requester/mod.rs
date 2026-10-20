//! What the host finds out about a process that asks for elevation (design
//! 5.8.4): whether it could be an adapter's descendant ([`lineage`]), and
//! what the native dialog shows of it, checked by the host.

pub mod lineage;
