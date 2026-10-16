//! SP-5 gitoxide probe. One operation per process run; timing is done by
//! compare_gix.py so it is measured the same way as the git CLI.
//!
//!   sp5-gix-probe status   <repo>                 -> number of status items
//!   sp5-gix-probe snapshot <repo>                 -> tree id (HEAD tree + worktree changes, no filters)
//!   sp5-gix-probe diff     <repo> <tree1> <tree2> -> number of changed paths
//!   sp5-gix-probe wtadd    <repo> <branch> <dest> -> checked-out file count
//!
//! The "snapshot" here is an emulation: gitoxide has no `git add -A`, so we walk
//! status (index vs worktree + untracked files), write blobs for changed and
//! untracked files and edit the HEAD tree. It assumes nothing is staged and does
//! NOT run clean filters (CRLF, LFS) - see results.
use std::{error::Error, path::Path, sync::atomic::AtomicBool};

use gix::{
    bstr::{BString, ByteSlice},
    objs::tree::EntryKind,
    progress::Discard,
    status::{index_worktree::Item as IwItem, Item, UntrackedFiles},
};
use gix_status_types::*;

mod gix_status_types {
    pub use gix::status::plumbing::index_as_worktree::{Change, EntryStatus};
}

type R<T> = Result<T, Box<dyn Error>>;

fn open(path: &str) -> R<gix::Repository> {
    Ok(gix::open(path)?)
}

fn status(repo: &gix::Repository) -> R<usize> {
    let iter = repo
        .status(Discard)?
        .untracked_files(UntrackedFiles::Files)
        .index_worktree_rewrites(None)
        .into_iter(None::<BString>)?;
    let mut n = 0;
    for item in iter {
        item?;
        n += 1;
    }
    Ok(n)
}

fn kind_of(meta: &std::fs::Metadata) -> EntryKind {
    use std::os::unix::fs::PermissionsExt;
    if meta.file_type().is_symlink() {
        EntryKind::Link
    } else if meta.permissions().mode() & 0o100 != 0 {
        EntryKind::BlobExecutable
    } else {
        EntryKind::Blob
    }
}

fn write_path(repo: &gix::Repository, workdir: &Path, rela: &[u8]) -> R<Option<(EntryKind, gix::ObjectId)>> {
    let path = workdir.join(rela.to_path()?);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(_) => return Ok(None),
    };
    let kind = kind_of(&meta);
    let id = if kind == EntryKind::Link {
        use std::os::unix::ffi::OsStrExt;
        let target = std::fs::read_link(&path)?;
        repo.write_blob(target.as_os_str().as_bytes())?.detach()
    } else if meta.is_file() {
        repo.write_blob_stream(std::fs::File::open(&path)?)?.detach()
    } else {
        return Ok(None);
    };
    Ok(Some((kind, id)))
}

fn snapshot(repo: &gix::Repository) -> R<gix::ObjectId> {
    let workdir = repo.workdir().ok_or("bare repo")?.to_owned();
    let head_tree = repo.head_commit()?.tree_id()?.detach();
    let mut editor = repo.edit_tree(head_tree)?;
    let iter = repo
        .status(Discard)?
        .untracked_files(UntrackedFiles::Files)
        .index_worktree_rewrites(None)
        .into_iter(None::<BString>)?;
    for item in iter {
        match item? {
            Item::IndexWorktree(IwItem::Modification { rela_path, status, .. }) => match status {
                EntryStatus::Change(Change::Removed) => {
                    editor.remove(rela_path.as_bstr())?;
                }
                EntryStatus::Change(_) | EntryStatus::IntentToAdd | EntryStatus::Conflict { .. } => {
                    match write_path(repo, &workdir, rela_path.as_bytes())? {
                        Some((kind, id)) => {
                            editor.upsert(rela_path.as_bstr(), kind, id)?;
                        }
                        None => {
                            editor.remove(rela_path.as_bstr())?;
                        }
                    }
                }
                EntryStatus::NeedsUpdate(_) => {}
            },
            Item::IndexWorktree(IwItem::DirectoryContents { entry, .. }) => {
                if entry.status == gix::dir::entry::Status::Untracked {
                    if let Some((kind, id)) = write_path(repo, &workdir, entry.rela_path.as_bytes())? {
                        editor.upsert(entry.rela_path.as_bstr(), kind, id)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(editor.write()?.detach())
}

fn diff(repo: &gix::Repository, a: &str, b: &str) -> R<usize> {
    let ta = repo.find_tree(gix::ObjectId::from_hex(a.as_bytes())?)?;
    let tb = repo.find_tree(gix::ObjectId::from_hex(b.as_bytes())?)?;
    Ok(repo.diff_tree_to_tree(&ta, &tb, None)?.len())
}

fn wtadd(repo: &gix::Repository, branch: &str, dest: &str) -> R<usize> {
    let name: gix::refs::FullName = format!("refs/heads/{branch}").try_into()?;
    let interrupt = AtomicBool::new(false);
    let (_wt, outcome) = repo.add_worktree(dest, gix::worktree::add::Head::Attached(name), Discard, &interrupt)?;
    Ok(outcome.files_updated)
}

fn main() -> R<()> {
    let args: Vec<String> = std::env::args().collect();
    let repo = open(&args[2])?;
    match args[1].as_str() {
        "status" => println!("{}", status(&repo)?),
        "snapshot" => println!("{}", snapshot(&repo)?),
        "diff" => println!("{}", diff(&repo, &args[3], &args[4])?),
        "wtadd" => println!("{}", wtadd(&repo, &args[3], &args[4])?),
        other => return Err(format!("unknown command {other}").into()),
    }
    Ok(())
}
