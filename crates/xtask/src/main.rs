//! `cargo xtask`: development and CI tasks for the cupper repository.

mod ci;
mod clock;
mod runtime_pack;
#[cfg(test)]
mod testutil;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        Some((task, rest)) if task == "ci" => ci::main(rest),
        Some((task, rest)) if task == "runtime-pack" => runtime_pack::main(rest),
        _ => {
            eprintln!("{}\n\n{}", ci::usage(), runtime_pack::USAGE);
            ExitCode::from(2)
        }
    }
}
