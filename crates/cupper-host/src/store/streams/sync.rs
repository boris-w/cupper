//! The locks of the streams, behind one switch (design 5.9.8): std's in every
//! build, shuttle's when the crate's own tests are built with `--cfg shuttle`.
//! Shuttle can only schedule the locks it provides, so PROP-stream-seq needs
//! the streams to take theirs from here. The `test` condition keeps shuttle,
//! a dev-dependency, out of everything but the unit tests.

#[cfg(all(test, shuttle))]
pub use shuttle::sync::{Mutex, MutexGuard};
#[cfg(not(all(test, shuttle)))]
pub use std::sync::{Mutex, MutexGuard};

/// Locks `mutex`. A panic under a stream lock is a bug that may have left the
/// state half-updated, so it propagates instead of being ignored.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().expect("a holder of this stream lock panicked")
}
