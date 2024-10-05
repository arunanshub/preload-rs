use bitflags::bitflags;

bitflags! {
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
    pub struct MarkovState: u8 {
        // avoiding zero-bit flag since it is always contained, but is never
        // intersected
        const NeitherRunning = 0b001;
        const ExeARunning = 0b010;
        const ExeBRunning = 0b100;
        const BothRunning = 0b110;
    }
}

impl Default for MarkovState {
    fn default() -> Self {
        Self::NeitherRunning
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_markov_state_flags() {
        assert_eq!(
            MarkovState::BothRunning,
            MarkovState::ExeARunning | MarkovState::ExeBRunning
        );
        assert_eq!(
            MarkovState::BothRunning | MarkovState::ExeARunning,
            MarkovState::BothRunning,
        );
    }

    #[test]
    fn test_markov_state_default() {
        assert_eq!(MarkovState::NeitherRunning, MarkovState::default());
    }
}
