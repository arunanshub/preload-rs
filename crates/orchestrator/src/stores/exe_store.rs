#![forbid(unsafe_code)]

use crate::domain::{Exe, ExeId, ExeKey};
use slotmap::SlotMap;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct ExeStore {
    exes: SlotMap<ExeId, Exe>,
    by_key: HashMap<ExeKey, ExeId>,
}

impl ExeStore {
    pub fn ensure(&mut self, key: ExeKey) -> ExeId {
        if let Some(id) = self.by_key.get(&key) {
            return *id;
        }
        let exe = Exe::new(key.clone());
        let id = self.exes.insert(exe);
        self.by_key.insert(key, id);
        id
    }

    pub fn get(&self, id: ExeId) -> Option<&Exe> {
        self.exes.get(id)
    }

    pub fn get_mut(&mut self, id: ExeId) -> Option<&mut Exe> {
        self.exes.get_mut(id)
    }

    pub fn id_by_key(&self, key: &ExeKey) -> Option<ExeId> {
        self.by_key.get(key).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (ExeId, &Exe)> {
        self.exes.iter()
    }

    pub fn keys(&self) -> impl Iterator<Item = &ExeKey> {
        self.by_key.keys()
    }
}
