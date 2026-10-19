//! The host's single-instance lock and its socket (design 5.2.13 steps 4 and
//! 8, 5.8.5).

use std::fs::{self, DirBuilder, File, OpenOptions, Permissions, TryLockError};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use cupper_protocol::transport::socket_path;
use tokio::net::UnixListener;

use crate::exit::HostExit;

/// `host.lock` in the data directory, held for as long as the host runs: a
/// second host exits with 3 instead (design 5.2.13 step 4). The file names
/// the pid of its holder, which the desktop signals when it has to restart a
/// host it cannot authenticate to (design 5.8.1).
#[derive(Debug)]
pub struct HostLock {
    data_dir: PathBuf,
    /// The lock lasts as long as this open file.
    _file: File,
}

#[derive(Debug, thiserror::Error)]
pub enum ListenError {
    #[error("another host holds {}", .0.display())]
    AlreadyRunning(PathBuf),
    #[error("{} must be a directory of this user with mode 0700", .0.display())]
    UnsafeDir(PathBuf),
    #[error("{} is in the way of the socket", .0.display())]
    InTheWay(PathBuf),
    #[error("cannot {what} {}", path.display())]
    Io {
        what: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot find the user's temporary directory")]
    TempDir(#[source] cupper_sys::Error),
}

impl ListenError {
    pub fn exit(&self) -> HostExit {
        match self {
            Self::AlreadyRunning(_) => HostExit::AlreadyRunning,
            _ => HostExit::DataDirUnusable,
        }
    }
}

fn io_error(what: &'static str, path: &Path) -> impl FnOnce(io::Error) -> ListenError {
    let path = path.to_owned();
    move |source| ListenError::Io { what, path, source }
}

impl HostLock {
    /// Takes the lock of `data_dir`, which must exist.
    pub fn acquire(data_dir: &Path) -> Result<Self, ListenError> {
        let path = data_dir.join("host.lock");
        // Not truncated on open: until we hold the lock, the pid in it is the
        // running host's.
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)
            .map_err(io_error("open", &path))?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(ListenError::AlreadyRunning(path)),
            Err(TryLockError::Error(source)) => return Err(io_error("lock", &path)(source)),
        }
        file.set_len(0)
            .and_then(|()| writeln!(file, "{}", std::process::id()))
            .map_err(io_error("write", &path))?;
        Ok(Self {
            data_dir: data_dir.to_owned(),
            _file: file,
        })
    }

    /// Binds the host socket at [`socket_path`]: `run/host.sock` in the data
    /// directory, or a short path in the user's temporary directory. Its
    /// directory is private to this user (0700) and the socket is 0600. Only
    /// the lock holder binds, so a socket already there is a stale one and is
    /// replaced. Call within a tokio runtime.
    pub fn bind(&self) -> Result<(UnixListener, PathBuf), ListenError> {
        let temp = cupper_sys::user_temp_dir().map_err(ListenError::TempDir)?;
        let path = socket_path(&self.data_dir, &temp);
        private_dir(path.parent().expect("the socket path has a directory"))?;
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_socket() => {
                fs::remove_file(&path).map_err(io_error("remove", &path))?;
            }
            Ok(_) => return Err(ListenError::InTheWay(path)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(source) => return Err(io_error("inspect", &path)(source)),
        }
        let listener = UnixListener::bind(&path).map_err(io_error("bind", &path))?;
        fs::set_permissions(&path, Permissions::from_mode(0o600))
            .map_err(io_error("restrict", &path))?;
        Ok((listener, path))
    }
}

/// Creates `dir` with mode 0700 unless it exists, then checks that it is a
/// directory, not a link, of this user with mode 0700: no other user can
/// reach the socket in it or put one there first (design 5.8.5, ADR-05).
fn private_dir(dir: &Path) -> Result<(), ListenError> {
    match DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
        Err(source) => return Err(io_error("create", dir)(source)),
    }
    let meta = fs::symlink_metadata(dir).map_err(io_error("inspect", dir))?;
    if meta.is_dir() && meta.uid() == cupper_sys::current_uid() && meta.mode() & 0o777 == 0o700 {
        Ok(())
    } else {
        Err(ListenError::UnsafeDir(dir.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::testkit::TempDir;

    fn mode(path: &Path) -> u32 {
        fs::symlink_metadata(path).unwrap().mode() & 0o777
    }

    #[test]
    fn host_lock_is_held_once_and_names_its_holder() {
        let dir = TempDir::new("lock");
        let lock = HostLock::acquire(dir.path()).unwrap();
        let pid = fs::read_to_string(dir.path().join("host.lock")).unwrap();
        assert_eq!(pid, format!("{}\n", std::process::id()));
        assert_eq!(mode(&dir.path().join("host.lock")), 0o600);

        let err = HostLock::acquire(dir.path()).unwrap_err();
        assert!(matches!(err, ListenError::AlreadyRunning(_)), "{err}");
        assert_eq!(err.exit().code(), 3);
        // The refused host left the holder's pid alone.
        assert_eq!(
            fs::read_to_string(dir.path().join("host.lock")).unwrap(),
            pid
        );

        drop(lock);
        HostLock::acquire(dir.path()).expect("free again");
    }

    #[tokio::test]
    async fn socket_is_private_and_replaces_a_stale_one() {
        let dir = TempDir::new("bind");
        let lock = HostLock::acquire(dir.path()).unwrap();
        let (listener, path) = lock.bind().unwrap();
        assert_eq!(path, dir.path().join("run/host.sock"));
        assert_eq!((mode(&path), mode(&dir.path().join("run"))), (0o600, 0o700));
        // A host that died leaves its socket file behind.
        drop(listener);
        assert!(path.exists());
        let (_listener, again) = lock.bind().expect("a stale socket is replaced");
        assert_eq!(again, path);
        tokio::net::UnixStream::connect(&path)
            .await
            .expect("connect");
    }

    #[tokio::test]
    async fn socket_refuses_an_unsafe_directory_or_a_file_in_the_way() {
        let dir = TempDir::new("unsafe");
        let lock = HostLock::acquire(dir.path()).unwrap();
        let run = dir.path().join("run");
        DirBuilder::new().mode(0o700).create(&run).unwrap();
        fs::set_permissions(&run, Permissions::from_mode(0o755)).unwrap();
        let err = lock.bind().unwrap_err();
        assert!(
            matches!(&err, ListenError::UnsafeDir(d) if *d == run),
            "{err}"
        );
        assert_eq!(err.exit().code(), 5);

        fs::set_permissions(&run, Permissions::from_mode(0o700)).unwrap();
        fs::write(run.join("host.sock"), b"not a socket").unwrap();
        let err = lock.bind().unwrap_err();
        assert!(matches!(err, ListenError::InTheWay(_)), "{err}");
        assert_eq!(err.exit().code(), 5);
    }

    #[tokio::test]
    async fn socket_falls_back_to_the_user_temp_dir_for_a_long_data_dir() {
        let dir = TempDir::new("long");
        let data_dir = dir.path().join("d".repeat(100));
        fs::create_dir(&data_dir).unwrap();
        let lock = HostLock::acquire(&data_dir).unwrap();
        let (_listener, path) = lock.bind().unwrap();
        let fallback = path.parent().unwrap().to_owned();
        assert!(
            path.starts_with(cupper_sys::user_temp_dir().unwrap()),
            "{path:?}"
        );
        assert_eq!((mode(&path), mode(&fallback)), (0o600, 0o700));
        fs::remove_dir_all(fallback).unwrap();
    }
}
