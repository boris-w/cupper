//! `cupper`, the command-line client (design 5.2.14). The commands come with
//! T046; so far it answers `--build-info`.

use std::process::ExitCode;

use cupper_cli::build_kind::{BUILD, build_info};

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args == ["--build-info"] {
        println!("{}", build_info(BUILD));
        return ExitCode::SUCCESS;
    }
    eprintln!("usage: cupper --build-info");
    ExitCode::from(2)
}
