//! How the CLI checks that the other end of the socket is the host of its
//! build (design 5.8.1): a fake host that got hold of the socket path must not
//! receive the user's commands, nor relay an `elevate` to the real host.
//!
//! The check runs once on connect, before anything is sent; it pins the
//! host's audit token, and every later read compares the token with it.

use std::os::fd::AsFd;
use std::path::{Path, PathBuf};

use cupper_protocol::methods::handshake::HostIdentity;
use cupper_sys::{AuditToken, Cdhash, Refusal, Requirement, file_cdhash, peer_token};

use crate::build_kind::BuildKind;

/// The code requirement of the release host, with the Team ID (design 5.8.1).
/// T180 sets it; until then a release CLI accepts no host.
pub const HOST_REQUIREMENT: Option<Requirement> = None;

/// The host files a development CLI looks for next to itself: the bundle in
/// the app's `Helpers` (design 5.10.1), the binary in a cargo target directory.
const HOST_FILES: [&str; 2] = ["CupperHost.app", "cupper-host"];

/// What the host has to show (design 5.8.1, one branch per build kind).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostCheck {
    /// Release: it satisfies the requirement, with the hardened runtime and
    /// no debugger. `None`: this build has no requirement yet.
    Signature(Option<Requirement>),
    /// Self-use: it runs exactly this cdhash.
    Cdhash(Cdhash),
    /// Development: it runs the code of this file. Weak, since anyone can
    /// build a binary there; output marks the host as not verified.
    HostFile(PathBuf),
    /// e2e: the `harness` proof of `initialize`, which T039 adds. Until then
    /// nothing is checked besides the user and the pinned token.
    Harness,
}

#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("the host could not be verified: {0}")]
    Refused(#[from] Refusal),
    #[error("the host runs as user {0}, not as this user")]
    OtherUser(u32),
    #[error("this build has no code requirement for the host")]
    NoRequirement,
    #[error("the host runs cdhash {running}, not {expected}")]
    CdhashMismatch { expected: Cdhash, running: Cdhash },
    #[error("no host file next to the CLI in {0}")]
    NoHostFile(PathBuf),
    #[error("reading the cdhash of {path}: {source}")]
    HostFile {
        path: PathBuf,
        source: cupper_sys::Error,
    },
    #[error("locating the CLI's own executable: {0}")]
    OwnPath(std::io::Error),
    #[error("the host runs in degraded authentication, which a release build refuses")]
    WeakAuth,
    #[error("another process now speaks for the host")]
    TokenChanged,
}

impl HostCheck {
    /// The check of `build`. A development build looks for the host file
    /// next to its own executable, following symbolic links.
    pub fn of(build: BuildKind) -> Result<Self, VerifyError> {
        Ok(match build {
            BuildKind::Release => Self::Signature(HOST_REQUIREMENT),
            BuildKind::SelfUse { host_cdhash } => Self::Cdhash(host_cdhash),
            BuildKind::Dev => {
                let exe = std::env::current_exe().and_then(std::fs::canonicalize);
                let exe = exe.map_err(VerifyError::OwnPath)?;
                Self::HostFile(host_file_next_to(&exe)?)
            }
            BuildKind::E2e => Self::Harness,
        })
    }

    /// Whether output has to mark the host as not verified by its signature.
    pub fn is_weak(&self) -> bool {
        matches!(self, Self::HostFile(_) | Self::Harness)
    }

    /// Checks the process at the other end of `socket`, a connected Unix
    /// socket, and returns its token to pin.
    pub fn verify(&self, socket: impl AsFd) -> Result<AuditToken, VerifyError> {
        let token = peer_token(socket)?;
        if token.euid() != cupper_sys::current_uid() {
            return Err(VerifyError::OtherUser(token.euid()));
        }
        let expected = match self {
            Self::Signature(None) => return Err(VerifyError::NoRequirement),
            Self::Signature(Some(requirement)) => {
                return Ok(token.verify(*requirement).map(|()| token)?);
            }
            Self::Harness => return Ok(token),
            Self::Cdhash(cdhash) => *cdhash,
            Self::HostFile(path) => file_cdhash(path).map_err(|source| VerifyError::HostFile {
                path: path.clone(),
                source,
            })?,
        };
        let running = token.cdhash()?;
        if running != expected {
            return Err(VerifyError::CdhashMismatch { expected, running });
        }
        Ok(token)
    }

    /// The host as `initialize` describes it: a release build refuses a host
    /// in degraded authentication (design 5.3.2, security #12).
    pub fn accept(&self, host: &HostIdentity) -> Result<(), VerifyError> {
        match self {
            Self::Signature(_) if host.weak_auth => Err(VerifyError::WeakAuth),
            _ => Ok(()),
        }
    }
}

/// After every read: the process that did the latest I/O on the other end
/// must still be the one verified (design 5.8.1, SP-4b G2–G4).
pub fn still_pinned(pinned: &AuditToken, socket: impl AsFd) -> Result<(), VerifyError> {
    match peer_token(socket)? {
        token if token == *pinned => Ok(()),
        _ => Err(VerifyError::TokenChanged),
    }
}

