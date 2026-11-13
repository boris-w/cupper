//! UserNotifications for the host (design 5.2.9): the authorization, posting,
//! clicks, and clearing what was delivered. A notification has no category,
//! so it has no action buttons (design 5.7.8), and no sound.
//!
//! The notification center serves an app bundle that LaunchServices has
//! (T101, T150), and throws in a process without a bundle identifier: the
//! caller makes sure the process runs from an app bundle. The completion
//! handlers run on the framework's queues.

use std::ptr::NonNull;
use std::sync::Mutex;

use block2::{DynBlock, RcBlock};
use objc2::rc::Retained;
use objc2::runtime::{Bool, ProtocolObject};
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_foundation::{NSDictionary, NSError, NSLocale, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNNotificationSettings, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};

/// The keys of a notification's `userInfo`.
const SESSION_KEY: &str = "session_id";
const REQUEST_KEY: &str = "request_id";

/// Whether the user lets the app notify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationAuth {
    /// Not asked yet.
    NotDetermined,
    /// Turned down, or turned off in System Settings; the system asks no more.
    Denied,
    Authorized,
}

/// A notification: its identifier, title and body, and the session and
/// request a click on it goes to, kept in its `userInfo`: a host the system
/// starts for the click has no database to look them up in (design 5.2.9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub id: String,
    pub title: String,
    pub body: String,
    pub session: String,
    pub request: Option<String>,
}

/// A click on one of the app's notifications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Click {
    /// The notification's identifier.
    pub id: String,
    pub session: Option<String>,
    pub request: Option<String>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and `ClickDelegate`
    // does not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[name = "CupperNotificationDelegate"]
    #[ivars = Box<dyn Fn(Click) + Send + Sync>]
    struct ClickDelegate;

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for ClickDelegate {}

    // SAFETY: `UNUserNotificationCenterDelegate` has no safety requirements.
    unsafe impl UNUserNotificationCenterDelegate for ClickDelegate {
        /// A banner even while the host is in front, as after a click.
        // SAFETY: the protocol's signature.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            done: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            done.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List,));
        }

        // SAFETY: the protocol's signature.
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            done: &DynBlock<dyn Fn()>,
        ) {
            let request = response.notification().request();
            let (session, request_id) = ids(&request.content().userInfo());
            let id = request.identifier().to_string();
            (self.ivars())(Click {
                id,
                session,
                request: request_id,
            });
            done.call(());
        }
    }
);

/// Keeps the notification center's delegate, which [`on_notification_click`]
/// set; the center holds it weakly.
pub struct NotificationClicks(
    #[expect(dead_code, reason = "held, not read")] Retained<ClickDelegate>,
);

/// Sets the notification center's delegate: each click on one of the app's
/// notifications goes to `click`, on the main thread. Set it before AppKit
/// finishes launching, so that a host the system started gets the click it
/// was started for (design 5.2.9); it lasts as long as what this returns.
pub fn on_notification_click(click: impl Fn(Click) + Send + Sync + 'static) -> NotificationClicks {
    let click: Box<dyn Fn(Click) + Send + Sync> = Box::new(click);
    let delegate = ClickDelegate::alloc().set_ivars(click);
    // SAFETY: NSObject's `init`, on a new instance.
    let delegate: Retained<ClickDelegate> = unsafe { msg_send![super(delegate), init] };
    UNUserNotificationCenter::currentNotificationCenter()
        .setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    NotificationClicks(delegate)
}

/// The session and request IDs a notification's `userInfo` holds.
fn ids(info: &NSDictionary) -> (Option<String>, Option<String>) {
    let text = |key: &str| {
        let value = info.objectForKey(&NSString::from_str(key))?;
        Some(value.downcast::<NSString>().ok()?.to_string())
    };
    (text(SESSION_KEY), text(REQUEST_KEY))
}

/// A completion handler called at most once from a block the framework may
/// call from any queue.
pub(crate) struct Once<F>(Mutex<Option<F>>);

impl<F> Once<F> {
    pub(crate) fn new(done: F) -> Self {
        Self(Mutex::new(Some(done)))
    }

    pub(crate) fn take(&self) -> Option<F> {
        self.0.lock().ok()?.take()
    }
}

