//! The host's main thread runs its `CFRunLoop` (design 5.2.3): the
//! UserNotifications delegate and IOKit notifications arrive there, while
//! tokio runs on other threads.
//!
//! A run loop with no source returns at once, and `CFRunLoopStop` only stops
//! a loop that is running. So [`RunLoop`] adds a source of its own: stopping
//! signals it, and the loop stops when the source fires. A stop that comes
//! before the loop runs stays pending, and the loop then stops as it starts.

use std::ffi::c_void;
use std::ptr;

use core_foundation::base::{TCFType, kCFAllocatorDefault};
use core_foundation::runloop::{
    CFRunLoop, CFRunLoopGetCurrent, CFRunLoopSource, CFRunLoopSourceContext, CFRunLoopSourceCreate,
    CFRunLoopSourceSignal, CFRunLoopStop, CFRunLoopWakeUp, kCFRunLoopDefaultMode,
};

/// The run loop of the thread that made it. It stays on that thread.
pub struct RunLoop {
    run_loop: CFRunLoop,
    source: CFRunLoopSource,
}

/// Stops a [`RunLoop`] from any thread.
#[derive(Clone)]
pub struct RunLoopStop {
    run_loop: CFRunLoop,
    source: StopSource,
}

#[derive(Clone)]
struct StopSource(CFRunLoopSource);

// SAFETY: the source is only signalled and released through it, and
// CFRunLoopSourceSignal and CFRelease are safe on any thread (Apple's
// "Thread Safety and Run Loop Objects").
unsafe impl Send for StopSource {}
// SAFETY: as for `Send`; signalling takes no Rust-side state.
unsafe impl Sync for StopSource {}

impl RunLoop {
    /// The current thread's run loop, with the source that stops it.
    pub fn current() -> Self {
        let mut context = CFRunLoopSourceContext {
            version: 0,
            info: ptr::null_mut(),
            retain: None,
            release: None,
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: stop_current,
        };
        // SAFETY: a version 0 context whose callbacks need no `info`;
        // CFRunLoopSourceCreate copies it.
        let source = unsafe { CFRunLoopSourceCreate(kCFAllocatorDefault, 0, &mut context) };
        assert!(!source.is_null(), "CFRunLoopSourceCreate failed");
        // SAFETY: a new source, which we own (the create rule).
        let source = unsafe { CFRunLoopSource::wrap_under_create_rule(source) };
        let run_loop = CFRunLoop::get_current();
        // SAFETY: a constant of CoreFoundation, valid while the process runs.
        run_loop.add_source(&source, unsafe { kCFRunLoopDefaultMode });
        Self { run_loop, source }
    }

    pub fn stopper(&self) -> RunLoopStop {
        RunLoopStop {
            run_loop: self.run_loop.clone(),
            source: StopSource(self.source.clone()),
        }
    }

    /// Runs the loop until [`RunLoopStop::stop`]; at once if that came first.
    pub fn run(self) {
        CFRunLoop::run_current();
    }
}

impl RunLoopStop {
    pub fn stop(&self) {
        // SAFETY: both objects are retained by `self`, and signalling a source
        // and waking its loop are safe from any thread.
        unsafe {
            CFRunLoopSourceSignal(self.source.0.as_concrete_TypeRef());
            CFRunLoopWakeUp(self.run_loop.as_concrete_TypeRef());
        }
    }
}

/// The stop source fired, on the thread running the loop.
extern "C" fn stop_current(_info: *const c_void) {
    // SAFETY: the run loop calls this on its own thread, whose loop is running.
    unsafe { CFRunLoopStop(CFRunLoopGetCurrent()) }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::thread;

    use super::*;

    #[test]
    fn run_loop_stops_from_another_thread() {
        let (stopper, stop) = mpsc::channel();
        let running = thread::spawn(move || {
            let run_loop = RunLoop::current();
            stopper.send(run_loop.stopper()).unwrap();
            run_loop.run();
        });
        stop.recv().unwrap().stop();
        running.join().unwrap();
    }

    #[test]
    fn run_loop_stopped_before_it_runs_returns_at_once() {
        thread::spawn(|| {
            let run_loop = RunLoop::current();
            run_loop.stopper().stop();
            run_loop.run();
        })
        .join()
        .unwrap();
    }
}
