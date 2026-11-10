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
//!
//! A click on a notification is [`click`]'s.

pub mod click;

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use cupper_protocol::ids::{IntentId, RequestId, SessionId};
use cupper_protocol::index::NotificationAuth;
use cupper_protocol::methods::admin::Settings;
use cupper_protocol::methods::handshake::{UiNavigate, UiNavigateParams};
pub use cupper_sys::{Click, Notice};

use self::click::{Handed, Outer};
use crate::clock::Clock;
use crate::gateway::Gateway;

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
    Record(Mutex<Vec<Notice>>, Mutex<Vec<Handed>>),
    Off,
}

/// The host's notifier, with the app a click goes to; clones share it.
#[derive(Debug, Clone)]
pub struct Notifier(Arc<(Backend, Language, Option<Outer>)>);

impl Notifier {
    /// UserNotifications, in the system's language; off unless `in_app`: the
    /// notification service serves an app bundle only.
    pub fn system(in_app: bool) -> Self {
        let (backend, outer) = match in_app {
            true => (Backend::System, Outer::of_this_host()),
            false => (Backend::Off, None),
        };
        Self(Arc::new((backend, Language::of_system(), outer)))
    }

    /// Records what it would post and hand to the app (test-harness builds).
    pub fn record(language: Language, outer: Option<Outer>) -> Self {
        let backend = Backend::Record(Mutex::default(), Mutex::default());
        Self(Arc::new((backend, language, outer)))
    }

    pub fn off() -> Self {
        Self(Arc::new((Backend::Off, Language::English, None)))
    }

    /// Whether it posts through UserNotifications, whose clicks the main
    /// thread then takes.
    pub fn posts(&self) -> bool {
        matches!(self.0.0, Backend::System)
    }

