use super::MarkovState;
use crate::{exe::ExeForMarkov, extract_exe, Error};

#[derive(Debug, Clone)]
pub struct MarkovInner {
    pub exe_a: ExeForMarkov,

    pub exe_b: ExeForMarkov,

    pub time: u64,

    pub time_to_leave: [f32; 4],

    pub weight: [[u32; 4]; 4],

    pub state: MarkovState,

    pub change_timestamp: u64,
}

impl MarkovInner {
    pub fn new(exe_a: ExeForMarkov, exe_b: ExeForMarkov) -> Self {
        Self {
            exe_a,
            exe_b,
            time: 0,
            time_to_leave: [0.0; 4],
            weight: [[0; 4]; 4],
            state: MarkovState::NeitherRunning,
            change_timestamp: 0,
        }
    }

    pub fn with_initialize(
        &mut self,
        state_time: u64,
        last_running_timestamp: u64,
    ) -> Result<(), Error> {
        self.state = get_markov_state(
            extract_exe!(self.exe_a).is_running(last_running_timestamp),
            extract_exe!(self.exe_b).is_running(last_running_timestamp),
        );

        let exe_a_change_timestamp = extract_exe!(self.exe_a).change_timestamp;
        let exe_b_change_timestamp = extract_exe!(self.exe_b).change_timestamp;
        self.change_timestamp = state_time;

        if exe_a_change_timestamp > 0 && exe_b_change_timestamp > 0 {
            if exe_a_change_timestamp < state_time {
                self.change_timestamp = exe_a_change_timestamp;
            }
            if exe_b_change_timestamp < state_time && exe_b_change_timestamp > self.change_timestamp
            {
                self.change_timestamp = exe_a_change_timestamp;
            }
            if exe_a_change_timestamp > self.change_timestamp {
                self.state ^= MarkovState::ExeARunning;
            }
            if exe_b_change_timestamp > self.change_timestamp {
                self.state ^= MarkovState::ExeBRunning;
            }
        }
        self.state_changed(state_time, last_running_timestamp)?;

        Ok(())
    }

    pub fn state_changed(
        &mut self,
        state_time: u64,
        last_running_timestamp: u64,
    ) -> Result<(), Error> {
        if self.change_timestamp == state_time {
            // already taken care of
            return Ok(());
        }

        let old_state = self.state;
        let new_state = get_markov_state(
            extract_exe!(self.exe_a).is_running(last_running_timestamp),
            extract_exe!(self.exe_b).is_running(last_running_timestamp),
        );

        if old_state != new_state {
            return Ok(());
        }
        let old_state_ix = old_state.bits() as usize;
        let new_state_ix = new_state.bits() as usize;

        self.weight[old_state_ix][old_state_ix] += 1;
        self.time_to_leave[old_state_ix] += ((state_time - self.change_timestamp) as f32
            - self.time_to_leave[old_state_ix])
            / self.weight[old_state_ix][old_state_ix] as f32;
        self.weight[old_state_ix][new_state_ix] += 1;
        self.state = new_state;
        self.change_timestamp = state_time;

        Ok(())
    }
}

const fn get_markov_state(is_exe_a_running: bool, is_exe_b_running: bool) -> MarkovState {
    match (is_exe_a_running, is_exe_b_running) {
        (false, false) => MarkovState::NeitherRunning,
        (false, true) => MarkovState::ExeBRunning,
        (true, false) => MarkovState::ExeARunning,
        (true, true) => MarkovState::BothRunning,
    }
}

mod macros {
    #[macro_export]
    macro_rules! extract_exe {
        ($exe:expr) => {{
            $exe.0.upgrade().ok_or(Error::ExeDoesNotExist)?.lock()
        }};
    }
}
