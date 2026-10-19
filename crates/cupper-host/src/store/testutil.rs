//! Helpers shared by the store's unit tests.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rusqlite::Connection;
use rusqlite::types::Value;

/// A fresh directory under the system temp dir, removed on drop.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "cupper-store-test-{}-{unique}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn host_db(&self) -> PathBuf {
        self.0.join("host.db")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The value of `PRAGMA <name>`, as text.
pub fn pragma(conn: &Connection, name: &str) -> String {
    let value = conn
        .query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))
        .expect("read a pragma");
    match value {
        Value::Integer(n) => n.to_string(),
        Value::Text(text) => text,
        other => panic!("PRAGMA {name} returned {other:?}"),
    }
}

pub fn pragmas(conn: &Connection, names: &[&str]) -> Vec<String> {
    names.iter().map(|name| pragma(conn, name)).collect()
}