fn host_file_next_to(exe: &Path) -> Result<PathBuf, VerifyError> {
    let dir = exe.parent().unwrap_or(Path::new("/"));
    HOST_FILES
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.exists())
        .ok_or_else(|| VerifyError::NoHostFile(dir.to_owned()))
}

#[cfg(test)]
mod tests {
    use cupper_protocol::ids::HostId;
    use cupper_sys::{RefusalKind, own_cdhash};

    use super::*;

    /// Both ends live in this test process, so the "host" is this process:
    /// a linker-signed binary without the hardened runtime.
    fn verify(check: &HostCheck) -> Result<AuditToken, VerifyError> {
        let (client, _host) = std::os::unix::net::UnixStream::pair().unwrap();
        check.verify(&client)
    }

    fn own_exe() -> PathBuf {
        std::env::current_exe().unwrap()
    }

    #[test]
    fn unit_cli_host_verify_each_build_kind_takes_its_branch() {
        let own = own_cdhash().unwrap();
        let other = Cdhash([7; 20]);
        // Release: no requirement until T180, so no host passes.
        let release = HostCheck::of(BuildKind::Release).unwrap();
        assert_eq!(release, HostCheck::Signature(None));
        assert!(matches!(verify(&release), Err(VerifyError::NoRequirement)));
        // Self-use: the cdhash compiled in, here injected.
        let self_use = |host_cdhash| HostCheck::of(BuildKind::SelfUse { host_cdhash }).unwrap();
        assert_eq!(
            verify(&self_use(own)).unwrap().pid(),
            std::process::id() as i32
        );
        let err = verify(&self_use(other)).unwrap_err();
        assert!(
            matches!(err, VerifyError::CdhashMismatch { expected, running }
            if expected == other && running == own),
            "{err}"
        );
        // Development: next to the test binary there is no host file.
        let dir = own_exe()
            .canonicalize()
            .unwrap()
            .parent()
            .unwrap()
            .to_owned();
        let err = HostCheck::of(BuildKind::Dev).unwrap_err();
        assert!(
            matches!(&err, VerifyError::NoHostFile(d) if *d == dir),
            "{err}"
        );
        // e2e: the proof is T039's; the user and the token are checked.
        assert!(verify(&HostCheck::of(BuildKind::E2e).unwrap()).is_ok());

        let weak: Vec<bool> = [
            release,
            self_use(own),
            HostCheck::HostFile(own_exe()),
            HostCheck::Harness,
        ]
        .iter()
        .map(HostCheck::is_weak)
        .collect();
        assert_eq!(weak, [false, false, true, true]);
    }

    #[test]
    fn unit_cli_host_verify_requirement_refuses_a_host_that_does_not_satisfy_it() {
        let requirement =
            Requirement::new(r#"anchor apple generic and identifier "dev.cupper.app.host""#);
        let err = verify(&HostCheck::Signature(Some(requirement))).unwrap_err();
        assert!(
            matches!(err, VerifyError::Refused(r) if r.kind == RefusalKind::Unsatisfied),
            "{err}"
        );
    }

    #[test]
    fn unit_cli_host_verify_host_file_cdhash() {
        let token = verify(&HostCheck::HostFile(own_exe())).unwrap();
        let (client, _host) = std::os::unix::net::UnixStream::pair().unwrap();
        assert!(
            still_pinned(&token, &client).is_ok(),
            "the same process on another socket"
        );

        let err = verify(&HostCheck::HostFile("/bin/ls".into())).unwrap_err();
        assert!(matches!(err, VerifyError::CdhashMismatch { .. }), "{err}");
        let missing = own_exe().with_file_name("no-such-host");
        let err = verify(&HostCheck::HostFile(missing.clone())).unwrap_err();
        assert!(
            matches!(&err, VerifyError::HostFile { path, .. } if *path == missing),
            "{err}"
        );

        let dir = tempdir();
        assert!(matches!(
            host_file_next_to(&dir.join("cupper")),
            Err(VerifyError::NoHostFile(_))
        ));
        for name in HOST_FILES.iter().rev() {
            std::fs::write(dir.join(name), b"").unwrap();
            assert_eq!(
                host_file_next_to(&dir.join("cupper")).unwrap(),
                dir.join(name)
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unit_cli_host_verify_release_refuses_weak_auth() {
        let host = |weak_auth| HostIdentity {
            host_id: "h_1".parse::<HostId>().unwrap(),
            version: "0.1.0".into(),
            weak_auth,
            data_epoch: 1,
        };
        let requirement = Requirement::new("anchor apple");
        let checks = [
            HostCheck::Signature(None),
            HostCheck::Signature(Some(requirement)),
            HostCheck::Cdhash(Cdhash([0; 20])),
            HostCheck::HostFile(own_exe()),
            HostCheck::Harness,
        ];
        for check in &checks {
            assert!(check.accept(&host(false)).is_ok(), "{check:?}");
            let refused = matches!(check.accept(&host(true)), Err(VerifyError::WeakAuth));
            assert_eq!(
                refused,
                matches!(check, HostCheck::Signature(_)),
                "{check:?}"
            );
        }
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cupper-cli-verify-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
