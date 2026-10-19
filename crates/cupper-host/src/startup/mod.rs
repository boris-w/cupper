//! Step 1 of the host's startup (design 5.2.13): the command line,
//! `--build-info` and the stdin [`payload`]. All three belong to the frozen
//! startup contract (5.12.5 #9).
//!
//! `test_config` has no switch: it comes only in the stdin payload (plan P-9).

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::exit::HostExit;

pub mod payload;
#[cfg(feature = "test-harness")]
pub mod test_config;

/// `cupper-host [--data-dir <dir>] [--build-info]`.
pub const USAGE: &str = "usage: cupper-host [--data-dir <dir>] [--build-info]";

/// The compile-time features of this build, in a fixed order. Release builds
/// have none; the release pipeline checks that through `--build-info`.
pub const FEATURES: &[&str] = &[
    #[cfg(feature = "dev-auth")]
    "dev-auth",
    #[cfg(feature = "test-harness")]
    "test-harness",
];

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Print [`build_info`] and exit 0.
    BuildInfo,
    /// Run the host on this data directory (absolute).
    Run { data_dir: PathBuf },
}

impl Command {
    /// Parses the arguments after the program name. `home` is `$HOME`, which
    /// the default data directory, `~/Library/Application Support/cupper`,
    /// sits under.
    pub fn parse(
        args: impl IntoIterator<Item = OsString>,
        home: Option<OsString>,
    ) -> Result<Self, CommandError> {
        let mut build_info = false;
        let mut data_dir = None;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if arg == "--build-info" {
                if build_info {
                    return Err(CommandError::Repeated("--build-info"));
                }
                build_info = true;
            } else if arg == "--data-dir" {
                if data_dir.is_some() {
                    return Err(CommandError::Repeated("--data-dir"));
                }
                let dir = PathBuf::from(args.next().ok_or(CommandError::NoDataDir)?);
                if !dir.is_absolute() {
                    return Err(CommandError::RelativeDataDir(dir));
                }
                data_dir = Some(dir);
            } else {
                return Err(CommandError::Unknown(arg));
            }
        }
        if build_info {
            return Ok(Self::BuildInfo);
        }
        let data_dir = match data_dir {
            Some(dir) => dir,
            None => default_data_dir(home.as_deref()).ok_or(CommandError::NoHome)?,
        };
        Ok(Self::Run { data_dir })
    }
}

fn default_data_dir(home: Option<&OsStr>) -> Option<PathBuf> {
    let home = Path::new(home?);
    home.is_absolute()
        .then(|| home.join("Library/Application Support/cupper"))
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("unknown argument {0:?}\n{USAGE}")]
    Unknown(OsString),
    #[error("{0} given twice\n{USAGE}")]
    Repeated(&'static str),
    #[error("--data-dir needs a directory\n{USAGE}")]
    NoDataDir,
    #[error("--data-dir must be an absolute path, not {0:?}")]
    RelativeDataDir(PathBuf),
    #[error("no --data-dir and no absolute $HOME to find the default data directory")]
    NoHome,
}

impl CommandError {
    pub fn exit(&self) -> HostExit {
        match self {
            Self::NoHome => HostExit::DataDirUnusable,
            _ => HostExit::Error,
        }
    }
}

/// A fresh random UUIDv4 for the host ID, which is `h_` and its 32 hex
/// digits (design 5.4.4). The host chooses it on its first run and keeps it.
/// It carries no time, so it does not come from `IdGen`.
pub fn new_host_uuid() -> Uuid {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).expect("the operating system's random source is available");
    uuid::Builder::from_random_bytes(bytes).into_uuid()
}

/// The unique id of the process that started the host, the only one that
/// may present `dev_key` (design 5.8.1). The parent the kernel recorded at
/// fork is read, so the launcher's exit cannot race this. A host that starts
/// with its launcher gone, adopted by launchd, would wait for a key nobody
/// else may present: [`LauncherGone`], and it exits with 1.
#[cfg(feature = "dev-auth")]
pub fn launcher() -> Result<u64, LauncherGone> {
    let me = std::process::id() as i32;
    match (cupper_sys::process_info(me), cupper_sys::unique_ids(me)) {
        (Some(info), Some(ids)) if info.ppid != 1 => Ok(ids.parent_unique_id),
        _ => Err(LauncherGone),
    }
}

