//! Project trust (design 5.2.6, security #2, minutes D9): over ACP the CLIs
//! show no workspace-trust dialog, so the host scans the project's own agent
//! configuration and loads it only once the user has trusted that very
//! content.
//!
//! - **Range**: `.claude/`, `.mcp.json` and `.codex/` at every level from the
//!   workspace up to its git root, which the caller finds (T044 through T060);
//!   the workspace alone outside git. The root must be the workspace or one of
//!   its parents, both taken as realpaths.
//! - **Symlinks** are always followed: a directory link is walked into, and a
//!   file counts with its target's content. A dangling link, a cycle or a
//!   target that is neither a regular file nor a directory fails the scan.
//! - **Limits**: more than [`MAX_FILES`] files or [`MAX_BYTES`] bytes (link
//!   targets included) fail the scan, and so do more than [`MAX_ENTRIES`]
//!   entries visited; [`ScanError::code`] tells the dialog why.
//! - **Digest** (rulings before T035): each file's sha-256 covers its path
//!   relative to the workspace, its target's realpath and its whole content,
//!   each preceded by its length as 8 bytes big-endian. The project's digest
//!   is the sha-256 of the tag `cupper-trust-v1` followed by those hashes in
//!   order of the relative paths: a fixed value when there is no file, and a
//!   new value for every change of content, path or link target. A
//!   [`Manifest`] keeps each file's hash for `diff_from_trusted`.
//! - **States** follow [`Trust`]: pure functions of the old state and a scan.
//!
//! Reading the files blocks: callers run [`scan`] on the blocking pool
//! (design 5.2.3). Findings for the native confirmation are T096's; they read
//! [`ScannedFile::content`].

mod digest;
mod state;
#[cfg(test)]
mod tests;
mod walk;

use std::path::{Path, PathBuf};

use cupper_protocol::ids::Digest;

pub use self::digest::Manifest;
pub use self::state::{DigestMismatch, Trust, Trusted, TrustedBy};
pub use self::walk::{MAX_BYTES, MAX_ENTRIES, MAX_FILES, ScanError};

/// The project configuration in a scan's range, as of the scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scan {
    /// Every file, ordered by [`ScannedFile::path`].
    pub files: Vec<ScannedFile>,
    pub digest: Digest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    /// Relative to the workspace, through any links; the levels above it
    /// start with `../` (as `Finding::path`).
    pub path: String,
    /// The realpath of what was read.
    pub target: PathBuf,
    /// A symlink was followed on the way: the file or a directory above it.
    pub linked: bool,
    /// Reached through a link whose target lies outside the workspace; the
    /// dialog says so prominently.
    pub outside_workspace: bool,
    /// What the digest covers of this file.
    pub hash: [u8; 32],
    pub content: Vec<u8>,
}

impl Scan {
    /// The per-file hashes, as `projects.trust_manifest` keeps them.
    pub fn manifest(&self) -> Manifest {
        Manifest::of(&self.files)
    }
}

/// Scans the range of `workspace` (design 5.2.6); `git_root` is `None`
/// outside git. Blocking I/O.
pub fn scan(workspace: &Path, git_root: Option<&Path>) -> Result<Scan, ScanError> {
    let mut files = walk::walk(workspace, git_root)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let digest = digest::digest(&files);
    Ok(Scan { files, digest })
}
