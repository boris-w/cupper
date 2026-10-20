//! The installer's pure core (design 5.2.12, 5.2.16): each component goes
//! `missing` → `installing` → `ready`, or ends `failed{reason}`. It does no
//! I/O; [`Installer`](super::Installer) carries out its actions and reports
//! the results back.

use super::files::Verdict;
use super::{Component, Reason, Status, Statuses};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Input {
    Start,
    /// What the component's version directory holds.
    Checked(Component, Verdict),
    /// An install ended.
    Installed(Component, Result<(), Reason>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    /// Remove what a killed run left, then check the version directory.
    Check(Component),
    /// Fill a staging directory and commit it; `replace` moves the broken
    /// version directory aside first.
    Install { component: Component, replace: bool },
    /// Remove the component's other versions that nothing uses.
    Recycle(Component),
}

pub(crate) struct InstallCore {
    statuses: Statuses,
}

impl InstallCore {
    pub fn new(components: impl IntoIterator<Item = Component>) -> Self {
        let statuses = components.into_iter().map(|c| (c, Status::Missing));
        Self {
            statuses: statuses.collect(),
        }
    }

    pub fn statuses(&self) -> &Statuses {
        &self.statuses
    }

    pub fn step(&mut self, input: Input) -> Vec<Action> {
        let (component, status, actions) = match input {
            Input::Start => return self.statuses.keys().map(|&c| Action::Check(c)).collect(),
            Input::Checked(c, Verdict::Intact) | Input::Installed(c, Ok(())) => {
                (c, Status::Ready, vec![Action::Recycle(c)])
            }
            Input::Checked(c, verdict) => {
                let replace = verdict == Verdict::Broken;
                let install = Action::Install {
                    component: c,
                    replace,
                };
                (c, Status::Installing, vec![install])
            }
            Input::Installed(c, Err(reason)) => (c, Status::Failed(reason), Vec::new()),
        };
        self.statuses.insert(component, status);
        actions
    }
}
