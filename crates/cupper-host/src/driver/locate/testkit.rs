//! Sample install trees for the locator's tests, built under a temp dir. The
//! names in them are placeholders, never a real user's.

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use crate::gateway::testkit::TempDir;

/// The start of a 64-bit Mach-O executable.
const MACH_O: &[u8] = &[0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0x00, 0x00, 0x01];

pub struct Tree {
    root: PathBuf,
    _dir: TempDir,
}

impl Tree {
    /// Builds `spec` below a fresh realpath'd root, one entry per line:
    /// `<path> dir`, `<path> macho` (executable), `<path> exec <text>`,
    /// `<path> plain <text>` (no execute bit) or `<path> -> <target>`.
    pub fn new(name: &str, spec: &str) -> Self {
        let dir = TempDir::new(name);
        let root = fs::canonicalize(dir.path()).expect("realpath of the temp dir");
        for line in spec.lines().map(str::trim).filter(|line| !line.is_empty()) {
            let (path, rest) = line.split_once(char::is_whitespace).expect("<path> <kind>");
            let path = root.join(path);
            fs::create_dir_all(path.parent().expect("below the root")).expect("create parents");
            let (kind, text) = rest.trim().split_once(' ').unwrap_or((rest.trim(), ""));
            match kind {
                "dir" => fs::create_dir_all(&path).expect("create a dir"),
                "macho" => write(&path, MACH_O, 0o755),
                "exec" => write(&path, format!("{text}\n").as_bytes(), 0o755),
                "plain" => write(&path, text.as_bytes(), 0o644),
                "->" => symlink(text, &path).expect("create a symlink"),
                other => panic!("unknown kind {other} in {line}"),
            }
        }
        Self { root, _dir: dir }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }
}

fn write(path: &Path, bytes: &[u8], mode: u32) {
    fs::write(path, bytes).expect("write a file");
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("set the mode");
}
