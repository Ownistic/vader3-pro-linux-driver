//! Bounded, experimental ordering for Steam's Guide + A shortcut.
use std::time::Duration;

use crate::input::{button, State};

// Phase durations are minimum report intervals, not measured end-to-end latency.
const GUIDE_LEAD: Duration = Duration::from_millis(100);
const CHORD: Duration = Duration::from_millis(60);
const GUIDE_TAIL: Duration = Duration::from_millis(40);

#[derive(Clone, Copy, Default)]
enum Phase {
    #[default]
    Idle,
    Lead(Duration),
    Chord(Duration),
    Tail(Duration),
}

#[derive(Default)]
pub(crate) struct QuickAccess {
    phase: Phase,
    armed: bool,
}

impl QuickAccess {
    pub(crate) fn cancel(&mut self) {
        self.phase = Phase::Idle;
        self.armed = false;
    }

    // Called once per report, after the UHID START guard. A phase's clock
    // begins when that phase is first emitted, not at an inferred deadline.
    pub(crate) fn compose(
        &mut self,
        physical: &State,
        valid: bool,
        eligible: bool,
        now: Duration,
    ) -> State {
        let mut output = *physical;
        if !valid || !eligible {
            self.cancel();
            return output;
        }
        let held = physical.button(button::FN);
        self.phase = match self.phase {
            Phase::Idle if !held => {
                self.armed = true;
                Phase::Idle
            }
            Phase::Idle if self.armed => {
                self.armed = false;
                Phase::Lead(now)
            }
            Phase::Lead(start) if now.saturating_sub(start) >= GUIDE_LEAD => Phase::Chord(now),
            Phase::Chord(start) if now.saturating_sub(start) >= CHORD => Phase::Tail(now),
            Phase::Tail(start) if now.saturating_sub(start) >= GUIDE_TAIL => Phase::Idle,
            phase => phase,
        };
        match self.phase {
            Phase::Lead(_) | Phase::Tail(_) => output.buttons |= 1 << button::HOME,
            Phase::Chord(_) => output.buttons |= (1 << button::HOME) | (1 << button::A),
            Phase::Idle => {}
        }
        output
    }
}
