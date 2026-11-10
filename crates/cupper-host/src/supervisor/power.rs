//! Keeping the Mac awake (design 5.2.13, NFR-13): while a turn runs or a
//! request waits for the user, the host holds a `PreventUserIdleSystemSleep`
//! assertion, and releases it once neither is left, or once the user turns
//! `host.prevent_idle_sleep` off (T147). Each session counts its
//! turn and its pending requests into a [`Share`] as they start and end; the
//! supervisor's actor carries out what [`PowerCore`] decides, and
//! `host.updated` says whether it holds. The orderly exit releases it for
//! good ([`Power::exit`]); a host that dies holds nothing, as the system
//! releases a process's assertions with it.

use std::convert::Infallible;
use std::mem;

use cupper_sys::IdleSleepAssertion;
use tokio::sync::{mpsc, oneshot};

use crate::clock::Now;
use crate::index::IndexHandle;
use crate::pure::{Core, TimerRequest};

/// What `pmset -g assertions` shows.
pub const NAME: &str = "cupper: agents at work";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// A session's running turns and pending requests went from `from` to `to`.
    Changed { from: u32, to: u32 },
    /// `host.prevent_idle_sleep`, at start and on each settings update.
    Enabled(bool),
    /// The orderly exit: released for good.
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Take,
    Release,
}

/// The running turns and pending requests of all sessions, and whether the
/// user turned the setting off.
#[derive(Debug, Default)]
pub struct PowerCore {
    count: u64,
    disabled: bool,
    exited: bool,
}

impl PowerCore {
    fn holds(&self) -> bool {
        self.count > 0 && !self.disabled && !self.exited
    }
}

impl Core for PowerCore {
    type Input = Input;
    type Action = Action;
    type TimerKey = Infallible;

    fn step(&mut self, input: Input, _now: Now) -> Vec<Action> {
        let held = self.holds();
        match input {
            Input::Changed { from, to } => {
                self.count = self.count.saturating_sub(from.into()) + u64::from(to);
            }
            Input::Enabled(enabled) => self.disabled = !enabled,
            Input::Exit => self.exited = true,
        }
        match (held, self.holds()) {
            (false, true) => vec![Action::Take],
            (true, false) => vec![Action::Release],
            _ => Vec::new(),
        }
    }

    fn timer_request(_: &Action) -> Option<TimerRequest<Infallible>> {
        None
    }

    fn timer_expired(key: Infallible) -> Input {
        match key {}
    }
}

/// The way into the supervisor's assertion. Clones share it.
#[derive(Debug, Clone)]
pub struct Power(mpsc::UnboundedSender<Mail>);

/// An input for the supervisor's actor, and who waits for it to be done.
#[derive(Debug)]
pub struct Mail(pub Input, Option<oneshot::Sender<()>>);

impl Power {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<Mail>) {
        let (mail, inbox) = mpsc::unbounded_channel();
        (Self(mail), inbox)
    }

    /// A session's share, none of the count yet.
    pub fn share(&self) -> Share {
        let power = self.clone();
        Share { count: 0, power }
    }

    /// `host.prevent_idle_sleep`: off releases at once (design 5.2.13).
    pub fn enabled(&self, enabled: bool) {
        let _ = self.0.send(Mail(Input::Enabled(enabled), None));
    }

    /// Releases the assertion for good, in the orderly exit (design 5.2.13).
    pub async fn exit(&self) {
        let (done, released) = oneshot::channel();
        let _ = self.0.send(Mail(Input::Exit, Some(done)));
        // An actor that is gone holds nothing.
        let _ = released.await;
    }
}

/// A session's share of the count. Dropped, it gives its part back.
#[derive(Debug)]
pub struct Share {
    count: u32,
    power: Power,
}

impl Share {
    pub fn set(&mut self, count: u32) {
        if count != self.count {
            let from = mem::replace(&mut self.count, count);
            let changed = Input::Changed { from, to: count };
            let _ = self.power.0.send(Mail(changed, None));
        }
    }
}

impl Drop for Share {
    fn drop(&mut self) {
        self.set(0);
    }
}

/// The actor's side: the core and the assertion it holds.
#[derive(Debug, Default)]
pub struct Holder {
    core: PowerCore,
    held: Option<IdleSleepAssertion>,
}

impl Holder {
    /// Takes or releases as the core decides; `host.updated` says what holds.
    pub fn handle(&mut self, Mail(input, done): Mail, now: Now, index: &IndexHandle) {
        for action in self.core.step(input, now) {
            self.held = match action {
                Action::Take => IdleSleepAssertion::take(NAME)
                    .inspect_err(|err| tracing::warn!(%err, "cannot keep the Mac awake"))
                    .ok(),
                Action::Release => None,
            };
            index.power_assertion(self.held.is_some());
        }
        if let Some(done) = done {
            let _ = done.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MonoTime;

    fn changed(from: u32, to: u32) -> Input {
        Input::Changed { from, to }
    }

    fn run(inputs: &[Input]) -> Vec<Vec<Action>> {
        let mut core = PowerCore::default();
        let now = Now {
            mono: MonoTime::ZERO,
            wall_ms: 0,
        };
        inputs.iter().map(|input| core.step(*input, now)).collect()
    }

    #[test]
    fn power_core_holds_from_the_first_turn_or_request_to_the_last() {
        use Action::*;
        let none = Vec::new;
        // A turn starts, its request comes and goes, another session's turn
        // starts and ends, then the first turn ends.
        let steps = [changed(0, 1), changed(1, 2), changed(2, 1), changed(0, 1)];
        let steps = [&steps[..], &[changed(1, 0), changed(1, 0)]].concat();
        let want = [vec![Take], none(), none(), none(), none(), vec![Release]];
        assert_eq!(run(&steps), want);
        // A request alone holds as a turn does; a share that comes back with
        // nothing does nothing.
        let steps = [changed(0, 0), changed(0, 1), changed(1, 0)];
        assert_eq!(run(&steps), [none(), vec![Take], vec![Release]]);
    }

    #[test]
    fn power_core_holds_only_while_the_setting_is_on() {
        use Action::*;
        let none = Vec::new;
        let (on, off) = (Input::Enabled(true), Input::Enabled(false));
        // Off before the turn: nothing held; on while it runs: held at once.
        let steps = [off, changed(0, 1), on, on];
        assert_eq!(run(&steps), [none(), none(), vec![Take], none()]);
        // Off while it runs: released at once; on after it: nothing.
        let steps = [changed(0, 1), off, off, changed(1, 0), on];
        assert_eq!(
            run(&steps),
            [vec![Take], vec![Release], none(), none(), none()]
        );
    }

    #[test]
    fn power_core_releases_at_the_exit_for_good() {
        use Action::*;
        let steps = [changed(0, 1), Input::Exit, changed(1, 2), changed(2, 0)];
        let want = [vec![Take], vec![Release], vec![], vec![]];
        assert_eq!(run(&steps), want);
        // Nothing held, nothing to release.
        assert_eq!(run(&[Input::Exit, changed(0, 1)]), [vec![], vec![]]);
    }
}
