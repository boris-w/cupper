//! The host's main thread runs AppKit's `NSApplication` as an accessory app:
//! no Dock icon, no menu bar, no window (design 5.2.3). The UserNotifications
//! delegate and IOKit notifications arrive there, while tokio runs on other
//! threads. A bare run loop is not enough: LaunchServices then does not know
//! the host as an app, so a click on its notification starts a second host or
//! ends in "… is not open anymore", and a host the system started never gets
//! the click (T101, T150 step 0).
//!
//! - **Stopping.** `NSApplication::stop` takes effect once the loop has
//!   handled an event, and only while it runs. So [`RunLoop`] adds a run
//!   loop source of its own: [`RunLoopStop::stop`] signals it from any
//!   thread; when it fires on the main thread it stops the app and posts an
//!   event to wake the loop. A stop that comes before the loop runs stays
//!   pending, and the loop then stops as it starts.
//! - **Quitting.** As an app, the host gets the quit Apple Event that logout,
//!   shutdown and Activity Monitor's "Quit" send, and AppKit's own handler
//!   would end the process on the spot, without the orderly exit. The loop
//!   takes the event over and hands it to the host, which exits as on SIGTERM
//!   (design 5.2.13). AppKit installs its handlers as it finishes launching,
//!   which happens once per process, so only the first loop hears the event;
//!   the host makes one.

use std::ffi::c_void;
use std::ptr;

use core_foundation::base::{TCFType, kCFAllocatorDefault};
use core_foundation::runloop::{
    CFRunLoop, CFRunLoopSource, CFRunLoopSourceContext, CFRunLoopSourceCreate,
    CFRunLoopSourceSignal, CFRunLoopWakeUp, kCFRunLoopCommonModes,
};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSEvent,
    NSEventModifierFlags, NSEventType,
};
use objc2_foundation::{
    NSAppleEventDescriptor, NSAppleEventManager, NSNotification, NSObject, NSObjectProtocol,
    NSPoint,
};

/// `kCoreEventClass` and `kAEQuitApplication` (`AppleEvents.h`).
const CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
const QUIT_APPLICATION: u32 = u32::from_be_bytes(*b"quit");

/// The main thread's app and its loop. It stays on the main thread.
pub struct RunLoop {
    app: Retained<NSApplication>,
    /// Held here: the app holds its delegate weakly.
    _delegate: Retained<Delegate>,
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

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and `Delegate` does
    // not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CupperRunLoopDelegate"]
    #[ivars = Box<dyn Fn()>]
    struct Delegate;

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for Delegate {}

    // SAFETY: `NSApplicationDelegate` has no safety requirements.
    unsafe impl NSApplicationDelegate for Delegate {
        /// AppKit has installed its Apple Event handlers: replace the quit one.
        // SAFETY: the protocol's signature.
        #[unsafe(method(applicationWillFinishLaunching:))]
        fn will_finish_launching(&self, _notification: &NSNotification) {
            let manager = NSAppleEventManager::sharedAppleEventManager();
            // SAFETY: `self` answers the selector with the signature of an
            // Apple Event handler, and the handler goes before `self` does
            // (`RunLoop`'s `Drop`).
            unsafe {
                let () = msg_send![
                    &manager,
                    setEventHandler: self,
                    andSelector: sel!(quitEvent:withReplyEvent:),
                    forEventClass: CORE_EVENT_CLASS,
                    andEventID: QUIT_APPLICATION,
                ];
            }
        }
    }

    impl Delegate {
        // SAFETY: the signature NSAppleEventManager calls a handler with.
        #[unsafe(method(quitEvent:withReplyEvent:))]
        fn quit_event(&self, _event: &NSAppleEventDescriptor, _reply: &NSAppleEventDescriptor) {
            (self.ivars())();
        }
    }
);

