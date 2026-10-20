//! Session status (design 5.2.9) as a pure core: what the sessions note,
//! read marks and presence go in; summaries to upsert, read marks to store
//! and notification intents come out.
//!
//! - Status, from the top: `needs_action` while a request is pending,
//!   `running` while a turn runs, `error` while the last turn that ended
//!   failed and is unread, `unread` while a turn ended after the read mark,
//!   else `idle`.
//! - The read mark only moves forward, and never past the last turn end the
//!   index knows: unread and error look at nothing else.
//! - Presence counts only from `desktop_ui` (security #16).
//! - Changes gather for [`COALESCE`] from the first one, then every changed
//!   summary is written at once. An intent is decided when its change comes
//!   in and written with them, unless by then a foreground `desktop_ui`
//!   watches the session, or, for `needs_action`, nothing is pending any
//!   more.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Duration;

use cupper_protocol::annotations::TimestampMs;
use cupper_protocol::domain::{FailureCode, PrincipalKind};
use cupper_protocol::ids::{ConnId, RequestId, SessionId};
use cupper_protocol::index::{IntentReason, PendingKind, SessionStatus, SessionSummary};
use cupper_protocol::session::TurnStatus;

use crate::clock::Now;
use crate::orchestrator::session::Note;
use crate::pure::{Core, TimerRequest};

/// How long changes gather before their summaries are written.
pub(super) const COALESCE: Duration = Duration::from_millis(100);

/// The core's one timer: write what changed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct Flush;

#[derive(Debug)]
pub(super) enum Input {
    Created(Box<SessionSummary>),
    /// A session's notes, committed at `at` (wall ms).
    Notes(SessionId, u64, Vec<Note>),
    /// `session/mark_read`.
    MarkRead(SessionId, u64),
    /// `client/presence` from connection `conn`.
    Presence {
        conn: ConnId,
        kind: PrincipalKind,
        foreground: bool,
        session: Option<SessionId>,
    },
    /// The connection closed.
    Closed(ConnId),
    Flush,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Action {
    Upsert(Box<SessionSummary>),
    Intent(Intent),
    /// Store the session's read mark (`sessions.last_read_seq`).
    Read(SessionId, u64),
    Timer(TimerRequest<Flush>),
}

/// A `notification.intent` but its ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Intent {
    pub session: SessionId,
    pub reason: IntentReason,
    pub request: Option<RequestId>,
}

/// A session as its row has it, at start and after `test/seed`.
#[derive(Debug, Clone)]
pub(super) struct Loaded {
    /// Its `status`, `unread`, `pending_kind` and `error_code` are computed
    /// again.
    pub summary: SessionSummary,
    pub running: bool,
    /// `last_turn_end_seq` and `last_read_seq`.
    pub ended: u64,
    pub read: u64,
    /// The last turn failed.
    pub failed: bool,
    pub oldest: Option<(PendingKind, RequestId)>,
}

#[derive(Debug)]
struct Entry {
    /// The fields the status does not decide.
    base: SessionSummary,
    /// What the index stream has.
    shown: Option<SessionSummary>,
    running: bool,
    pending: u32,
    oldest: Option<(PendingKind, RequestId)>,
    ended: u64,
    read: u64,
    failed: bool,
    code: Option<FailureCode>,
}

impl Entry {
    fn created(base: SessionSummary) -> Self {
        Self {
            base,
            shown: None,
            running: false,
            pending: 0,
            oldest: None,
            ended: 0,
            read: 0,
            failed: false,
            code: None,
        }
    }

    /// Takes in `note`; the intent it calls for, if any.
    fn apply(&mut self, id: &SessionId, note: Note) -> Option<Intent> {
        let (reason, request) = match note {
            Note::TurnStarted => {
                self.running = true;
                return None;
            }
            Note::TurnEnded { seq, status, error } => {
                self.running = false;
                self.ended = seq;
                self.failed = status == TurnStatus::Failed;
                self.code = error;
                match status {
                    TurnStatus::Completed => (IntentReason::TurnCompleted, None),
                    TurnStatus::Failed => (IntentReason::Error, None),
                    _ => return None,
                }
            }
            Note::Pending { count, oldest } => {
                let entered = self.pending == 0 && count > 0;
                let request = oldest.as_ref().map(|(_, request)| request.clone());
                self.pending = count;
                self.oldest = oldest;
                if !entered {
                    return None;
                }
                (IntentReason::NeedsAction, request)
            }
            Note::Branch(branch) => {
                self.base.branch = branch;
                return None;
            }
            Note::ReadOnly(read_only) => {
                self.base.read_only = read_only;
                return None;
            }
        };
        let session = id.clone();
        Some(Intent {
            session,
            reason,
            request,
        })
    }

