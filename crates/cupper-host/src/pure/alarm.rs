//! Step 3 of the adapter loop: one [`Clock`] timer for a [`TimerTable`]'s
//! next deadline.

use std::future;
use std::sync::Arc;

use super::TimerTable;
use crate::clock::{Clock, MonoTime, Timer};

/// The one clock timer an actor keeps armed for its cores' timers.
#[derive(Debug)]
pub struct Alarm {
    clock: Arc<dyn Clock>,
    timer: Option<Timer>,
}

impl Alarm {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self { clock, timer: None }
    }

    /// Arms the timer for the table's next deadline after a step, unless it
    /// is armed for that deadline already; with no deadline, disarms it.
    pub fn follow<K: Clone + Eq + std::hash::Hash>(&mut self, table: &TimerTable<K>) {
        let next = table.next_deadline();
        if self.timer.as_ref().map(Timer::deadline) != next {
            self.timer = next.map(|at| self.clock.timer_at(at));
        }
    }

    /// Completes when the armed timer fires, and disarms it; pending while
    /// disarmed. Cancel-safe: dropped unfinished, the timer stays armed.
    pub async fn rung(&mut self) -> MonoTime {
        let Some(timer) = &mut self.timer else {
            return future::pending().await;
        };
        let at = timer.deadline();
        (&mut *timer).await;
        self.timer = None;
        at
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures::FutureExt;

    use super::*;
    use crate::clock::ManualClock;
    use crate::pure::TimerRequest;

    fn at(ms: u64) -> MonoTime {
        MonoTime::from_origin(Duration::from_millis(ms))
    }

    #[test]
    fn alarm_follows_the_next_deadline_of_its_table() {
        let clock = ManualClock::new();
        let mut alarm = Alarm::new(Arc::new(clock.clone()));
        let mut table = TimerTable::new();
        alarm.follow(&table);
        assert_eq!(alarm.rung().now_or_never(), None);
        for (key, ms) in [("late", 30), ("soon", 10)] {
            table.apply(TimerRequest::Set { key, at: at(ms) });
        }
        alarm.follow(&table);
        assert_eq!(clock.next_deadline(), Some(at(10)));
        clock.advance(Duration::from_millis(9));
        assert_eq!(alarm.rung().now_or_never(), None);
        clock.advance(Duration::from_millis(1));
        assert_eq!(alarm.rung().now_or_never(), Some(at(10)));
        // Fired once; the table decides what is next.
        assert_eq!(alarm.rung().now_or_never(), None);
        assert_eq!(table.pop_due(at(10)), Some("soon"));
        table.apply(TimerRequest::Cancel("late"));
        alarm.follow(&table);
        assert_eq!(clock.next_deadline(), None);
    }
}
