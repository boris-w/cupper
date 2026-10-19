//! Drives a pure core through inputs and virtual time, for tests.

use std::time::Duration;

use super::{Core, TimerTable};
use crate::clock::{Clock, ManualClock, MonoTime, Now};

/// One entry of a script for [`CoreDriver::run`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step<I> {
    /// Step the core with this input at the current time.
    Input(I),
    /// Let virtual time pass, firing the core's timers as they come due.
    Advance(Duration),
}

/// Runs a [`Core`] the way its actor would, on a [`ManualClock`] that only this
/// driver moves. The core's timer requests take effect as in production: a
/// timer fires as an input at its deadline, in deadline order, and a cancelled
/// or moved timer does not fire. Every action is also returned, so a test can
/// assert on timer requests too.
#[derive(Debug)]
pub struct CoreDriver<C: Core> {
    core: C,
    clock: ManualClock,
    timers: TimerTable<C::TimerKey>,
}

impl<C: Core> CoreDriver<C> {
    pub fn new(core: C) -> Self {
        Self::with_clock(core, ManualClock::new())
    }

    /// Starts from `clock`'s time. Share the clock with an
    /// [`IdGen`](crate::clock::IdGen) to make the core's IDs reproducible, but
    /// advance it only through this driver.
    pub fn with_clock(core: C, clock: ManualClock) -> Self {
        Self {
            core,
            clock,
            timers: TimerTable::new(),
        }
    }

    pub fn core(&self) -> &C {
        &self.core
    }

    pub fn now(&self) -> Now {
        self.clock.now()
    }

    /// The timers the core has set and that have not fired yet.
    pub fn timers(&self) -> &TimerTable<C::TimerKey> {
        &self.timers
    }

    /// Steps the core with `input` at the current time.
    pub fn input(&mut self, input: C::Input) -> Vec<C::Action> {
        self.step(input)
    }

    /// Moves time forward by `by`, firing each timer that comes due at its own
    /// deadline. Timers set by those steps fire too if they come due in time;
    /// timers already due fire with `Duration::ZERO`.
    pub fn advance(&mut self, by: Duration) -> Vec<C::Action> {
        self.advance_stamped(by)
            .into_iter()
            .map(|(_, action)| action)
            .collect()
    }

    /// Plays `script` and returns every action with the time it was taken.
    pub fn run(
        &mut self,
        script: impl IntoIterator<Item = Step<C::Input>>,
    ) -> Vec<(MonoTime, C::Action)> {
        let mut transcript = Vec::new();
        for step in script {
            match step {
                Step::Input(input) => {
                    let at = self.clock.now_mono();
                    transcript.extend(self.step(input).into_iter().map(|action| (at, action)));
                }
                Step::Advance(by) => transcript.extend(self.advance_stamped(by)),
            }
        }
        transcript
    }

    fn advance_stamped(&mut self, by: Duration) -> Vec<(MonoTime, C::Action)> {
        let target = self.clock.now_mono() + by;
        let mut out = Vec::new();
        while let Some(deadline) = self.timers.next_deadline().filter(|at| *at <= target) {
            let at = deadline.max(self.clock.now_mono());
            self.clock.advance_to(at);
            if let Some(key) = self.timers.pop_due(at) {
                let actions = self.step(C::timer_expired(key));
                out.extend(actions.into_iter().map(|action| (at, action)));
            }
        }
        self.clock.advance_to(target);
        out
    }

