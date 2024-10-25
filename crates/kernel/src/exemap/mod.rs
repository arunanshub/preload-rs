pub(crate) mod database;

use crate::Map;
use educe::Educe;

#[derive(Debug, Default, Clone, Educe)]
#[educe(Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ExeMap {
    pub map: Map,

    // TODO: should this be Option<u64>?
    #[educe(Eq(ignore), Ord(ignore), Hash(ignore))]
    pub exe_seq: Option<u64>,

    #[educe(Eq(ignore), Ord(ignore), Hash(ignore))]
    pub prob: f32,
}

impl ExeMap {
    pub fn new(map: Map) -> Self {
        Self {
            map,
            exe_seq: None,
            prob: 1.0,
        }
    }

    /// Called by an [`Exe`](crate::Exe).
    ///
    /// # Note
    ///
    /// Why are we doing this? This is because, during the `write to the db`
    /// phase, exemap needs to know the `exe_seq` that it is related to. Ideally
    /// we could have given exemap a weakref to exe, but that would have made
    /// the code more complex. Maybe that's a todo for some other time.
    pub fn with_exe_seq(mut self, exe_seq: u64) -> Self {
        self.exe_seq.replace(exe_seq);
        self
    }

    pub fn with_prob(mut self, prob: f32) -> Self {
        self.prob = prob;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exemap_prob_always_1() {
        let map = Map::new("test", 0, 0, 0);
        let exe_map = ExeMap::new(map);
        assert_eq!(exe_map.prob, 1.0);
    }
}