    fn summary(&self) -> SessionSummary {
        let unread = self.ended > self.read;
        let status = status_of(self.pending, self.running, unread, self.failed);
        SessionSummary {
            error_code: self.code.clone().filter(|_| status == SessionStatus::Error),
            status,
            unread,
            pending_interactions: self.pending,
            pending_kind: self.oldest.as_ref().map(|(kind, _)| kind.clone()),
            ..self.base.clone()
        }
    }
}

/// A session's status from its pending requests, whether a turn runs,
/// whether a turn ended after the read mark, and whether the last turn that
/// ended failed; `session/get` computes it the same way.
pub(crate) fn status_of(pending: u32, running: bool, unread: bool, failed: bool) -> SessionStatus {
    if pending > 0 {
        SessionStatus::NeedsAction
    } else if running {
        SessionStatus::Running
    } else if unread && failed {
        SessionStatus::Error
    } else if unread {
        SessionStatus::Unread
    } else {
        SessionStatus::Idle
    }
}

#[derive(Debug, Default)]
pub(super) struct Status {
    sessions: BTreeMap<SessionId, Entry>,
    /// The session each foreground `desktop_ui` connection shows.
    watching: HashMap<ConnId, SessionId>,
    changed: BTreeSet<SessionId>,
    intents: Vec<Intent>,
    armed: bool,
}

impl Status {
    /// Takes in the sessions it does not know yet from `rows`, and returns
    /// the summaries that differ from what `view` (the index stream's
    /// sessions) has, to write now.
    pub fn load(
        &mut self,
        rows: Vec<Loaded>,
        view: &BTreeMap<SessionId, SessionSummary>,
    ) -> Vec<SessionSummary> {
        let mut upserts = Vec::new();
        for row in rows {
            let id = row.summary.session_id.clone();
            if self.sessions.contains_key(&id) {
                continue;
            }
            let shown = view.get(&id).cloned();
            let mut entry = Entry {
                code: shown.as_ref().and_then(|s| s.error_code.clone()),
                pending: row.summary.pending_interactions,
                base: row.summary,
                shown,
                running: row.running,
                oldest: row.oldest,
                ended: row.ended,
                read: row.read,
                failed: row.failed,
            };
            let summary = entry.summary();
            if entry.shown.as_ref() != Some(&summary) {
                entry.shown = Some(summary.clone());
                upserts.push(summary);
            }
            self.sessions.insert(id, entry);
        }
        upserts
    }

    /// A session the index does not know hears nothing: it was archived.
    fn notes(&mut self, id: SessionId, at: u64, notes: Vec<Note>) {
        let Some(entry) = self.sessions.get_mut(&id) else {
            return;
        };
        for note in notes {
            self.intents.extend(entry.apply(&id, note));
        }
        let latest = entry.base.last_activity_at.0.max(at);
        entry.base.last_activity_at = TimestampMs(latest);
        self.changed.insert(id);
    }

    fn flush(&mut self) -> Vec<Action> {
        self.armed = false;
        let mut actions = Vec::new();
        for id in std::mem::take(&mut self.changed) {
            let Some(entry) = self.sessions.get_mut(&id) else {
                continue;
            };
            let summary = entry.summary();
            if entry.shown.as_ref() != Some(&summary) {
                entry.shown = Some(summary.clone());
                actions.push(Action::Upsert(Box::new(summary)));
            }
        }
        for intent in std::mem::take(&mut self.intents) {
            let watched = self.watching.values().any(|id| *id == intent.session);
            let pending = self.sessions.get(&intent.session).map(|e| e.pending);
            let moot = intent.reason == IntentReason::NeedsAction && pending == Some(0);
            if !watched && !moot {
                actions.push(Action::Intent(intent));
            }
        }
        actions
    }
}

impl Core for Status {
    type Input = Input;
    type Action = Action;
    type TimerKey = Flush;

