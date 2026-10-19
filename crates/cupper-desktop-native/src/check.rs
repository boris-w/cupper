//! How the desktop checks that the other end of the socket is the host of its
//! build (design 5.8.1, 5.7.9). The check runs once on connect, before
//! anything is sent, and pins the host's audit token; [`crate::relay`]
//! compares the token with it after every read.

use std::os::fd::AsFd;

use cupper_sys::{AuditToken, Refusal, RefusalKind, Requirement, peer_token};

/// The code requirement of the release host, with the Team ID (design 5.8.1).
/// T180 sets it; until then a release build accepts no host.
pub const HOST_REQUIREMENT: Option<Requirement> = None;

/// What the host has to show (design 5.8.1, the column "UI 校验宿主").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostCheck {
    /// Release: it satisfies the requirement, with the hardened runtime and
    /// no debugger. `None`: this build has no requirement yet.
    Signature(Option<Requirement>),
    /// Development, self-use and e2e builds: it runs as this user, and proves
    /// the `dev_key` this process started it with in `initialize`'s
    /// `host_proof` (`HostConnection.verifyHostProof`).
    KeyProof,
}

/// This build's check.
pub const BUILD_CHECK: HostCheck = if cfg!(feature = "dev-auth") {
    HostCheck::KeyProof
} else {
    HostCheck::Signature(HOST_REQUIREMENT)
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Unverified {
    #[error("the host could not be verified: {0}")]
    Refused(#[from] Refusal),
    #[error("the host runs as user {0}, not as this user")]
    OtherUser(u32),
    #[error("this build has no code requirement for the host")]
    NoRequirement,
}

impl Unverified {
    /// The semantic code for the UI: the four kinds of verification errors
    /// of design 5.8.1, a host that is not the one of this build, a host of
    /// another user, and a release build without a requirement.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Refused(refusal) => match refusal.kind {
                RefusalKind::PeerExited => "peer_exited",
                RefusalKind::PeerExeced => "peer_execed",
                RefusalKind::CodeChanged => "code_changed",
                RefusalKind::Other => "unverifiable",
                RefusalKind::Unsatisfied
                | RefusalKind::NoRuntime
                | RefusalKind::GetTaskAllow
                | RefusalKind::Debugged => "not_the_host",
            },
            Self::OtherUser(_) => "other_user",
            Self::NoRequirement => "no_requirement",
        }
    }
}

impl HostCheck {
    /// Checks the process at the other end of `socket`, a connected Unix
    /// socket, and returns its token to pin.
    pub fn verify(self, socket: impl AsFd) -> Result<AuditToken, Unverified> {
        let token = peer_token(socket)?;
        if token.euid() != cupper_sys::current_uid() {
            return Err(Unverified::OtherUser(token.euid()));
        }
        match self {
            Self::Signature(None) => Err(Unverified::NoRequirement),
            Self::Signature(Some(requirement)) => Ok(token.verify(requirement).map(|()| token)?),
            Self::KeyProof => Ok(token),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixStream;

    use super::*;

    /// Both ends live in this test process, so the "host" is this process: a
    /// linker-signed binary without the hardened runtime.
    fn verify(check: HostCheck) -> Result<AuditToken, Unverified> {
        let (client, _host) = UnixStream::pair().unwrap();
        check.verify(&client)
    }

    #[test]
    fn unit_native_check_each_build_kind_takes_its_branch() {
        let release = HostCheck::Signature(HOST_REQUIREMENT);
        let err = verify(release).unwrap_err();
        assert_eq!(
            (err, err.code()),
            (Unverified::NoRequirement, "no_requirement")
        );

        let requirement =
            Requirement::new(r#"anchor apple generic and identifier "dev.cupper.app.host""#);
        let err = verify(HostCheck::Signature(Some(requirement))).unwrap_err();
        assert_eq!(err.code(), "not_the_host", "{err}");

        let pinned = verify(HostCheck::KeyProof).unwrap();
        assert_eq!(pinned.pid(), std::process::id() as i32);
        assert_eq!(pinned.euid(), cupper_sys::current_uid());

        let expected = if cfg!(feature = "dev-auth") {
            HostCheck::KeyProof
        } else {
            release
        };
        assert_eq!(BUILD_CHECK, expected);
    }
}
