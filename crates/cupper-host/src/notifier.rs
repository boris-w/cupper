//! System notifications (design 5.2.9). The index hands over each
//! notification intent with the session's title; the notifier says it in
//! the built-in copy of its reason, in Chinese or English as the system is,
//! unless the reason's setting is off. The copy is the one exception to "the
//! host sends codes, not text": notifications reach no client. No agent
//! text goes in, since a notification shows on the lock screen too.
//!
//! Which backend posts (design 5.9.9):
//! - release, self-use and dev builds running from an app bundle:
//!   UserNotifications, which they also ask for the authorization as they
//!   start: the user has just opened Cupper and can answer; asked at the first
//!   notification, which shows only while the window is in the background,
//!   the prompt would come when nobody looks, and hold that notification;
//! - builds with the test harness never touch the notification service:
//!   `test_config.notifier: record` records (and stands for an authorized
//!   app), `system` turns notifications off;
//! - anything else, a host outside a bundle: off.

use std::sync::{Arc, Mutex, PoisonError};

use cupper_protocol::ids::IntentId;
use cupper_protocol::index::NotificationAuth;
use cupper_protocol::methods::admin::Settings;
pub use cupper_sys::Notice;

/// Why a notification goes out; `needs_action` splits by the request's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Approval,
    Question,
    TurnCompleted,
    TurnFailed,
}

/// The language of the copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Chinese,
}

impl Language {
    /// The system's first preferred language: Chinese for `zh-*`.
    pub fn of_system() -> Self {
        match cupper_sys::preferred_language() {
            Some(language) if language.starts_with("zh") => Self::Chinese,
            _ => Self::English,
        }
    }
}

/// The copy of `reason`: the title of a session without one, and the body.
/// Worded after Codex's (ui-spec 4.8).
fn copy(reason: Reason, language: Language) -> (&'static str, &'static str) {
    match (reason, language) {
        (Reason::Approval, Language::English) => {
            ("Approval required", "Approve or decline to proceed.")
        }
        (Reason::Approval, Language::Chinese) => ("需要批准", "批准或拒绝后才能继续。"),
        (Reason::Question, Language::English) => ("Question", "Answer a question to proceed."),
        (Reason::Question, Language::Chinese) => ("需要回答", "回答问题后才能继续。"),
        (Reason::TurnCompleted, Language::English) => {
            ("Turn complete", "The agent finished a turn.")
        }
        (Reason::TurnCompleted, Language::Chinese) => ("回合完成", "agent 完成了一个回合。"),
        (Reason::TurnFailed, Language::English) => ("Turn failed", "The turn ended with an error."),
        (Reason::TurnFailed, Language::Chinese) => ("回合失败", "回合出错结束。"),
    }
}

/// Whether the settings let `reason` notify: a failed turn is an error.
fn enabled(settings: &Settings, reason: Reason) -> bool {
    match reason {
        Reason::Approval | Reason::Question => settings.notify_needs_action,
        Reason::TurnCompleted => settings.notify_turn_completed,
        Reason::TurnFailed => settings.notify_error,
    }
}

#[derive(Debug)]
enum Backend {
    System,
    Record(Mutex<Vec<Notice>>),
    Off,
}

/// The host's notifier; clones share it.
#[derive(Debug, Clone)]
pub struct Notifier(Arc<(Backend, Language)>);

impl Notifier {
    /// UserNotifications, in the system's language; off unless `in_app`: the
    /// notification service serves an app bundle only.
    pub fn system(in_app: bool) -> Self {
        let backend = if in_app {
            Backend::System
        } else {
            Backend::Off
        };
        Self(Arc::new((backend, Language::of_system())))
    }

    /// Records instead of posting (test-harness builds).
    pub fn record(language: Language) -> Self {
        Self(Arc::new((Backend::Record(Mutex::default()), language)))
    }

    pub fn off() -> Self {
        Self(Arc::new((Backend::Off, Language::English)))
    }

    /// Notifies of intent `id` with the session's `title`, unless the
    /// settings turn `reason` off.
    pub fn notify(&self, settings: &Settings, id: &IntentId, reason: Reason, title: &str) {
        let (backend, language) = &*self.0;
        if !enabled(settings, reason) {
            return;
        }
        let (fallback, body) = copy(reason, *language);
        let title = if title.trim().is_empty() {
            fallback
        } else {
            title
        };
        let notice = Notice {
            id: id.to_string(),
            title: title.to_owned(),
            body: body.to_owned(),
        };
        match backend {
            Backend::System => cupper_sys::post_notification(&notice, |posted| {
                if let Err(err) = posted {
                    tracing::warn!("the system took no notification: {err}");
                }
            }),
            Backend::Record(notices) => notices
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(notice),
            Backend::Off => {}
        }
    }

    /// As the host starts: asks for the authorization if the user was never
    /// asked, and reports it (design 5.2.9, rule 5). A user who turned it
    /// down is not asked again; the system would not ask anyway (T101).
    pub fn authorize(&self, report: impl FnOnce(NotificationAuth) + Send + 'static) {
        let protocol = |auth| match auth {
            cupper_sys::NotificationAuth::NotDetermined => NotificationAuth::NotDetermined,
            cupper_sys::NotificationAuth::Denied => NotificationAuth::Denied,
            cupper_sys::NotificationAuth::Authorized => NotificationAuth::Authorized,
        };
        match &self.0.0 {
            Backend::System => cupper_sys::notification_authorization(move |auth| {
                if auth != cupper_sys::NotificationAuth::NotDetermined {
                    return report(protocol(auth));
                }
                cupper_sys::request_notification_authorization(move |auth| report(protocol(auth)));
            }),
            Backend::Record(_) => report(NotificationAuth::Authorized),
            Backend::Off => {}
        }
    }

    /// Clears the delivered notifications, as the host exits in order
    /// (design 5.2.13): no notification is left to start a host.
    pub fn clear(&self) {
        if let Backend::System = self.0.0 {
            cupper_sys::remove_delivered_notifications();
        }
    }

    /// What a recording notifier recorded.
    #[cfg(test)]
    pub fn recorded(&self) -> Vec<Notice> {
        match &self.0.0 {
            Backend::Record(notices) => notices.lock().unwrap().clone(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Chinese copy, its title for a session without one included; the
    /// English is IT-notifier-record's.
    #[test]
    fn notifier_says_it_in_chinese_on_a_chinese_system() {
        let notifier = Notifier::record(Language::Chinese);
        let settings = crate::settings::defaults("/h".as_ref(), "/d".as_ref());
        let id = IntentId::from_uuid(1);
        notifier.notify(&settings, &id, Reason::Question, " ");
        notifier.notify(&settings, &id, Reason::TurnFailed, "修复构建");
        let shown: Vec<_> = notifier
            .recorded()
            .into_iter()
            .map(|n| (n.title, n.body))
            .collect();
        let shown: Vec<(&str, &str)> = shown
            .iter()
            .map(|(t, b)| (t.as_str(), b.as_str()))
            .collect();
        assert_eq!(
            shown,
            [
                ("需要回答", "回答问题后才能继续。"),
                ("修复构建", "回合出错结束。")
            ]
        );
    }
}
