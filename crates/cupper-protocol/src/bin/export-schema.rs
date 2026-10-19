//! `export-schema <path>`: writes the protocol's JSON Schema (design 5.3.10)
//! to `<path>`; `pnpm gen` writes `protocol/schema/v1.json` with it. The only
//! I/O in this crate, and not part of the library or the iOS build (design 5.1.1).

use std::path::PathBuf;
use std::process::ExitCode;

use cupper_protocol::canonical::to_canonical_pretty;
use cupper_protocol::schema::document;

fn main() -> ExitCode {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [path] = args.as_slice() else {
        eprintln!("usage: export-schema <path>");
        return ExitCode::from(2);
    };
    let written = to_canonical_pretty(&document())
        .map_err(|e| e.to_string())
        .and_then(|text| std::fs::write(path, text).map_err(|e| e.to_string()));
    match written {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("export-schema: {}: {e}", path.display());
            ExitCode::FAILURE
        }
    }
}
