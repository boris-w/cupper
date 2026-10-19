//! Reading downloaded tarballs, which are untrusted data: only regular files
//! are taken (no links), every path must stay below its destination, and the
//! archive's top directory (`package/` in npm tarballs) is dropped.

use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use tar::{Archive, EntryType};

/// Calls `take` with each regular file's path below the top directory, its
/// contents, and whether any execute bit is set.
pub fn for_each_file(
    archive: &Path,
    mut take: impl FnMut(&Path, Vec<u8>, bool) -> Result<()>,
) -> Result<()> {
    let file = File::open(archive).with_context(|| format!("cannot open {}", archive.display()))?;
    let mut tar = Archive::new(GzDecoder::new(BufReader::new(file)));
    let read = || format!("cannot read {}", archive.display());
    for entry in tar.entries().with_context(read)? {
        let mut entry = entry.with_context(read)?;
        if !matches!(
            entry.header().entry_type(),
            EntryType::Regular | EntryType::Continuous
        ) {
            continue;
        }
        let path = inner_path(&entry.path().with_context(read)?)?;
        let executable = entry.header().mode().with_context(read)? & 0o111 != 0;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).with_context(read)?;
        take(&path, bytes, executable)?;
    }
    Ok(())
}

/// Writes every regular file of `archive` below `dest`.
pub fn extract(archive: &Path, dest: &Path) -> Result<()> {
    for_each_file(archive, |path, bytes, executable| {
        write_file(&dest.join(path), &bytes, executable)
    })
}

/// Writes the named members of `archive` (paths below the top directory) to
/// their destinations; fails when one is missing.
pub fn extract_members(archive: &Path, members: &[(&str, PathBuf)]) -> Result<()> {
    let mut found = vec![false; members.len()];
    for_each_file(archive, |path, bytes, executable| {
        for (index, (member, dest)) in members.iter().enumerate() {
            if path == Path::new(member) {
                write_file(dest, &bytes, executable)?;
                found[index] = true;
            }
        }
        Ok(())
    })?;
    if let Some(index) = found.iter().position(|found| !found) {
        bail!("{} has no `{}`", archive.display(), members[index].0);
    }
    Ok(())
}

pub fn write_file(path: &Path, bytes: &[u8], executable: bool) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    fs::write(path, bytes).with_context(|| format!("cannot write {}", path.display()))?;
    let mode = if executable { 0o755 } else { 0o644 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("cannot set the mode of {}", path.display()))
}

/// The path below the top directory; only plain names, no `..` or root.
fn inner_path(path: &Path) -> Result<PathBuf> {
    let mut parts = path.components().filter(|part| *part != Component::CurDir);
    let top = parts.next();
    let inner: Option<PathBuf> = parts
        .map(|part| match part {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect();
    match (top, inner) {
        (Some(Component::Normal(_)), Some(inner)) if !inner.as_os_str().is_empty() => Ok(inner),
        _ => bail!("unsafe path `{}` in the archive", path.display()),
    }
}

#[cfg(test)]
pub mod tests {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use tar::{Builder, Header};

    use super::*;
    use crate::testutil::TempDir;

    /// A .tar.gz with the given entries; names are written raw, so unsafe
    /// paths can be built too.
    pub fn tarball(path: &Path, entries: &[(&str, &str, EntryType, u32)]) {
        let mut builder = Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
        for (name, contents, kind, mode) in entries {
            let mut header = Header::new_gnu();
            header.as_gnu_mut().expect("gnu").name[..name.len()].copy_from_slice(name.as_bytes());
            header.set_entry_type(*kind);
            header.set_mode(*mode);
            header.set_size(contents.len() as u64);
            header.set_cksum();
            builder
                .append(&header, contents.as_bytes())
                .expect("append");
        }
        let gz = builder.into_inner().expect("tar");
        fs::write(path, gz.finish().expect("gzip")).expect("write tarball");
    }

    #[test]
    fn only_regular_files_below_the_top_directory_are_extracted() {
        let dir = TempDir::new("unpack-extract");
        let archive = dir.path().join("a.tgz");
        tarball(
            &archive,
            &[
                ("package/", "", EntryType::Directory, 0o755),
                ("package/lib/main.js", "main", EntryType::Regular, 0o644),
                ("./package/bin/tool", "tool", EntryType::Regular, 0o755),
                ("package/link", "", EntryType::Symlink, 0o777),
            ],
        );
        let dest = dir.path().join("out");
        extract(&archive, &dest).expect("extract");
        assert_eq!(
            fs::read_to_string(dest.join("lib/main.js")).expect("main"),
            "main"
        );
        let mode = fs::metadata(dest.join("bin/tool"))
            .expect("tool")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755);
        assert!(!dest.join("link").exists());

        let one = dir.path().join("one.js");
        extract_members(&archive, &[("lib/main.js", one.clone())]).expect("member");
        assert_eq!(fs::read_to_string(one).expect("one"), "main");
        let err = extract_members(&archive, &[("lib/none.js", dir.path().join("none"))])
            .expect_err("missing member");
        assert!(err.to_string().contains("lib/none.js"), "{err}");
    }

    #[test]
    fn paths_that_leave_the_destination_are_refused() {
        let dir = TempDir::new("unpack-unsafe");
        for name in [
            "package/../../escape",
            "/etc/escape",
            "../escape",
            "toplevel-only",
        ] {
            let archive = dir.path().join("bad.tgz");
            tarball(&archive, &[(name, "x", EntryType::Regular, 0o644)]);
            let dest = dir.path().join("out");
            let err = extract(&archive, &dest).expect_err(name);
            assert!(err.to_string().contains("unsafe path"), "{name}: {err}");
            assert!(
                !dir.path().join("escape").exists() && !dest.exists(),
                "{name}"
            );
        }
    }
}
