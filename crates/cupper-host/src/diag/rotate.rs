//! A log file that rotates by size (design 5.11.1): `name`, then `name.1` to
//! `name.<keep - 1>`, and the oldest falls off. The directory is 0700, the
//! files 0600.

use std::fs::{self, DirBuilder, File, OpenOptions, Permissions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Rotating {
    path: PathBuf,
    max_bytes: u64,
    keep: usize,
    file: File,
    size: u64,
}

impl Rotating {
    /// Appends to `dir/name`; at most `keep` files of up to `max_bytes` each.
    pub fn open(dir: &Path, name: &str, max_bytes: u64, keep: usize) -> io::Result<Self> {
        DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
        fs::set_permissions(dir, Permissions::from_mode(0o700))?;
        let path = dir.join(name);
        let file = append(&path)?;
        let size = file.metadata()?.len();
        Ok(Self {
            path,
            max_bytes,
            keep,
            file,
            size,
        })
    }

    /// Appends `line` and a newline, rotating first when they would not fit.
    pub fn write_line(&mut self, line: &str) -> io::Result<()> {
        let line = format!("{line}\n");
        let len = line.len() as u64;
        if self.size > 0 && self.size + len > self.max_bytes {
            self.rotate()?;
        }
        self.file.write_all(line.as_bytes())?;
        self.size += len;
        Ok(())
    }

    fn rotate(&mut self) -> io::Result<()> {
        let numbered = |index: usize| match index {
            0 => self.path.clone(),
            _ => PathBuf::from(format!("{}.{index}", self.path.display())),
        };
        for index in (1..self.keep).rev() {
            let older = numbered(index - 1);
            if older.exists() {
                fs::rename(older, numbered(index))?;
            }
        }
        self.file = append(&self.path)?;
        self.size = 0;
        Ok(())
    }
}

fn append(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::MetadataExt;

    use super::*;
    use crate::store::testutil::TempDir;

    #[test]
    fn unit_redaction_logs_rotate_by_size() {
        let dir = TempDir::new("rotate");
        let logs = dir.path().join("logs");
        let mut log = Rotating::open(&logs, "host.log", 100, 3).expect("open");
        for line in 0..10 {
            log.write_line(&format!("{line:>39}")).expect("write");
        }
        drop(log);
        let read = |name: &str| fs::read_to_string(logs.join(name)).expect(name);
        // 40 bytes a line, two to a file: the newest four survive.
        assert_eq!(read("host.log"), format!("{:>39}\n{:>39}\n", 8, 9));
        assert_eq!(read("host.log.1"), format!("{:>39}\n{:>39}\n", 6, 7));
        assert_eq!(read("host.log.2"), format!("{:>39}\n{:>39}\n", 4, 5));
        assert!(!logs.join("host.log.3").exists());
        assert_eq!(fs::metadata(&logs).expect("dir").mode() & 0o777, 0o700);
        assert_eq!(
            fs::metadata(logs.join("host.log")).expect("file").mode() & 0o777,
            0o600
        );

        // Reopened, it counts what the file already holds.
        let mut log = Rotating::open(&logs, "host.log", 100, 3).expect("reopen");
        log.write_line(&format!("{:>39}", 10)).expect("write");
        assert_eq!(read("host.log"), format!("{:>39}\n", 10));
        assert_eq!(read("host.log.1"), format!("{:>39}\n{:>39}\n", 8, 9));
    }
}
