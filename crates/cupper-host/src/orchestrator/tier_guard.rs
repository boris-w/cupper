//! `TierGuard` (design 5.2.6 "配置项", security #3, check 4): whether the
//! session's actual approval tier is one its setter may put it in.
//!
//! - The setter is the session's creator, then whoever chose an option that
//!   moves the tier (or, from M1b, set it with `session/set_config`), then
//!   whoever confirmed the tier with `session/confirm_config`.
//! - After an attach, and whenever the agent changes the mode by itself, the
//!   tier is checked: `full_access` without a native confirmation, or a tier
//!   above what the setter may set, puts the session in `config_unconfirmed`.
//! - During a turn such a change is first set back, with 2 s for the agent to
//!   take it; when it does not, or changes the mode again, the turn is
//!   interrupted. Either way the session is then `config_unconfirmed`.

use std::time::Duration;

use cupper_protocol::domain::Tier;
use cupper_protocol::ids::PrincipalId;
use cupper_protocol::methods::handshake::Principal;
use cupper_protocol::session::ConfirmationReason;

use crate::clock::Now;
use crate::durations::{CONFIG_REVERT, Durations};
use crate::policy;
use crate::pure::{Core, TimerRequest};

#[derive(Debug)]
pub struct TierGuard {
    revert_for: Duration,
    setter: Principal,
    /// The session went to `full_access` through a native confirmation.
    confirmed: bool,
    /// The last mode the agent reported, and the last one accepted.
    actual: Option<(String, Tier)>,
    accepted: Option<String>,
    /// A principal chose an option that moves the tier up to this one.
    chosen: Option<(Principal, Tier)>,
    unconfirmed: Option<ConfirmationReason>,
    /// The mode being set back, and why.
    reverting: Option<(String, ConfirmationReason)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// The permission mode after an attach (`during_turn` false), or as the
    /// agent changed it.
    Mode {
        mode: String,
        tier: Tier,
        during_turn: bool,
    },
    /// `by` chose an option, which the policy allowed, that puts the session
    /// in `tier`.
    Set {
        by: Principal,
        tier: Tier,
    },
    /// `session/confirm_config`.
    Confirmed {
        by: Principal,
    },
    Expired(RevertTimer),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Set the permission mode back to this one.
    Revert(String),
    /// Interrupt the running turn.
    Interrupt,
    /// The session enters `config_unconfirmed` for this reason, or leaves it.
    Unconfirmed(Option<ConfirmationReason>),
    /// Who set the tier now (`sessions.mode_set_by`); `confirmed` when
    /// `full_access` was just confirmed (`full_access_confirmed_at`).
    SetBy {
        by: PrincipalId,
        confirmed: bool,
    },
    Timer(TimerRequest<RevertTimer>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RevertTimer;

impl TierGuard {
    /// `confirmed`: the session was created in `full_access`, which takes
    /// the native confirmation.
    pub fn new(durations: &Durations, setter: Principal, confirmed: bool) -> Self {
        Self {
            revert_for: durations.get(CONFIG_REVERT),
            setter,
            confirmed,
            actual: None,
            accepted: None,
            chosen: None,
            unconfirmed: None,
            reverting: None,
        }
    }

    /// Who set the session's tier.
    pub fn setter(&self) -> &PrincipalId {
        &self.setter.id
    }

    /// Why `tier` needs a confirmation with the current setter, if it does.
    fn violation(&self, tier: &Tier) -> Option<ConfirmationReason> {
        let rank = |tier: &Tier| policy::tier_rank(tier);
        if rank(tier) == rank(&Tier::FullAccess) && !self.confirmed {
            Some(ConfirmationReason::FullAccessUnconfirmed)
        } else if rank(tier) > rank(&policy::ceiling(&self.setter)) {
            Some(ConfirmationReason::TierAboveSetter)
        } else {
            None
        }
    }

    fn mode(&mut self, mode: String, tier: Tier, during_turn: bool, now: Now) -> Vec<Action> {
        let mut out = Vec::new();
        // Another option changed; the mode did not (a repeat while a revert
        // is pending is the agent raising it again).
        let same = self
            .actual
            .as_ref()
            .is_some_and(|(actual, _)| *actual == mode);
        if same && self.reverting.is_none() {
            return out;
        }
        self.actual = Some((mode.clone(), tier.clone()));
        // An option the policy allowed makes its chooser the setter.
        let chosen = self.chosen.take();
        let chosen =
            chosen.filter(|(_, up_to)| policy::tier_rank(&tier) <= policy::tier_rank(up_to));
        if let Some((by, _)) = chosen.clone() {
            self.set_by(by, &tier, &mut out);
        }
        let violation = self.violation(&tier);
        if let Some((_, reason)) = self.reverting.take() {
            out.push(Action::Timer(TimerRequest::Cancel(RevertTimer)));
            if violation.is_some() {
                out.push(Action::Interrupt);
            } else {
                self.accepted = Some(mode);
            }
            self.unconfirm(Some(reason), &mut out);
            return out;
        }
        match (violation, &self.accepted) {
            (None, _) => {
                self.accepted = Some(mode);
                if chosen.is_some() {
                    self.unconfirm(None, &mut out);
                }
            }
            (Some(reason), Some(back)) if during_turn => {
                out.push(Action::Revert(back.clone()));
                self.reverting = Some((back.clone(), reason));
                let at = now.mono + self.revert_for;
                out.push(Action::Timer(TimerRequest::Set {
                    key: RevertTimer,
                    at,
                }));
            }
            (Some(reason), _) => self.unconfirm(Some(reason), &mut out),
        }
        out
    }

    fn set_by(&mut self, by: Principal, tier: &Tier, out: &mut Vec<Action>) {
        let full = policy::tier_rank(tier) == policy::tier_rank(&Tier::FullAccess);
        let confirmed = full && !self.confirmed;
        self.confirmed |= full;
        out.push(Action::SetBy {
            by: by.id.clone(),
            confirmed,
        });
        self.setter = by;
    }

    fn unconfirm(&mut self, reason: Option<ConfirmationReason>, out: &mut Vec<Action>) {
        if self.unconfirmed.is_some() != reason.is_some() {
            self.unconfirmed = reason.clone();
            out.push(Action::Unconfirmed(reason));
        }
    }
}

impl Core for TierGuard {
    type Input = Input;
    type Action = Action;
    type TimerKey = RevertTimer;

    fn step(&mut self, input: Input, now: Now) -> Vec<Action> {
        let mut out = Vec::new();
        match input {
            Input::Mode {
                mode,
                tier,
                during_turn,
            } => return self.mode(mode, tier, during_turn, now),
            Input::Set { by, tier } => self.chosen = Some((by, tier)),
            // The actual tier becomes the confirmed one.
            Input::Confirmed { by } if self.unconfirmed.is_some() => {
                if let Some((mode, tier)) = self.actual.clone() {
                    self.set_by(by, &tier, &mut out);
                    self.accepted = Some(mode);
                }
                self.unconfirm(None, &mut out);
            }
            Input::Confirmed { .. } => {}
            Input::Expired(RevertTimer) => {
                if let Some((_, reason)) = self.reverting.take() {
                    out.push(Action::Interrupt);
                    self.unconfirm(Some(reason), &mut out);
                }
            }
        }
        out
    }

    fn timer_request(action: &Action) -> Option<TimerRequest<RevertTimer>> {
        match action {
            Action::Timer(request) => Some(request.clone()),
            _ => None,
        }
    }

    fn timer_expired(key: RevertTimer) -> Input {
        Input::Expired(key)
    }
}
