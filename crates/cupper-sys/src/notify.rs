//! UserNotifications for the host (design 5.2.9): the authorization, posting,
//! and clearing what was delivered. A notification has no category, so it
//! has no action buttons (design 5.7.8), and no sound.
//!
//! The notification center serves an app bundle that LaunchServices has
//! (T101, T150), and throws in a process without a bundle identifier: the
//! caller makes sure the process runs from an app bundle. The completion
//! handlers run on the framework's queues.

use std::ptr::NonNull;
use std::sync::Mutex;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_foundation::{NSError, NSLocale, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent,
    UNNotificationRequest, UNNotificationSettings, UNUserNotificationCenter,
};

/// Whether the user lets the app notify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationAuth {
    /// Not asked yet.
    NotDetermined,
    /// Turned down, or turned off in System Settings; the system asks no more.
    Denied,
    Authorized,
}

/// A notification: its identifier, title and body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub id: String,
    pub title: String,
    pub body: String,
}

/// A completion handler called at most once from a block the framework may
/// call from any queue.
struct Once<F>(Mutex<Option<F>>);

impl<F> Once<F> {
    fn new(done: F) -> Self {
        Self(Mutex::new(Some(done)))
    }

    fn take(&self) -> Option<F> {
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

/// What a notification shows: a title and a body, nothing else.
fn content(notice: &Notice) -> Retained<UNMutableNotificationContent> {
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(&notice.title));
    content.setBody(&NSString::from_str(&notice.body));
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
        };
        let content = content(&notice);
        assert_eq!(content.categoryIdentifier().to_string(), "");
        assert!(content.sound().is_none());
        assert_eq!(content.title().to_string(), "Fix the build");
        assert_eq!(content.body().to_string(), "Approval required");
    }
}
