use bitflags::bitflags;

bitflags! {
    #[repr(transparent)]
    #[derive(Debug, Default, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
    pub struct MarkovState: u8 {
        // avoiding zero-bit flag since it is always contained, but is never
        // intersected
        const NeitherRunning = 0b001;
        const ExeARunning = 0b010;
        const ExeBRunning = 0b100;
        const BothRunning = 0b110;
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
}
