//! A project's trust state (design 5.2.6, 5.5.10): pure functions of the
//! state `projects` records and a new scan. No I/O, no clock: the caller
//! passes the time.

use cupper_protocol::ids::{Digest, PrincipalId};
use cupper_protocol::index::TrustState;
use cupper_protocol::methods::project::TrustedFileChange;

use super::{Manifest, Scan};

/// A project's trust, as the `trust_*` columns of `projects` record it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trust {
    pub state: TrustState,
    /// What the user trusted: set once trusted, and kept while `stale`.
    pub trusted: Option<Trusted>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trusted {
    pub digest: Digest,
    pub manifest: Manifest,
    /// UTC ms.
    pub at_ms: u64,
    pub by: TrustedBy,
}

/// `projects.trusted_by`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustedBy {
    /// Who confirmed: `pr_desktop`, or `pr_test` in tests.
    Principal(PrincipalId),
    /// The range held no file when the project was added.
    AutoEmpty,
}

impl TrustedBy {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Principal(id) => id.as_str(),
            Self::AutoEmpty => "auto_empty",
        }
    }

    /// Back from [`as_str`](Self::as_str).
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "auto_empty" => Some(Self::AutoEmpty),
            id => id.parse().ok().map(Self::Principal),
        }
    }
}

/// `project/trust` with a digest the rescan no longer has
/// (`conflict/digest_mismatch`): the user saw other content.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the configuration changed since the user confirmed it")]
pub struct DigestMismatch;

impl Trust {
    /// A project just added: trusted at once when the range holds no file
    /// (`auto_empty`), untrusted otherwise.
    pub fn added(scan: &Scan, now_ms: u64) -> Self {
        if !scan.files.is_empty() {
            return Self {
                state: TrustState::Untrusted,
                trusted: None,
            };
        }
        Self::trusted(scan, now_ms, TrustedBy::AutoEmpty)
    }

    /// After a rescan (before each Attach and prompt, after each turn): a
    /// trusted project whose digest changed is `stale` and stays so until the
    /// user confirms again, even if the files change back.
    pub fn rescanned(&self, scan: &Scan) -> Self {
        let changed = self
            .trusted
            .as_ref()
            .is_some_and(|trusted| trusted.digest != scan.digest);
        let mut next = self.clone();
        if self.state == TrustState::Trusted && changed {
            next.state = TrustState::Stale;
        }
        next
    }

    /// `project/trust`: `by` confirmed the content of digest `confirmed`, and
    /// `scan` is the rescan; trusted only if they agree.
    pub fn confirmed(
        scan: &Scan,
        confirmed: &Digest,
        by: PrincipalId,
        now_ms: u64,
    ) -> Result<Self, DigestMismatch> {
        if scan.digest != *confirmed {
            return Err(DigestMismatch);
        }
        Ok(Self::trusted(scan, now_ms, TrustedBy::Principal(by)))
    }

    /// `diff_from_trusted`: the files changed since the project was
    /// trusted; for a `stale` project only.
    pub fn diff_from_trusted(&self, scan: &Scan) -> Option<Vec<TrustedFileChange>> {
        let trusted = self.trusted.as_ref()?;
        (self.state == TrustState::Stale).then(|| trusted.manifest.diff(&scan.manifest()))
    }

    fn trusted(scan: &Scan, at_ms: u64, by: TrustedBy) -> Self {
        Self {
            state: TrustState::Trusted,
            trusted: Some(Trusted {
                digest: scan.digest.clone(),
                manifest: scan.manifest(),
                at_ms,
                by,
            }),
        }
    }
}
