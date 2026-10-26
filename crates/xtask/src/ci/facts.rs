//! Facts about the machine for the summary (tool versions, OS, a generic model
//! name), and the local identity the summary must never contain.

use std::path::Path;
use std::process::Command;

/// `(summary key, program, args)`; the version is the first token that starts
/// with a digit, after an optional `v`.
const TOOLS: &[(&str, &str, &[&str])] = &[
    ("rustc", "rustc", &["--version"]),
    ("cargo", "cargo", &["--version"]),
    ("cargo_deny", "cargo", &["deny", "--version"]),
    ("node", "node", &["--version"]),
    ("pnpm", "pnpm", &["--version"]),
];

pub fn toolchain(root: &Path) -> Vec<(&'static str, Option<String>)> {
    TOOLS
        .iter()
        .map(|(key, program, args)| {
            let version = output(root, program, args).and_then(|text| {
                text.split_whitespace()
                    .map(|token| token.trim_start_matches('v'))
                    .find(|token| token.starts_with(|c: char| c.is_ascii_digit()))
                    .map(str::to_owned)
            });
            (*key, version)
        })
        .collect()
}

/// For example `macOS 26.4`.
pub fn os(root: &Path) -> String {
    if cfg!(target_os = "macos") {
        if let Some(version) = output(root, "sw_vers", &["-productVersion"]) {
            return format!("macOS {version}");
        }
    } else if let Ok(release) = std::fs::read_to_string("/etc/os-release")
        && let Some(name) = release
            .lines()
            .find_map(|line| line.strip_prefix("PRETTY_NAME="))
    {
        return name.trim_matches('"').to_owned();
    }
    std::env::consts::OS.to_owned()
}

/// A generic model name such as `MacBook Pro (Apple M4 Max)`: the marketing
/// name and chip only, never the model identifier or serial number.
pub fn machine(root: &Path) -> String {
    let fallback = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
    if !cfg!(target_os = "macos") {
        return fallback;
    }
    let Some(hardware) = output(root, "system_profiler", &["SPHardwareDataType"]) else {
        return fallback;
    };
    let field = |key: &str| {
        hardware
            .lines()
            .find_map(|line| line.trim().strip_prefix(key)?.strip_prefix(':'))
            .map(str::trim)
    };
    match (field("Model Name"), field("Chip")) {
        (Some(model), Some(chip)) => format!("{model} ({chip})"),
        (Some(model), None) => model.to_owned(),
        _ => fallback,
    }
}

/// Names that identify the local user or machine: login names, host names,
/// and the git author. Lowercase, for case-insensitive matching.
pub fn identity(root: &Path) -> Vec<String> {
    // A GitHub-hosted runner's user and host are made up per machine, not a
    // person's, and its user `runner` is also a step's name (T197).
    if std::env::var("RUNNER_ENVIRONMENT").is_ok_and(|env| env == "github-hosted") {
        return Vec::new();
    }
    let mut names: Vec<String> = ["USER", "LOGNAME"]
        .iter()
        .filter_map(|var| std::env::var(var).ok())
        .collect();
    if let Some(home) = std::env::var_os("HOME") {
        names.extend(
            Path::new(&home)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
        );
    }
    if let Some(host) = output(root, "hostname", &[]) {
        names.extend(host.split('.').next().map(str::to_owned));
        names.push(host);
    }
    if cfg!(target_os = "macos") {
        for key in ["ComputerName", "LocalHostName"] {
            names.extend(output(root, "scutil", &["--get", key]));
        }
    }
    for key in ["user.name", "user.email"] {
        names.extend(output(root, "git", &["config", "--get", key]));
    }
    let mut names: Vec<String> = names
        .into_iter()
        .map(|name| name.trim().to_lowercase())
        .filter(|name| !name.is_empty())
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// Trimmed stdout of a successful command, or `None`.
fn output(dir: &Path, program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    (out.status.success() && !text.trim().is_empty()).then(|| text.trim().to_owned())
}