    fn step(&mut self, input: C::Input) -> Vec<C::Action> {
        let actions = self.core.step(input, self.clock.now());
        for action in &actions {
            if let Some(request) = C::timer_request(action) {
                self.timers.apply(request);
            }
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pure::TimerRequest;

    /// A toy coalescer. The first chunk of a message goes out at once; later
    /// chunks are held until 40 ms after the first held one, or until `End`.
    /// Every input but the idle timer itself restarts a 100 ms idle timer.
    #[derive(Debug, Default)]
    struct Toy {
        open: bool,
        held: String,
    }

    #[derive(Debug)]
    enum In {
        Chunk(&'static str),
        End,
        Expired(Key),
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum Key {
        Flush,
        Idle,
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Out {
        Emit(String),
        Idle,
        Timer(TimerRequest<Key>),
    }

    impl Toy {
        fn flush(&mut self) -> Option<Out> {
            (!self.held.is_empty()).then(|| Out::Emit(std::mem::take(&mut self.held)))
        }
    }

    impl Core for Toy {
        type Input = In;
        type Action = Out;
        type TimerKey = Key;

        fn step(&mut self, input: In, now: Now) -> Vec<Out> {
            let set = |key, after| {
                Out::Timer(TimerRequest::Set {
                    key,
                    at: now.mono + after,
                })
            };
            let mut out = Vec::new();
            match input {
                In::Chunk(text) if !self.open => {
                    self.open = true;
                    out.push(Out::Emit(text.to_owned()));
                }
                In::Chunk(text) => {
                    if self.held.is_empty() {
                        out.push(set(Key::Flush, ms(40)));
                    }
                    self.held.push_str(text);
                }
                In::End => {
                    self.open = false;
                    out.extend(self.flush());
                    out.push(Out::Timer(TimerRequest::Cancel(Key::Flush)));
                }
                In::Expired(Key::Flush) => out.extend(self.flush()),
                In::Expired(Key::Idle) => return vec![Out::Idle],
            }
            out.push(set(Key::Idle, ms(100)));
            out
        }

        fn timer_request(action: &Out) -> Option<TimerRequest<Key>> {
            match action {
                Out::Timer(request) => Some(request.clone()),
                _ => None,
            }
        }

        fn timer_expired(key: Key) -> In {
            In::Expired(key)
        }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn at(n: u64) -> MonoTime {
        MonoTime::ZERO + ms(n)
    }

    fn set(key: Key, deadline: u64) -> Out {
        Out::Timer(TimerRequest::Set {
            key,
            at: at(deadline),
        })
    }

    #[test]
    fn core_driver_fires_timers_at_their_deadlines_in_order() {
        let mut driver = CoreDriver::new(Toy::default());
        let transcript = driver.run([
            Step::Input(In::Chunk("a")),
            Step::Advance(ms(10)),
            Step::Input(In::Chunk("b")),
            Step::Advance(ms(10)),
            Step::Input(In::Chunk("c")),
            Step::Advance(ms(200)),
        ]);
        // The idle timer moves on every input and fires once, at its last
        // deadline, which a timer-driven step set during the same advance.
        assert_eq!(
            transcript,
            [
                (at(0), Out::Emit("a".to_owned())),
                (at(0), set(Key::Idle, 100)),
                (at(10), set(Key::Flush, 50)),
                (at(10), set(Key::Idle, 110)),
                (at(20), set(Key::Idle, 120)),
                (at(50), Out::Emit("bc".to_owned())),
                (at(50), set(Key::Idle, 150)),
                (at(150), Out::Idle),
            ]
        );
        let now = driver.now();
        assert_eq!(now.mono, at(220));
        assert_eq!(now.wall_ms, ManualClock::DEFAULT_START_WALL_MS + 220);
        assert!(driver.timers().is_empty());
        assert!(driver.core().held.is_empty());
    }

    #[test]
    fn core_driver_cancelled_timer_does_not_fire() {
        let mut driver = CoreDriver::new(Toy::default());
        driver.input(In::Chunk("a"));
        driver.input(In::Chunk("b"));
        assert_eq!(
            driver.input(In::End),
            [
                Out::Emit("b".to_owned()),
                Out::Timer(TimerRequest::Cancel(Key::Flush)),
                set(Key::Idle, 100),
            ]
        );
        assert_eq!(driver.advance(ms(99)), []);
        assert_eq!(driver.advance(ms(1)), [Out::Idle]);
        assert_eq!(driver.advance(Duration::ZERO), []);
    }
}
