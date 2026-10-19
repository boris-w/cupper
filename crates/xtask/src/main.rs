//! `cargo xtask`: development and CI tasks for the cupper repository.

use std::process::ExitCode;

const USAGE: &str = "usage: cargo xtask <task>\n\nNo tasks yet.";

fn main() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}
