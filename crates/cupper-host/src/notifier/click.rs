//! A click on a notification (design 5.2.9) takes the user to its session in
//! the desktop app. With the app connected, the host sends `ui/navigate` on
//! the control connection and then brings the app forward; without, it
//! starts the app with the one URL that says where to go. A host the system
//! started for the click has no connection and no database: it hands over
//! the URL and exits (rule 3, design 5.2.13 step 1).
//!
//! The app is the one the host's bundle is in, three levels up, and only if
//! its bundle identifier is this build kind's; the signature's requirement
//! comes with the release builds' Team ID (T180).

use std::ffi::OsStr;
use std::path::PathBuf;

use cupper_protocol::ids::{RequestId, SessionId};

use crate::runtime::BundleDir;

/// The desktop app a click goes to, and the URL scheme of its build kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outer {
    pub app: PathBuf,
    pub scheme: String,
}

impl Outer {
    /// The app this host's bundle is in, if it is this build kind's: the
    /// host's identifier is the app's with `.host` after it (design 5.7.9).
    pub fn of_this_host() -> Option<Self> {
        let contents = BundleDir::of_this_host().ok().filter(BundleDir::is_app)?;
        // Cupper.app/Contents/Helpers/CupperHost.app/Contents
        let app = contents.0.ancestors().nth(4)?.to_owned();
        let identity = cupper_sys::own_signing_identity().ok()??;
        let app_id = identity.identifier.strip_suffix(".host")?;
        let ours = cupper_sys::bundle_identifier(&app).is_some_and(|id| id == app_id);
        let scheme = scheme(app_id).filter(|_| ours)?;
        Some(Self { app, scheme })
    }

    /// The URL that takes the app to `session` and its request (design 5.7.2).
    pub fn url(&self, session: &SessionId, request: Option<&RequestId>) -> String {
        let query = request.map_or(String::new(), |request| format!("?request={request}"));
        format!("{}://session/{session}{query}", self.scheme)
    }
}

/// The URL scheme of the app `id` (design 5.7.9): `cupper` for
/// `dev.cupper.app`, `cupper-<kind>` for `dev.cupper.app.<kind>`.
fn scheme(app_id: &str) -> Option<String> {
    match app_id.strip_prefix("dev.cupper.app")? {
        "" => Some("cupper".into()),
        kind => Some(format!("cupper-{}", kind.strip_prefix('.')?)),
    }
}

/// What a click did besides `ui/navigate`; record mode keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Handed {
    /// The app came forward.
    Activated(PathBuf),
    /// The app got the URL, and started if it did not run.
    Opened { url: String, app: PathBuf },
}

/// Whether the system started this host for a click (design 5.2.9 rule 3):
/// no arguments, launchd its parent, and a LaunchServices job's name.
pub fn started_for_a_click(no_args: bool, parent: u32, service: Option<&OsStr>) -> bool {
    let job = service.and_then(OsStr::to_str);
    no_args && parent == 1 && job.is_some_and(|job| job.starts_with("application."))
}

/// A host the system started for a click: waits for it on AppKit's loop,
/// hands the app its URL, and exits. No lock, socket or database.
#[cfg(not(feature = "test-harness"))]
pub fn forward() -> crate::exit::HostExit {
    use crate::clock::{Clock, SystemClock};
    use crate::durations::{Durations, NOTIFIER_FORWARD};
    use crate::exit::HostExit;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build();
    let (Ok(runtime), Some(outer)) = (runtime, Outer::of_this_host()) else {
        return HostExit::Error;
    };
    let (clicks, mut clicked) = tokio::sync::mpsc::unbounded_channel();
    // Before AppKit finishes launching, which hands over the click.
    let _delegate = cupper_sys::on_notification_click(move |click| {
        let _ = clicks.send(click);
    });
    let Some(main_loop) = cupper_sys::RunLoop::on_main_thread(|| ()) else {
        return HostExit::Error;
    };
    let stop = main_loop.stopper();
    runtime.spawn(async move {
        let clock = SystemClock::new();
        let wait = clock.timer_at(clock.now_mono() + Durations::default().get(NOTIFIER_FORWARD));
        let click = tokio::select! {
            click = clicked.recv() => click,
            () = wait => None,
        };
        let session = click
            .as_ref()
            .and_then(|c| c.session.as_deref()?.parse().ok());
        if let (Some(click), Some(session)) = (click, session) {
            let request = click.request.and_then(|request| request.parse().ok());
            let (done, opened) = tokio::sync::oneshot::channel();
            let url = outer.url(&session, request.as_ref());
            cupper_sys::open_url_with_app(&url, &outer.app, move |_| {
                let _ = done.send(());
            });
            let _ = opened.await;
        }
        stop.stop();
    });
    main_loop.run();
    HostExit::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifier_click_url_scheme_of_each_build_kind() {
        let kinds = [
            "dev.cupper.app",
            "dev.cupper.app.local",
            "dev.cupper.app.e2e",
        ];
        let schemes = kinds.map(|id| scheme(id).unwrap());
        assert_eq!(schemes, ["cupper", "cupper-local", "cupper-e2e"]);
        assert_eq!(scheme("dev.cupper.appx"), None);
        assert_eq!(scheme("com.example.app"), None);
        let outer = Outer {
            app: "/Applications/Cupper.app".into(),
            scheme: "cupper-dev".into(),
        };
        let session = SessionId::from_uuid(1);
        let request = RequestId::from_uuid(2);
        assert_eq!(
            outer.url(&session, None),
            format!("cupper-dev://session/{session}")
        );
        assert_eq!(
            outer.url(&session, Some(&request)),
            format!("cupper-dev://session/{session}?request={request}")
        );
    }

    /// IT-notifier-click's unit part: how a host the system started is told
    /// from one the desktop started.
    #[test]
    fn it_notifier_click_started_for_a_click_only_by_launchservices() {
        let job = Some(OsStr::new("application.dev.cupper.app.host.1.2"));
        assert!(started_for_a_click(true, 1, job));
        assert!(!started_for_a_click(false, 1, job), "arguments");
        assert!(!started_for_a_click(true, 4242, job), "the desktop's child");
        assert!(!started_for_a_click(true, 1, None));
        assert!(!started_for_a_click(true, 1, Some(OsStr::new("0"))));
    }
}