#[cfg(feature = "dev-auth")]
#[derive(Debug, thiserror::Error)]
#[error("the process that started the host has already exited")]
pub struct LauncherGone;

/// One line of JSON: `{"debug_assertions", "features", "name", "version"}`.
/// `features` lists the enabled entries of [`FEATURES`].
pub fn build_info() -> String {
    serde_json::json!({
        "name": env!("CARGO_PKG_NAME"),
        "version": env!("CARGO_PKG_VERSION"),
        "features": FEATURES,
        "debug_assertions": cfg!(debug_assertions),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str], home: Option<&str>) -> Result<Command, CommandError> {
        Command::parse(args.iter().map(OsString::from), home.map(OsString::from))
    }

    #[test]
    fn command_line_takes_data_dir_and_build_info() {
        assert_eq!(
            parse(&[], Some("/Users/example")).unwrap(),
            Command::Run {
                data_dir: "/Users/example/Library/Application Support/cupper".into()
            }
        );
        assert_eq!(
            parse(&["--data-dir", "/tmp/cupper-dev"], None).unwrap(),
            Command::Run {
                data_dir: "/tmp/cupper-dev".into()
            }
        );
        // `--build-info` needs no data directory and wins over `--data-dir`.
        assert_eq!(parse(&["--build-info"], None).unwrap(), Command::BuildInfo);
        assert_eq!(
            parse(&["--data-dir", "/d", "--build-info"], None).unwrap(),
            Command::BuildInfo
        );
    }

    #[test]
    fn command_line_rejects_what_it_does_not_know() {
        let cases: &[(&[&str], Option<&str>, HostExit)] = &[
            (&["--self-test"], None, HostExit::Error),
            (&["--data-dir"], None, HostExit::Error),
            (&["--data-dir", "relative/dir"], None, HostExit::Error),
            (
                &["--data-dir", "/a", "--data-dir", "/b"],
                None,
                HostExit::Error,
            ),
            (&["--build-info", "--build-info"], None, HostExit::Error),
            (&["--data-dir=/a"], None, HostExit::Error),
            (&[], None, HostExit::DataDirUnusable),
            (&[], Some("relative-home"), HostExit::DataDirUnusable),
        ];
        for (args, home, exit) in cases {
            let err = parse(args, *home).expect_err(&format!("{args:?} was accepted"));
            assert_eq!(err.exit(), *exit, "{args:?}: {err}");
        }
    }

    /// P-9: the test configuration has no switch of its own, in any build.
    #[test]
    fn unit_startup_payload_test_config_has_no_switch() {
        for args in [&["--test-config", "{}"][..], &["--test-config={}"]] {
            let err = parse(args, Some("/home")).unwrap_err();
            assert!(matches!(err, CommandError::Unknown(_)), "{args:?}: {err}");
            assert_eq!(err.exit(), HostExit::Error);
        }
    }

    #[test]
    fn host_uuid_is_a_random_v4() {
        let id = new_host_uuid();
        assert_eq!(id.get_version(), Some(uuid::Version::Random));
        assert_eq!(id.get_variant(), uuid::Variant::RFC4122);
        assert_ne!(id, new_host_uuid());
    }

    #[test]
    fn build_info_lists_the_enabled_features() {
        let info: serde_json::Value = serde_json::from_str(&build_info()).unwrap();
        assert_eq!(info["name"], "cupper-host");
        assert_eq!(info["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(info["debug_assertions"], cfg!(debug_assertions));
        let expected: Vec<&str> = [
            ("dev-auth", cfg!(feature = "dev-auth")),
            ("test-harness", cfg!(feature = "test-harness")),
        ]
        .into_iter()
        .filter_map(|(name, on)| on.then_some(name))
        .collect();
        assert_eq!(info["features"], serde_json::json!(expected));
    }
}
