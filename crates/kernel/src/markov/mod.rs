mod markov_state;

pub use markov_state::MarkovState;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Markov {
    pub exe_a: PathBuf,

    pub exe_b: PathBuf,

    pub time: u64,

    pub time_to_leave: [f32; 4],

    pub weight: [[u32; 4]; 4],

    pub state: MarkovState,

    pub change_timestamp: u64,
}
