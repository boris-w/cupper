//! Walking a scan's range (design 5.2.6): each level from the workspace up
//! to the git root, following every symlink.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use super::ScannedFile;
use super::digest::file_hash;

/// More files than this fail the scan.
pub const MAX_FILES: usize = 2_000;
/// More bytes than this, link targets included, fail the scan.
pub const MAX_BYTES: u64 = 16 << 20;
/// More entries visited than this, files and directories, each time a
/// directory link leads into them again, fail the scan: directory links that
/// fork without a cycle would otherwise take exponential time (rulings after
/// T035).
pub const MAX_ENTRIES: usize = 20_000;

/// What the CLIs load at each level (design 5.2.6).
const NAMES: [&str; 3] = [".claude", ".mcp.json", ".codex"];

/// `ELOOP` of macOS: too many levels of symlinks.
const ELOOP: i32 = 62;

/// Why a scan failed. The project stays untrusted, and the dialog explains
/// [`code`](Self::code).
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("{} is neither the workspace nor one of its parents", .0.display())]
    RootNotAncestor(PathBuf),
    #[error("more than {MAX_FILES} files")]
    TooManyFiles,
    #[error("more than {MAX_BYTES} bytes")]
    TooLarge,
    #[error("more than {MAX_ENTRIES} entries visited")]
    TooManyEntries,
    #[error("the symlink {} leads nowhere", .0.display())]
    DanglingLink(PathBuf),
    #[error("the symlink {} leads into a cycle", .0.display())]
    LinkCycle(PathBuf),
    #[error("{} is neither a regular file nor a directory", .0.display())]
    NotRegular(PathBuf),
    #[error("cannot read {}", path.display())]
    Unreadable {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl ScanError {
    /// A stable code for the dialog's text.
    pub fn code(&self) -> &'static str {
        match self {
            Self::RootNotAncestor(_) => "root_not_ancestor",
            Self::TooManyFiles => "too_many_files",
            Self::TooLarge => "too_large",
            Self::TooManyEntries => "too_many_entries",
            Self::DanglingLink(_) => "dangling_link",
            Self::LinkCycle(_) => "link_cycle",
            Self::NotRegular(_) => "not_regular",
            Self::Unreadable { .. } => "unreadable",
        }
    }
}

/// The files in the range of `workspace`, in no particular order.
pub(super) fn walk(
    workspace: &Path,
    git_root: Option<&Path>,
) -> Result<Vec<ScannedFile>, ScanError> {
    let workspace = realpath(workspace)?;
    let root = match git_root {
        Some(root) => realpath(root)?,
        None => workspace.clone(),
    };
    if !workspace.starts_with(&root) {
        return Err(ScanError::RootNotAncestor(root));
    }
    let mut walker = Walker {
        workspace: workspace.clone(),
        files: Vec::new(),
        bytes: 0,
        entries: 0,
        ancestors: Vec::new(),
    };
    let (mut level, mut up) = (workspace.as_path(), String::new());
    loop {
        for name in NAMES {
            walker.entry(&level.join(name), format!("{up}{name}"), false)?;
        }
        if level == root {
            return Ok(walker.files);
        }
        level = level.parent().expect("the root is above");
        up.push_str("../");
    }
}

struct Walker {
    workspace: PathBuf,
    files: Vec<ScannedFile>,
    bytes: u64,
    entries: usize,
    /// The realpaths of the directories being walked, to tell a cycle.
    ancestors: Vec<PathBuf>,
}

impl Walker {
    /// `path`, shown as `rel`; `linked` once a link was followed above it.
    fn entry(&mut self, path: &Path, rel: String, linked: bool) -> Result<(), ScanError> {
        let link = match fs::symlink_metadata(path) {
            Ok(meta) => meta.file_type().is_symlink(),
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(unreadable(path, err)),
        };
        if self.entries == MAX_ENTRIES {
            return Err(ScanError::TooManyEntries);
        }
        self.entries += 1;
        let target = fs::canonicalize(path).map_err(|err| match err.raw_os_error() {
            _ if err.kind() == io::ErrorKind::NotFound => ScanError::DanglingLink(path.into()),
            Some(ELOOP) => ScanError::LinkCycle(path.into()),
            _ => unreadable(path, err),
        })?;
        let linked = linked || link;
        let meta = fs::metadata(&target).map_err(|err| unreadable(&target, err))?;
        if meta.is_dir() {
            if self.ancestors.contains(&target) {
                return Err(ScanError::LinkCycle(path.into()));
            }
            let entries = fs::read_dir(&target).map_err(|err| unreadable(&target, err))?;
            self.ancestors.push(target.clone());
            for entry in entries {
                let name = entry.map_err(|err| unreadable(&target, err))?.file_name();
                // APFS names are UTF-8.
                let rel = format!("{rel}/{}", name.to_string_lossy());
                // Through the directory's realpath: resolving the path through
                // every link above again would cost more at every level.
                self.entry(&target.join(&name), rel, linked)?;
            }
            self.ancestors.pop();
            return Ok(());
        }
        if !meta.is_file() {
            return Err(ScanError::NotRegular(path.into()));
        }
        if self.files.len() == MAX_FILES {
            return Err(ScanError::TooManyFiles);
        }
        let room = MAX_BYTES - self.bytes;
        if meta.len() > room {
            return Err(ScanError::TooLarge);
        }
        let mut content = Vec::new();
        File::open(&target)
            .and_then(|file| file.take(room + 1).read_to_end(&mut content))
            .map_err(|err| unreadable(&target, err))?;
        // The file may have grown since its metadata was read.
        let len = content.len() as u64;
        if len > room {
            return Err(ScanError::TooLarge);
        }
        self.bytes += len;
        let outside_workspace = linked && !target.starts_with(&self.workspace);
        self.files.push(ScannedFile {
            hash: file_hash(&rel, &target, &content),
            path: rel,
            target,
            linked,
            outside_workspace,
            content,
        });
        Ok(())
    }
}

fn realpath(path: &Path) -> Result<PathBuf, ScanError> {
    fs::canonicalize(path).map_err(|err| unreadable(path, err))
}

fn unreadable(path: &Path, source: io::Error) -> ScanError {
    ScanError::Unreadable {
        path: path.into(),
        source,
    }
}
