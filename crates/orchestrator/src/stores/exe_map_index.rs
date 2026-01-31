#![forbid(unsafe_code)]

use crate::domain::{ExeId, MapId};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default)]
pub struct ExeMapIndex {
    exe_to_maps: HashMap<ExeId, HashSet<MapId>>,
    map_to_exes: HashMap<MapId, HashSet<ExeId>>,
}

impl ExeMapIndex {
    pub fn attach(&mut self, exe_id: ExeId, map_id: MapId) {
        self.exe_to_maps.entry(exe_id).or_default().insert(map_id);
        self.map_to_exes.entry(map_id).or_default().insert(exe_id);
    }

    pub fn maps_for_exe(&self, exe_id: ExeId) -> impl Iterator<Item = MapId> + '_ {
        self.exe_to_maps
            .get(&exe_id)
            .into_iter()
            .flat_map(|set| set.iter().copied())
    }

    pub fn exes_for_map(&self, map_id: MapId) -> impl Iterator<Item = ExeId> + '_ {
        self.map_to_exes
            .get(&map_id)
            .into_iter()
            .flat_map(|set| set.iter().copied())
    }

    pub fn remove_exe(&mut self, exe_id: ExeId) {
        if let Some(maps) = self.exe_to_maps.remove(&exe_id) {
            for map_id in maps {
                if let Some(exes) = self.map_to_exes.get_mut(&map_id) {
                    exes.remove(&exe_id);
                    if exes.is_empty() {
                        self.map_to_exes.remove(&map_id);
                    }
                }
            }
        }
    }
}
