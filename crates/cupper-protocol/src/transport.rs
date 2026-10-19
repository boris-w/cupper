//! Where the host socket lives (design 5.2.13, ADR-05). Pure path arithmetic
//! shared by the host and its clients; looking up the user's temporary
//! directory and checking its owner and mode stay with the caller.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// `sun_path` capacity on macOS, including the trailing NUL.
pub const SUN_PATH_MAX: usize = 104;

/// `<data_dir>/run/host.sock`, or, when that does not fit into `sun_path`,
/// `<user_temp_dir>/cupper-<hash of data_dir>/host.sock`, where `user_temp_dir`
/// is `confstr(_CS_DARWIN_USER_TEMP_DIR)`.
pub fn socket_path(data_dir: &Path, user_temp_dir: &Path) -> PathBuf {
    let preferred = data_dir.join("run").join("host.sock");
    if preferred.as_os_str().len() < SUN_PATH_MAX {
        return preferred;
    }
    let digest = Sha256::digest(data_dir.as_os_str().as_encoded_bytes());
    let short: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
    user_temp_dir
        .join(format!("cupper-{short}"))
        .join("host.sock")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_proto_base_socket_path_falls_back_to_user_temp() {
        let temp = Path::new("/tmp/user-temp/T");
        assert_eq!(
            socket_path(Path::new("/d/cupper"), temp),
            Path::new("/d/cupper/run/host.sock")
        );

        let long_a = PathBuf::from(format!("/d/{}", "a".repeat(120)));
        let long_b = PathBuf::from(format!("/d/{}", "b".repeat(120)));
        let a = socket_path(&long_a, temp);
        assert!(a.starts_with(temp) && a.ends_with("host.sock"), "{a:?}");
        assert!(a.as_os_str().len() < SUN_PATH_MAX);
        assert_eq!(a, socket_path(&long_a, temp), "clients find the same path");
        assert_ne!(a, socket_path(&long_b, temp));
    }
}
