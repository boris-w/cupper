//! `cargo xtask`: development and CI tasks for the cupper repository.

mod ci;
mod clock;
mod codesign;
mod git_lfs;
mod host_bundle;
mod runtime_pack;
mod size;
#[cfg(test)]
mod testutil;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        Some((task, rest)) if task == "ci" => ci::main(rest),
        Some((task, rest)) if task == "runtime-pack" => runtime_pack::main(rest),
        Some((task, rest)) if task == "host-bundle" => host_bundle::main(rest),
        Some((task, rest)) if task == "lock" => ci::lock::main(rest),
        Some((task, rest)) if task == "size" => size::main(rest),
        Some((task, rest)) if task == "git-lfs" => git_lfs::main(rest),
        _ => {
            eprintln!(
                "{}\n\n{}\n\n{}\n\n{}\n\n{}\n\n{}",
                ci::usage(),
                runtime_pack::USAGE,
                host_bundle::USAGE,
                ci::lock::USAGE,
                size::USAGE,
                git_lfs::USAGE
            );
            ExitCode::from(2)
        }
    }
}
