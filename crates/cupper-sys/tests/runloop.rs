//! The host's main loop (design 5.2.3, 5.2.13), on the main thread, which
//! libtest's threads are not: one `main` (`harness = false`).
//!
//! - The quit Apple Event that logout and Activity Monitor send reaches the
//!   host's handler instead of ending the process, and a stop from another
//!   thread then ends the loop. This needs the first loop of the process:
//!   AppKit finishes launching only once.
//! - A stop that comes before the loop runs ends it as it starts.

use std::sync::mpsc;
use std::thread;

use cupper_sys::{RunLoop, quit_once_running};

fn main() {
    let (quit, quitted) = mpsc::channel();
    let run_loop = RunLoop::on_main_thread(move || quit.send(()).unwrap())
        .expect("a test without libtest runs on the main thread");
    let stop = run_loop.stopper();
    let stopper = thread::spawn(move || {
        quitted.recv().expect("the quit event reached the handler");
        stop.stop();
    });
    quit_once_running();
    run_loop.run();
    stopper.join().unwrap();
    println!(
        "runloop: the quit event came to the host, and a stop from another thread ended the loop"
    );

    let run_loop = RunLoop::on_main_thread(|| ()).unwrap();
    run_loop.stopper().stop();
    run_loop.run();
    println!("runloop: a stop before the loop ran ended it as it started");
}
