//! The installer's pure core (design 5.2.12, 5.2.16): each component goes
//! `missing` → `installing` → `ready`, or ends `failed{reason}`. A component
//! with dependencies starts once they are ready, and fails with them. Only a
//! `network` failure is tried again, after 1, 2 and 4 times the retry
//! duration. The core does no I/O; [`Installer`](super::Installer) carries
//! out its actions and reports the results back.

use std::collections::BTreeMap;
use std::time::Duration;

use super::files::Verdict;
use super::{Component, Reason, Status, Statuses};
use crate::clock::Now;
use crate::pure::{Core, TimerRequest};

/// Tries after the first.
const RETRIES: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Input {
    Start,
    /// What the component's version directory holds.
    Checked(Component, Verdict),
    /// An install ended.
    Installed(Component, Result<(), Reason>),
    /// The wait before trying again is over.
    Retry(Component),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    /// Remove what a killed run left, then check the version directory.
    Check(Component),
    /// Fill a staging directory and commit it; `replace` moves the broken
    /// version directory aside first.
    Install {
        component: Component,
        replace: bool,
    },
    /// Remove the component's other versions that nothing uses.
    Recycle(Component),
    Timer(TimerRequest<Component>),
}

/// A component being installed.
#[derive(Debug, Clone, Copy)]
struct Job {
    replace: bool,
    started: bool,
    retries: u32,
}

pub(crate) struct InstallCore {
    statuses: Statuses,
    jobs: BTreeMap<Component, Job>,
    retry: Duration,
}

impl InstallCore {
    pub fn new(components: impl IntoIterator<Item = Component>, retry: Duration) -> Self {
        let statuses = components.into_iter().map(|c| (c, Status::Missing));
        Self {
            statuses: statuses.collect(),
            jobs: BTreeMap::new(),
            retry,
        }
    }

    pub fn statuses(&self) -> &Statuses {
        &self.statuses
    }

    /// Starts `c` once its dependencies are ready; fails it when one failed.
    fn start(&mut self, c: Component) -> Vec<Action> {
        let deps = c.deps().iter().filter_map(|d| self.statuses.get(d));
        match deps.copied().find(|status| *status != Status::Ready) {
            Some(Status::Failed(reason)) => self.fail(c, reason),
            Some(_) => Vec::new(),
            None => {
                let job = self.jobs.get_mut(&c).expect("a job is started");
                job.started = true;
                let replace = job.replace;
                vec![Action::Install {
                    component: c,
                    replace,
                }]
            }
        }
    }

    fn end(&mut self, c: Component, status: Status, mut actions: Vec<Action>) -> Vec<Action> {
        self.statuses.insert(c, status);
        self.jobs.remove(&c);
        let waiting = self
            .jobs
            .iter()
            .filter(|(d, job)| !job.started && d.deps().contains(&c));
        let waiting: Vec<Component> = waiting.map(|(d, _)| *d).collect();
        for d in waiting {
            actions.extend(self.start(d));
        }
        actions
    }

    fn fail(&mut self, c: Component, reason: Reason) -> Vec<Action> {
        self.end(c, Status::Failed(reason), Vec::new())
    }
}

impl Core for InstallCore {
    type Input = Input;
    type Action = Action;
    type TimerKey = Component;

    fn step(&mut self, input: Input, now: Now) -> Vec<Action> {
        match input {
            Input::Start => self.statuses.keys().map(|&c| Action::Check(c)).collect(),
            Input::Checked(c, Verdict::Intact) | Input::Installed(c, Ok(())) => {
                self.end(c, Status::Ready, vec![Action::Recycle(c)])
            }
            Input::Checked(c, verdict) => {
                let replace = verdict == Verdict::Broken;
                let (started, retries) = (false, 0);
                let job = Job {
                    replace,
                    started,
                    retries,
                };
                self.jobs.insert(c, job);
                self.statuses.insert(c, Status::Installing);
                self.start(c)
            }
            Input::Installed(c, Err(Reason::Network))
                if self.jobs.get(&c).is_some_and(|job| job.retries < RETRIES) =>
            {
                let job = self.jobs.get_mut(&c).expect("checked above");
                job.retries += 1;
                let at = now.mono + self.retry * 2u32.pow(job.retries - 1);
                vec![Action::Timer(TimerRequest::Set { key: c, at })]
            }
            Input::Installed(c, Err(reason)) => self.fail(c, reason),
            Input::Retry(c) => self.start(c),
        }
    }

    fn timer_request(action: &Action) -> Option<TimerRequest<Component>> {
        match action {
            Action::Timer(request) => Some(request.clone()),
            _ => None,
        }
    }

    fn timer_expired(key: Component) -> Input {
        Input::Retry(key)
    }
}