impl RunLoop {
    /// The main thread's loop, `quit` called on each quit Apple Event; `None`
    /// on any other thread.
    pub fn on_main_thread(quit: impl Fn() + 'static) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let app = NSApplication::sharedApplication(mtm);
        // Whatever Info.plist says: a host outside a bundle has none.
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        let quit: Box<dyn Fn()> = Box::new(quit);
        let delegate = Delegate::alloc(mtm).set_ivars(quit);
        // SAFETY: NSObject's `init`, on a new instance.
        let delegate: Retained<Delegate> = unsafe { msg_send![super(delegate), init] };
        app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
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
            perform: stop_app,
        };
        // SAFETY: a version 0 context whose callbacks need no `info`;
        // CFRunLoopSourceCreate copies it.
        let source = unsafe { CFRunLoopSourceCreate(kCFAllocatorDefault, 0, &mut context) };
        assert!(!source.is_null(), "CFRunLoopSourceCreate failed");
        // SAFETY: a new source, which we own (the create rule).
        let source = unsafe { CFRunLoopSource::wrap_under_create_rule(source) };
        let run_loop = CFRunLoop::get_current();
        // In every mode, should AppKit run the loop in another one.
        // SAFETY: a constant of CoreFoundation, valid while the process runs.
        run_loop.add_source(&source, unsafe { kCFRunLoopCommonModes });
        Some(Self {
            app,
            _delegate: delegate,
            run_loop,
            source,
        })
    }

    pub fn stopper(&self) -> RunLoopStop {
        RunLoopStop {
            run_loop: self.run_loop.clone(),
            source: StopSource(self.source.clone()),
        }
    }

    /// Runs the app until [`RunLoopStop::stop`]; at once if that came first.
    pub fn run(self) {
        self.app.run();
    }
}

impl Drop for RunLoop {
    /// Its stop source and quit handler go with it: CoreFoundation takes a
    /// later loop's source, the same callback without `info`, for this one.
    fn drop(&mut self) {
        // SAFETY: a constant of CoreFoundation, valid while the process runs.
        let modes = unsafe { kCFRunLoopCommonModes };
        self.run_loop.remove_source(&self.source, modes);
        let manager = NSAppleEventManager::sharedAppleEventManager();
        // SAFETY: removes the handler the delegate installed, if any, before
        // the delegate goes.
        unsafe {
            let () = msg_send![
                &manager,
                removeEventHandlerForEventClass: CORE_EVENT_CLASS,
                andEventID: QUIT_APPLICATION,
            ];
        }
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

/// The stop source fired, on the main thread.
extern "C" fn stop_app(_info: *const c_void) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    app.stop(None);
    // `stop` takes effect once the loop has handled an event.
    let event = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
        NSEventType::ApplicationDefined,
        NSPoint::new(0.0, 0.0),
        NSEventModifierFlags::empty(),
        0.0,
        0,
        None,
        0,
        0,
        0,
    );
    if let Some(event) = event {
        app.postEvent_atStart(&event, true);
    }
}

/// Sends this process the quit Apple Event from the main thread once the
/// loop runs, as logout would. Call it on the main thread before
/// [`RunLoop::run`].
#[cfg(any(test, feature = "test-util"))]
pub fn quit_once_running() {
    use core_foundation::date::CFAbsoluteTimeGetCurrent;
    use core_foundation::runloop::{CFRunLoopTimer, CFRunLoopTimerRef};
    use objc2::ClassType;
    use objc2::runtime::AnyObject;

    extern "C" fn fire(_timer: CFRunLoopTimerRef, _info: *mut c_void) {
        let target = NSAppleEventDescriptor::currentProcessDescriptor();
        // SAFETY: the class method's signature; -1 and 0 are
        // `kAutoGenerateReturnID` and `kAnyTransactionID`.
        let event: Retained<NSAppleEventDescriptor> = unsafe {
            msg_send![
                NSAppleEventDescriptor::class(),
                appleEventWithEventClass: CORE_EVENT_CLASS,
                eventID: QUIT_APPLICATION,
                targetDescriptor: &*target,
                returnID: -1i16,
                transactionID: 0i32,
            ]
        };
        let mut error: *mut AnyObject = ptr::null_mut();
        // SAFETY: the method's signature; 1 is `NSAppleEventSendNoReply`.
        let _: *mut AnyObject = unsafe {
            msg_send![&event, sendEventWithOptions: 1usize, timeout: 5.0f64, error: &mut error]
        };
    }
    // SAFETY: reads the clock; no preconditions.
    let at = unsafe { CFAbsoluteTimeGetCurrent() } + 0.1;
    let timer = CFRunLoopTimer::new(at, 0.0, 0, 0, fire, ptr::null_mut());
    // SAFETY: a constant of CoreFoundation, valid while the process runs.
    CFRunLoop::get_current().add_timer(&timer, unsafe { kCFRunLoopCommonModes });
}
