//! Probing the machine by running programs once (design 5.6.6): so far the
//! login shell's environment. Finding the programs and the rules for their
//! environment are T054's `driver::locate`; the two meet only through plain
//! values (paths, variable names, version strings), wired together by the
//! agent runtime.

pub mod login_shell;
pub mod run;

#[cfg(test)]
pub(crate) mod testkit {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    /// An executable `#!/bin/sh` script `name` in `dir` with `body`.
    pub fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write the script");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }
}
