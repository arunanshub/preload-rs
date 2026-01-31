#![forbid(unsafe_code)]

use std::fmt;

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MarkovState {
    Neither = 0,
    AOnly = 1,
    BOnly = 2,
    Both = 3,
}

impl MarkovState {
    pub fn from_running(a: bool, b: bool) -> Self {
        match (a, b) {
            (false, false) => MarkovState::Neither,
            (true, false) => MarkovState::AOnly,
            (false, true) => MarkovState::BOnly,
            (true, true) => MarkovState::Both,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

impl fmt::Debug for MarkovState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            MarkovState::Neither => "Neither",
            MarkovState::AOnly => "AOnly",
            MarkovState::BOnly => "BOnly",
            MarkovState::Both => "Both",
        };
        f.write_str(name)
    }
}

#[derive(Debug, Clone)]
pub struct MarkovEdge {
    pub state: MarkovState,
    pub last_change_time: u64,
    pub state_last_left: [u64; 4],
    pub time_to_leave: [f32; 4],
    pub transition_prob: [[f32; 4]; 4],
    pub both_running_time: u64,
}

impl MarkovEdge {
    pub fn new(state: MarkovState, now: u64) -> Self {
        Self {
            state,
            last_change_time: now,
            state_last_left: [now; 4],
            time_to_leave: [0.0; 4],
            transition_prob: [[0.0; 4]; 4],
            both_running_time: 0,
        }
    }

    /// Update the edge state and statistics when a transition occurs.
    pub fn update_state(&mut self, new_state: MarkovState, now: u64, decay: f32) {
        if new_state == self.state {
            return;
        }

        let old_state = self.state;
        let old_ix = old_state.index();
        let new_ix = new_state.index();

        let dt_since_left = now.saturating_sub(self.state_last_left[old_ix]);
        let dt_since_change = now.saturating_sub(self.last_change_time);

        let mix_tt = (-decay * dt_since_left as f32).exp();
        let mix_tp = (-decay * dt_since_change as f32).exp();

        let dwell = dt_since_change as f32;
        self.time_to_leave[old_ix] = mix_tt * self.time_to_leave[old_ix] + (1.0 - mix_tt) * dwell;

        for i in 0..4 {
            for j in 0..4 {
                if i == j {
                    continue;
                }
                let p = if i == old_ix && j == new_ix { 1.0 } else { 0.0 };
                self.transition_prob[i][j] =
                    mix_tp * self.transition_prob[i][j] + (1.0 - mix_tp) * p;
            }
        }

        self.state_last_left[old_ix] = now;
        self.last_change_time = now;
        self.state = new_state;
    }
}
