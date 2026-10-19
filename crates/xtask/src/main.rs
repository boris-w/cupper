//! `cargo xtask`: development and CI tasks for the cupper repository.

mod ci;
mod clock;
#[cfg(test)]
mod testutil;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        Some((task, rest)) if task == "ci" => ci::main(rest),
        _ => {
            eprintln!("{}", ci::usage());
            ExitCode::from(2)
        }
    }
}
