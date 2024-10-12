mod database;

use crate::Map;
use educe::Educe;

#[derive(Debug, Default, Clone, Educe)]
#[educe(Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ExeMap {
    pub map: Map,

    #[educe(Eq(ignore), Ord(ignore), Hash(ignore))]
    pub prob: f32,
}

impl ExeMap {
    pub fn new(map: Map) -> Self {
        Self { map, prob: 1.0 }
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
