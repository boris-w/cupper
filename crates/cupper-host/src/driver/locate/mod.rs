//! `CliLocator` (design 5.6.6, 5.2.10 "程序的路径"): which `claude`, `git`
//! and `git-lfs` the host runs, and the environment an adapter gets.

pub mod resolve;
#[cfg(test)]
mod testkit;
pub mod trust;
