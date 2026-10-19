//! The build kinds of design 5.7.9 as the CLI tells them apart, and what
//! follows from them before any connection: `--build-info`, the default data
//! directory and the socket path.

use std::path::{Path, PathBuf};

use cupper_sys::Cdhash;

/// What this binary was built as. It decides how the CLI checks the host
/// (design 5.8.1, [`HostCheck::of`](crate::verify::HostCheck::of)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildKind {
    /// No features: the host must satisfy the Developer ID requirement.
    Release,
    /// `self-use`: the host must run the cdhash compiled in.
    SelfUse { host_cdhash: Cdhash },
    /// `dev-auth`: the host must run the code of the host file next to the CLI.
    Dev,
    /// `test-harness`: the host proves itself with the `harness` key (T039).
    E2e,
}

/// This binary's kind; `test-harness` wins over `self-use`, which wins over
/// `dev-auth`.
#[cfg(feature = "test-harness")]
pub const BUILD: BuildKind = BuildKind::E2e;
#[cfg(all(feature = "self-use", not(feature = "test-harness")))]
pub const BUILD: BuildKind = BuildKind::SelfUse {
    host_cdhash: parse_cdhash(env!("CUPPER_HOST_CDHASH")),
};
#[cfg(all(
    feature = "dev-auth",
    not(any(feature = "self-use", feature = "test-harness"))
))]
pub const BUILD: BuildKind = BuildKind::Dev;
#[cfg(not(any(feature = "dev-auth", feature = "self-use", feature = "test-harness")))]
pub const BUILD: BuildKind = BuildKind::Release;

/// The compile-time features of this build, in a fixed order. Release builds
/// have none; the release pipeline checks that through `--build-info`.
pub const FEATURES: &[&str] = &[
    #[cfg(feature = "dev-auth")]
    "dev-auth",
    #[cfg(feature = "self-use")]
    "self-use",
    #[cfg(feature = "test-harness")]
    "test-harness",
];

impl BuildKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::SelfUse { .. } => "self_use",
            Self::Dev => "dev",
            Self::E2e => "e2e",
        }
    }

    /// How this kind checks the host, as `--build-info` names it.
    pub fn host_check(self) -> &'static str {
        match self {
            Self::Release => "signature",
            Self::SelfUse { .. } => "cdhash",
            Self::Dev => "host_file_cdhash",
            Self::E2e => "harness_proof",
        }
    }

    /// The host's data directory under `home` (design 5.7.9). An e2e build
    /// has none: every test case passes its own.
    pub fn default_data_dir(self, home: &Path) -> Option<PathBuf> {
        let name = match self {
            Self::Release => "cupper",
            Self::SelfUse { .. } => "cupper-local",
            Self::Dev => "cupper-dev",
            Self::E2e => return None,
        };
        Some(home.join("Library/Application Support").join(name))
    }
}

/// Where the host of `data_dir` listens, found by the rule the host binds
/// with (design 5.2.13 step 8).
pub fn socket_path(data_dir: &Path) -> cupper_sys::Result<PathBuf> {
    let temp = cupper_sys::user_temp_dir()?;
    Ok(cupper_protocol::transport::socket_path(data_dir, &temp))
}

/// One line of JSON: `{"build", "debug_assertions", "features",
/// "host_cdhash"?, "host_check", "name", "version"}`. `host_cdhash` is the
/// one a self-use build compiled in (T005 checks it against the host's).
pub fn build_info(build: BuildKind) -> String {
    let mut info = serde_json::json!({
        "name": "cupper",
        "version": env!("CARGO_PKG_VERSION"),
        "features": FEATURES,
        "debug_assertions": cfg!(debug_assertions),
        "build": build.name(),
        "host_check": build.host_check(),
    });
    if let BuildKind::SelfUse { host_cdhash } = build {
        info["host_cdhash"] = host_cdhash.to_string().into();
    }
    info.to_string()
}

/// `CUPPER_HOST_CDHASH`: 40 lowercase hex digits, as [`Cdhash`] prints.
/// Anything else stops the compilation of a self-use build.
pub const fn parse_cdhash(hex: &str) -> Cdhash {
    const fn nibble(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            _ => panic!("CUPPER_HOST_CDHASH must be lowercase hex"),
        }
    }
    let hex = hex.as_bytes();
    assert!(hex.len() == 40, "CUPPER_HOST_CDHASH must be 40 hex digits");
    let mut hash = [0u8; 20];
    let mut i = 0;
    while i < 20 {
        hash[i] = (nibble(hex[2 * i]) << 4) | nibble(hex[2 * i + 1]);
        i += 1;
    }
    Cdhash(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_cli_host_verify_build_info_names_the_kind_and_its_check() {
        let cdhash = "0123456789abcdef0123456789abcdef01234567";
        let self_use = BuildKind::SelfUse {
            host_cdhash: parse_cdhash(cdhash),
        };
        let kinds = [
            (BuildKind::Release, "release", "signature"),
            (self_use, "self_use", "cdhash"),
            (BuildKind::Dev, "dev", "host_file_cdhash"),
            (BuildKind::E2e, "e2e", "harness_proof"),
        ];
        for (build, name, check) in kinds {
            let mut expected = serde_json::json!({"name": "cupper", "version": env!("CARGO_PKG_VERSION"),
                "features": FEATURES, "debug_assertions": cfg!(debug_assertions),
                "build": name, "host_check": check});
            if build == self_use {
                expected["host_cdhash"] = cdhash.into();
            }
            let info: serde_json::Value = serde_json::from_str(&build_info(build)).unwrap();
            assert_eq!(info, expected);
        }
        // `test-harness` wins over `self-use`, which wins over `dev-auth`.
        let expected = match (cfg!(feature = "test-harness"), cfg!(feature = "self-use")) {
            (true, _) => "e2e",
            (false, true) => "self_use",
            _ if cfg!(feature = "dev-auth") => "dev",
            _ => "release",
        };
        assert_eq!(BUILD.name(), expected);
    }

    #[test]
    fn unit_cli_host_verify_data_dir_and_socket_follow_the_build() {
        let home = Path::new("/Users/example");
        let dir = |build: BuildKind| build.default_data_dir(home);
        let support = home.join("Library/Application Support");
        assert_eq!(dir(BuildKind::Release), Some(support.join("cupper")));
        let self_use = BuildKind::SelfUse {
            host_cdhash: Cdhash([0; 20]),
        };
        assert_eq!(dir(self_use), Some(support.join("cupper-local")));
        assert_eq!(dir(BuildKind::Dev), Some(support.join("cupper-dev")));
        assert_eq!(dir(BuildKind::E2e), None);

        let socket = socket_path(&support.join("cupper")).unwrap();
        assert_eq!(socket, support.join("cupper/run/host.sock"));
        let long = PathBuf::from(format!("/d/{}", "a".repeat(120)));
        let temp = cupper_sys::user_temp_dir().unwrap();
        assert!(socket_path(&long).unwrap().starts_with(temp));
    }
}