/// The app's authorization, to `done`.
pub fn notification_authorization(done: impl FnOnce(NotificationAuth) + Send + 'static) {
    let done = Once::new(done);
    let block = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
        // SAFETY: the framework passes settings that live while the block runs.
        let status = unsafe { settings.as_ref() }.authorizationStatus();
        let auth = match status {
            UNAuthorizationStatus::NotDetermined => NotificationAuth::NotDetermined,
            UNAuthorizationStatus::Denied => NotificationAuth::Denied,
            // Provisional and ephemeral authorizations deliver too.
            _ => NotificationAuth::Authorized,
        };
        if let Some(done) = done.take() {
            done(auth);
        }
    });
    UNUserNotificationCenter::currentNotificationCenter()
        .getNotificationSettingsWithCompletionHandler(&block);
}

/// Asks the user to let the app show alerts, no sounds or badges; the
/// authorization then, to `done`. A request the system turned away without
/// asking leaves it undetermined (T101).
pub fn request_notification_authorization(done: impl FnOnce(NotificationAuth) + Send + 'static) {
    let done = Once::new(done);
    let block = RcBlock::new(move |_granted: Bool, _error: *mut NSError| {
        if let Some(done) = done.take() {
            notification_authorization(done);
        }
    });
    UNUserNotificationCenter::currentNotificationCenter()
        .requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert, &block);
}

/// Posts `notice` at once; whether the system took it, to `done`.
pub fn post_notification(notice: &Notice, done: impl FnOnce(Result<(), String>) + Send + 'static) {
    let id = NSString::from_str(&notice.id);
    let request =
        UNNotificationRequest::requestWithIdentifier_content_trigger(&id, &content(notice), None);
    let done = Once::new(done);
    let block = RcBlock::new(move |error: *mut NSError| {
        // SAFETY: null, or an error that lives while the block runs.
        let result = match unsafe { error.as_ref() } {
            None => Ok(()),
            Some(error) => Err(error.localizedDescription().to_string()),
        };
        if let Some(done) = done.take() {
            done(result);
        }
    });
    UNUserNotificationCenter::currentNotificationCenter()
        .addNotificationRequest_withCompletionHandler(&request, Some(&block));
}

/// Removes the app's notifications from Notification Center (design 5.2.13).
pub fn remove_delivered_notifications() {
    UNUserNotificationCenter::currentNotificationCenter().removeAllDeliveredNotifications();
}

/// The user's first preferred language, as `zh-Hans-CN` or `en-US`.
pub fn preferred_language() -> Option<String> {
    let first = NSLocale::preferredLanguages().firstObject()?;
    Some(first.to_string())
}

/// What a notification shows, a title and a body, and the IDs a click needs.
fn content(notice: &Notice) -> Retained<UNMutableNotificationContent> {
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(&notice.title));
    content.setBody(&NSString::from_str(&notice.body));
    let mut entries = vec![(SESSION_KEY, notice.session.as_str())];
    entries.extend(
        notice
            .request
            .as_deref()
            .map(|request| (REQUEST_KEY, request)),
    );
    let keys: Vec<_> = entries
        .iter()
        .map(|(key, _)| NSString::from_str(key))
        .collect();
    let values: Vec<_> = entries
        .iter()
        .map(|(_, value)| NSString::from_str(value))
        .collect();
    let keys: Vec<&NSString> = keys.iter().map(|key| &**key).collect();
    let values: Vec<&NSString> = values.iter().map(|value| &**value).collect();
    let info = NSDictionary::from_slices(&keys, &values);
    // SAFETY: strings to strings, a property list as `userInfo` must be.
    unsafe { content.setUserInfo(&Retained::cast_unchecked::<NSDictionary>(info)) };
    content
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UNIT-notifier-no-actions (design 5.7.8): actions belong to a
    /// notification's category, and the host's notifications have none, so
    /// they show no buttons; no sound either.
    #[test]
    fn unit_notifier_no_actions_a_notice_is_a_title_and_a_body() {
        let notice = Notice {
            id: "n_1".into(),
            title: "Fix the build".into(),
            body: "Approval required".into(),
            session: "s_1".into(),
            request: None,
        };
        let content = content(&notice);
        assert_eq!(content.categoryIdentifier().to_string(), "");
        assert!(content.sound().is_none());
        assert_eq!(content.title().to_string(), "Fix the build");
        assert_eq!(content.body().to_string(), "Approval required");
        assert_eq!(ids(&content.userInfo()), (Some("s_1".into()), None));
        let asked = Notice {
            request: Some("r_2".into()),
            ..notice
        };
        let ids_of = ids(&super::content(&asked).userInfo());
        assert_eq!(ids_of, (Some("s_1".into()), Some("r_2".into())));
    }
}
