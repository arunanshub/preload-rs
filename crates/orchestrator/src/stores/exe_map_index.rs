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

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use slotmap::SlotMap;

    proptest! {
        #[test]
        fn index_relationships_remain_consistent(
            exe_count in 0usize..10,
            map_count in 0usize..10,
            attachments in prop::collection::vec((0u8..20, 0u8..20), 0..50),
            removals in prop::collection::vec(0u8..20, 0..10),
        ) {
            let mut index = ExeMapIndex::default();
            let mut exe_ids = SlotMap::<ExeId, ()>::with_key();
            let mut map_ids = SlotMap::<MapId, ()>::with_key();

            let exes: Vec<_> = (0..exe_count).map(|_| exe_ids.insert(())).collect();
            let maps: Vec<_> = (0..map_count).map(|_| map_ids.insert(())).collect();

            if !exes.is_empty() && !maps.is_empty() {
                for (e, m) in attachments {
                    let exe = exes[e as usize % exes.len()];
                    let map = maps[m as usize % maps.len()];
                    index.attach(exe, map);
                }

                for e in removals {
                    let exe = exes[e as usize % exes.len()];
                    index.remove_exe(exe);
                }
            }

            for (exe, maps) in index.exe_to_maps.iter() {
                for map in maps {
                    let back = index
                        .map_to_exes
                        .get(map)
                        .map(|set| set.contains(exe))
                        .unwrap_or(false);
                    prop_assert!(back);
                }
            }

            for (map, exes) in index.map_to_exes.iter() {
                prop_assert!(!exes.is_empty());
                for exe in exes {
                    let back = index
                        .exe_to_maps
                        .get(exe)
                        .map(|set| set.contains(map))
                        .unwrap_or(false);
                    prop_assert!(back);
                }
            }
        }
    }
}