    /// Notifies of intent `id` with the session's `title`, unless the
    /// settings turn `reason` off; a click goes to `session` and `request`.
    pub fn notify(
        &self,
        settings: &Settings,
        id: &IntentId,
        reason: Reason,
        title: &str,
        (session, request): (&SessionId, Option<&RequestId>),
    ) {
        let (backend, language, _) = &*self.0;
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
            session: session.to_string(),
            request: request.map(RequestId::to_string),
        };
        match backend {
            Backend::System => cupper_sys::post_notification(&notice, |posted| {
                if let Err(err) = posted {
                    tracing::warn!("the system took no notification: {err}");
                }
            }),
            Backend::Record(notices, _) => notices
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
            Backend::Record(..) => report(NotificationAuth::Authorized),
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

    /// Takes a click to the app (design 5.2.9): `ui/navigate` on the control
    /// connection and, once the system has brought the host forward, the app
    /// forward too; with no control connection, the URL. Clicks on another
    /// app's notification or without a session go nowhere.
    pub async fn click(
        &self,
        click: Click,
        gateway: &Gateway,
        clock: &dyn Clock,
        settle: Duration,
    ) {
        let Some(session) = click
            .session
            .and_then(|session| session.parse::<SessionId>().ok())
        else {
            return tracing::warn!(id = click.id, "a click without a session");
        };
        let request = click
            .request
            .and_then(|request| request.parse::<RequestId>().ok());
        let params = UiNavigateParams {
            session_id: session.clone(),
            request_id: request.clone(),
        };
        let navigated = gateway.notify_controls::<UiNavigate>(&params);
        let Some(outer) = &self.0.2 else {
            return;
        };
        let handed = if navigated {
            clock.timer_at(clock.now_mono() + settle).await;
            Handed::Activated(outer.app.clone())
        } else {
            let url = outer.url(&session, request.as_ref());
            Handed::Opened {
                url,
                app: outer.app.clone(),
            }
        };
        let done = |done: cupper_sys::Result<()>| {
            if let Err(err) = done {
                tracing::warn!("cannot bring the app forward: {err}");
            }
        };
        match (&self.0.0, handed) {
            (Backend::System, Handed::Activated(app)) => cupper_sys::open_app(&app, done),
            (Backend::System, Handed::Opened { url, app }) => {
                cupper_sys::open_url_with_app(&url, &app, done)
            }
            (Backend::Record(_, handed_all), handed) => handed_all
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(handed),
            (Backend::Off, _) => {}
        }
    }

    /// The click on recorded notification `id`, as the system would report
    /// it (`test/notification_click`).
    pub fn recorded_click(&self, id: &IntentId) -> Option<Click> {
        let Backend::Record(notices, _) = &self.0.0 else {
            return None;
        };
        let notices = notices.lock().unwrap_or_else(PoisonError::into_inner);
        let notice = notices.iter().find(|notice| notice.id == id.as_str())?;
        Some(Click {
            id: notice.id.clone(),
            session: Some(notice.session.clone()),
            request: notice.request.clone(),
        })
    }

    /// What a recording notifier recorded: the notices, and what clicks
    /// handed to the app.
    #[cfg(test)]
    pub fn recorded(&self) -> Vec<Notice> {
        match &self.0.0 {
            Backend::Record(notices, _) => notices.lock().unwrap().clone(),
            _ => Vec::new(),
        }
    }

    #[cfg(test)]
    pub fn handed(&self) -> Vec<Handed> {
        match &self.0.0 {
            Backend::Record(_, handed) => handed.lock().unwrap().clone(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// IT-notifier-click (design 5.2.9): with no desktop UI connected, a
    /// click hands the app its URL; with the UI's control connection, it
    /// sends `ui/navigate` there and brings the app forward. Record mode
    /// keeps what would go to the app.
    #[cfg(feature = "dev-auth")]
    #[tokio::test]
    async fn it_notifier_click_navigates_the_ui_or_hands_the_url() {
        use crate::gateway::testkit::{Client, KEY, TestGateway, initialize_params, key_auth};
        use serde_json::json;

        let gw = TestGateway::start(None);
        let app = std::path::PathBuf::from("/Applications/Cupper Local.app");
        let outer = Outer {
            app: app.clone(),
            scheme: "cupper-local".into(),
        };
        let notifier = Notifier::record(Language::English, Some(outer));
        let clock = crate::clock::SystemClock::new();
        let (session, request) = (SessionId::from_uuid(1), RequestId::from_uuid(2));
        let click = Click {
            id: "n_1".into(),
            session: Some(session.to_string()),
            request: Some(request.to_string()),
        };
        notifier
            .click(click.clone(), &gw.gateway, &clock, Duration::ZERO)
            .await;

        let mut control = Client::connect(&gw.socket).await;
        let nonce = control.hello().await;
        let (auth, _) = key_auth("dev_key", &KEY, &nonce, std::process::id() as i32);
        let mut params = initialize_params(&nonce, auth);
        (params["client"]["kind"], params["role"]) = (json!("desktop"), json!("control"));
        control.call("initialize", params).await.unwrap();
        notifier
            .click(click, &gw.gateway, &clock, Duration::ZERO)
            .await;
        let navigate = loop {
            let message = control.next().await.expect("ui/navigate");
            if message["method"] == "ui/navigate" {
                break message;
            }
        };
        let to = json!({"session_id": session, "request_id": request});
        assert_eq!(navigate["params"], to);
        let url = format!("cupper-local://session/{session}?request={request}");
        assert_eq!(
            notifier.handed(),
            [
                Handed::Opened {
                    url,
                    app: app.clone()
                },
                Handed::Activated(app)
            ]
        );
    }

    /// The Chinese copy, its title for a session without one included; the
    /// English is IT-notifier-record's.
    #[test]
    fn notifier_says_it_in_chinese_on_a_chinese_system() {
        let notifier = Notifier::record(Language::Chinese, None);
        let settings = crate::settings::defaults("/h".as_ref(), "/d".as_ref());
        let (id, to) = (IntentId::from_uuid(1), (&SessionId::from_uuid(2), None));
        notifier.notify(&settings, &id, Reason::Question, " ", to);
        notifier.notify(&settings, &id, Reason::TurnFailed, "修复构建", to);
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
