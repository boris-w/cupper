//! Where the host socket lives. Pure path computation, shared by host and clients.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// `sun_path` capacity on macOS, including the trailing NUL.
pub const SUN_PATH_MAX: usize = 104;

/// `<data_dir>/run/host.sock`, or a short per-user fallback under `/tmp` when
/// that path does not fit into `sun_path`.
pub fn socket_path(data_dir: &Path, uid: u32) -> PathBuf {
    let preferred = data_dir.join("run").join("host.sock");
    if preferred.as_os_str().len() < SUN_PATH_MAX {
        return preferred;
    }
    let digest = Sha256::digest(data_dir.as_os_str().as_encoded_bytes());
    let short = &hex::encode(digest)[..16];
    PathBuf::from(format!("/tmp/cupper-{uid}/{short}/host.sock"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_paths_fall_back_to_tmp() {
        let short = socket_path(Path::new("/tmp/x"), 501);
        assert_eq!(short, PathBuf::from("/tmp/x/run/host.sock"));
        let long_dir = PathBuf::from(format!("/tmp/{}", "d".repeat(120)));
        let fallback = socket_path(&long_dir, 501);
        assert!(fallback.starts_with("/tmp/cupper-501/"));
        assert!(fallback.as_os_str().len() < SUN_PATH_MAX);
    }
}
