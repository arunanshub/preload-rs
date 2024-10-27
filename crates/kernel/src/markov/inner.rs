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

    /// Set markov's state based on the running status of the exes.
    ///
    /// See also, [`MarkovState`].
    pub fn set_state(&mut self, last_running_timestamp: u64) -> Result<(), Error> {
        let is_exe_a_running = extract_exe!(self.exe_a).is_running(last_running_timestamp);
        let is_exe_b_running = extract_exe!(self.exe_b).is_running(last_running_timestamp);
        self.state = get_markov_state(is_exe_a_running, is_exe_b_running);
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

    pub fn bid_in_exes(
        &mut self,
        use_correlation: bool,
        state_time: u64,
        cycle: f32,
    ) -> Result<(), Error> {
        let state = self.state.bits() as usize;
        if self.weight[state][state] == 0 {
            return Ok(());
        }

        let correlation = if use_correlation {
            self.correlation(state_time)?
        } else {
            1.0
        };

        if (self.state & MarkovState::ExeARunning) == MarkovState::NeitherRunning {
            let exe = std::mem::take(&mut self.exe_a);
            self.bid_for_exe(&exe, MarkovState::ExeARunning, correlation, cycle)?;
            self.exe_a = exe;
        }
        if (self.state & MarkovState::ExeBRunning) == MarkovState::NeitherRunning {
            let exe = std::mem::take(&mut self.exe_b);
            self.bid_for_exe(&exe, MarkovState::ExeBRunning, correlation, cycle)?;
            self.exe_b = exe;
        }
        Ok(())
    }

    fn bid_for_exe(
        &mut self,
        exe: &ExeForMarkov,
        ystate: MarkovState,
        correlation: f32,
        cycle: f32,
    ) -> Result<(), Error> {
        let state = self.state;
        let state_ix = state.bits() as usize;
        let ystate_ix = ystate.bits() as usize;

        if self.weight[state_ix][state_ix] == 0 || self.time_to_leave[state_ix] <= 1.0 {
            return Ok(());
        }

        // p_state_change is the probability of the state of markov changing in
        // the next period. period is taken as 1.5 cycles. it's computed as:
        //                                            -λ.period
        //   p(state changes in time < period) = 1 - e
        //
        // where λ is one over average time to leave the state.
        let p_state_change = {
            let temp = cycle * 1.5 / self.time_to_leave[state_ix];
            1.0 - temp.exp()
        };

        // p_y_runs_next is the probability that X runs, given that a state
        // change occurs. it's computed linearly based on the number of times
        // transition has occured from this state to other states.
        //
        // regularize a bit by adding something to denominator
        let p_y_runs_next = {
            let temp = (self.weight[state_ix][ystate_ix] + self.weight[state_ix][3]) as f32;
            temp / (self.weight[state_ix][state_ix] as f32 + 0.01)
        };

        // FIXME: (from original impl.) what should we do we correlation w.r.t. state?
        let p_runs = correlation.abs() * p_state_change * p_y_runs_next;
        extract_exe!(exe).lnprob += (1.0 - p_runs).ln();
        Ok(())
    }

    fn correlation(&self, state_time: u64) -> Result<f32, Error> {
        let t = state_time;
        let a = extract_exe!(self.exe_a).time;
        let b = extract_exe!(self.exe_b).time;
        let ab = self.time;

        let correlation = if a == 0 || a == t || b == 0 || b == t {
            0.0
        } else {
            let numerator = (t * ab - a * b) as f32;
            let denominator2 = (a * b * (t - a) * (t - b)) as f32;
            numerator / denominator2.sqrt()
        };

        Ok(correlation)
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
            $exe.0.upgrade().ok_or(Error::ExeMarkovDeallocated)?.lock()
        }};
    }
}