    fn step(&mut self, input: Input, now: Now) -> Vec<Action> {
        let mut actions = Vec::new();
        match input {
            Input::Created(summary) => {
                let id = summary.session_id.clone();
                self.sessions.insert(id.clone(), Entry::created(*summary));
                self.changed.insert(id);
            }
            Input::Notes(id, at, notes) => self.notes(id, at, notes),
            Input::MarkRead(id, up_to) => {
                if let Some(entry) = self.sessions.get_mut(&id) {
                    let read = up_to.min(entry.ended);
                    if read > entry.read {
                        entry.read = read;
                        actions.push(Action::Read(id.clone(), read));
                        self.changed.insert(id);
                    }
                }
            }
            Input::Presence {
                conn,
                kind,
                foreground,
                session,
            } => match session.filter(|_| foreground && kind == PrincipalKind::DesktopUi) {
                Some(session) => drop(self.watching.insert(conn, session)),
                None => drop(self.watching.remove(&conn)),
            },
            Input::Closed(conn) => drop(self.watching.remove(&conn)),
            Input::Flush => return self.flush(),
        }
        let due = !self.changed.is_empty() || !self.intents.is_empty();
        if due && !self.armed {
            self.armed = true;
            let at = now.mono + COALESCE;
            actions.push(Action::Timer(TimerRequest::Set { key: Flush, at }));
        }
        actions
    }

    fn timer_request(action: &Action) -> Option<TimerRequest<Flush>> {
        match action {
            Action::Timer(request) => Some(request.clone()),
            _ => None,
        }
    }

    fn timer_expired(_: Flush) -> Input {
        Input::Flush
    }
}

#[cfg(test)]
mod tests {
    use cupper_protocol::domain::AgentKind;
    use cupper_protocol::ids::ProjectId;
    use cupper_protocol::index::{TitleSource, WorkspaceKind};

    use super::*;
    use crate::clock::MonoTime;
    use crate::pure::{CoreDriver, Step};

    use SessionStatus::{Error, Idle, NeedsAction, Running, Unread};

    fn id(n: u128) -> SessionId {
        SessionId::from_uuid(n)
    }

    fn summary(n: u128) -> SessionSummary {
        SessionSummary {
            session_id: id(n),
            project_id: ProjectId::from_uuid(9),
            agent: AgentKind::Claude,
            title: "Fix the build".into(),
            title_source: TitleSource::Prompt,
            workspace_kind: WorkspaceKind::Local,
            branch: None,
            status: Idle,
            unread: false,
            pending_interactions: 0,
            pending_kind: None,
            last_activity_at: TimestampMs(1),
            error_code: None,
            read_only: false,
        }
    }

    fn notes(notes: Vec<Note>) -> Input {
        Input::Notes(id(1), 2, notes)
    }

    fn ended(seq: u64, status: TurnStatus) -> Note {
        let failed = status == TurnStatus::Failed;
        let error = failed.then_some(FailureCode::NotLoggedIn);
        Note::TurnEnded { seq, status, error }
    }

    fn pending(count: u32, oldest: Option<PendingKind>) -> Note {
        let oldest = oldest.map(|kind| (kind, RequestId::from_uuid(u128::from(count))));
        Note::Pending { count, oldest }
    }

    type Shown = (
        SessionStatus,
        bool,
        Option<PendingKind>,
        Option<FailureCode>,
    );

    /// What the summaries say once `input`'s changes are written.
    fn shown(driver: &mut CoreDriver<Status>, input: Input) -> Vec<Shown> {
        driver.input(input);
        let actions = driver.advance(COALESCE).into_iter();
        let upserts = actions.filter_map(|action| match action {
            Action::Upsert(s) => Some((s.status, s.unread, s.pending_kind, s.error_code)),
            _ => None,
        });
        upserts.collect()
    }

    fn reads(actions: Vec<Action>) -> Vec<u64> {
        let reads = actions.into_iter().filter_map(|action| match action {
            Action::Read(_, seq) => Some(seq),
            _ => None,
        });
        reads.collect()
    }

    #[test]
    fn unit_index_status_follows_the_priority_and_the_oldest_request() {
        let mut d = CoreDriver::new(Status::default());
        let created = Input::Created(Box::new(summary(1)));
        assert_eq!(shown(&mut d, created), [(Idle, false, None, None)]);
        let started = notes(vec![Note::TurnStarted]);
        assert_eq!(shown(&mut d, started), [(Running, false, None, None)]);
        let asked = notes(vec![pending(1, Some(PendingKind::Approval))]);
        let approval = Some(PendingKind::Approval);
        assert_eq!(shown(&mut d, asked), [(NeedsAction, false, approval, None)]);
        // The oldest request was answered; the one left asks for input.
        let input = notes(vec![pending(1, Some(PendingKind::Input))]);
        let input_kind = Some(PendingKind::Input);
        assert_eq!(
            shown(&mut d, input),
            [(NeedsAction, false, input_kind, None)]
        );
        let answered = notes(vec![pending(0, None)]);
        assert_eq!(shown(&mut d, answered), [(Running, false, None, None)]);
        let failed = notes(vec![ended(10, TurnStatus::Failed)]);
        let code = Some(FailureCode::NotLoggedIn);
        assert_eq!(shown(&mut d, failed), [(Error, true, None, code)]);
        // Seen: no longer an error.
        assert_eq!(
            shown(&mut d, Input::MarkRead(id(1), 10)),
            [(Idle, false, None, None)]
        );
        // A failure, then a successful turn: unread, not an error.
        let again = notes(vec![Note::TurnStarted, ended(20, TurnStatus::Failed)]);
        assert_eq!(shown(&mut d, again)[0].0, Error);
        let success = notes(vec![Note::TurnStarted, ended(30, TurnStatus::Completed)]);
        assert_eq!(shown(&mut d, success), [(Unread, true, None, None)]);
        // A request outranks a running turn, which outranks unread.
        let both = notes(vec![
            Note::TurnStarted,
            pending(1, Some(PendingKind::Approval)),
        ]);
        assert_eq!(shown(&mut d, both)[0].0, NeedsAction);
        assert_eq!(shown(&mut d, notes(vec![pending(0, None)]))[0].0, Running);
    }

