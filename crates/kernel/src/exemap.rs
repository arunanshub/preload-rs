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
