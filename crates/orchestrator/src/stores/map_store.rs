#![forbid(unsafe_code)]

use crate::domain::{MapId, MapKey, MapSegment};
use slotmap::SlotMap;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct MapStore {
    maps: SlotMap<MapId, MapSegment>,
    by_key: HashMap<MapKey, MapId>,
}

impl MapStore {
    pub fn ensure(&mut self, segment: MapSegment) -> MapId {
        self.ensure_with_flag(segment).0
    }

    pub fn ensure_with_flag(&mut self, segment: MapSegment) -> (MapId, bool) {
        let key = segment.key();
        if let Some(id) = self.by_key.get(&key) {
            return (*id, false);
        }
        let id = self.maps.insert(segment);
        self.by_key.insert(key, id);
        (id, true)
    }

    pub fn get(&self, id: MapId) -> Option<&MapSegment> {
        self.maps.get(id)
    }

    pub fn id_by_key(&self, key: &MapKey) -> Option<MapId> {
        self.by_key.get(key).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (MapId, &MapSegment)> {
        self.maps.iter()
    }
}