    #[test]
    fn unit_index_status_read_marks_move_forward_up_to_the_last_turn_end() {
        let mut d = CoreDriver::new(Status::default());
        d.input(Input::Created(Box::new(summary(1))));
        d.input(notes(vec![
            Note::TurnStarted,
            ended(10, TurnStatus::Completed),
        ]));
        assert_eq!(reads(d.input(Input::MarkRead(id(1), 50))), [10]);
        for back in [10, 4] {
            assert!(reads(d.input(Input::MarkRead(id(1), back))).is_empty());
        }
        assert!(reads(d.input(Input::MarkRead(id(2), 50))).is_empty());
        d.input(notes(vec![
            Note::TurnStarted,
            ended(20, TurnStatus::Completed),
        ]));
        assert_eq!(reads(d.input(Input::MarkRead(id(1), 15))), [15]);
        // Not up to the turn's end: still unread.
        let written = d.advance(COALESCE);
        assert!(matches!(&written[..], [Action::Upsert(s), ..] if s.status == Unread && s.unread));
    }

    #[test]
    fn unit_index_status_changes_within_100_ms_are_written_once() {
        let mut d = CoreDriver::new(Status::default());
        let ms = Duration::from_millis;
        let script = [
            Step::Input(Input::Created(Box::new(summary(1)))),
            Step::Advance(ms(40)),
            Step::Input(notes(vec![Note::TurnStarted])),
            Step::Advance(ms(40)),
            Step::Input(notes(vec![pending(1, Some(PendingKind::Approval))])),
            Step::Advance(ms(100)),
            // Back to what was written: nothing to write.
            Step::Input(notes(vec![pending(0, None)])),
            Step::Input(notes(vec![pending(1, Some(PendingKind::Approval))])),
            Step::Advance(ms(100)),
            Step::Input(notes(vec![pending(0, None)])),
            Step::Advance(ms(100)),
        ];
        let at = |ms| MonoTime::from_origin(Duration::from_millis(ms));
        let written: Vec<_> = (d.run(script).into_iter())
            .filter_map(|(when, action)| match action {
                Action::Upsert(s) => Some((when, s.status)),
                _ => None,
            })
            .collect();
        assert_eq!(written, [(at(100), NeedsAction), (at(380), Running)]);
    }

    #[test]
    fn unit_index_status_starts_from_the_rows_and_keeps_what_it_knows() {
        let mut status = Status::default();
        let row = Loaded {
            summary: summary(1),
            running: false,
            ended: 8,
            read: 3,
            failed: true,
            oldest: None,
        };
        // The stream has the live summary, with the failure's code.
        let live = SessionSummary {
            status: Error,
            unread: true,
            error_code: Some(FailureCode::NotLoggedIn),
            ..summary(1)
        };
        let view = BTreeMap::from([(id(1), live)]);
        assert_eq!(status.load(vec![row.clone()], &view), []);
        let asking = Loaded {
            summary: SessionSummary {
                pending_interactions: 2,
                ..summary(2)
            },
            failed: false,
            oldest: Some((PendingKind::Input, RequestId::from_uuid(3))),
            ..row.clone()
        };
        // Session 1 is known already; session 2 is new to the stream.
        let written = status.load(vec![row, asking], &view);
        let written: Vec<_> = (written.iter())
            .map(|s| (&s.session_id, &s.status, &s.pending_kind))
            .collect();
        assert_eq!(written, [(&id(2), &NeedsAction, &Some(PendingKind::Input))]);
    }
}
