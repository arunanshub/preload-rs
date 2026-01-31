#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::{path::PathBuf, time::Duration};

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Persistence {
    /// Optional path to the state database.
    pub state_path: Option<PathBuf>,

    /// Autosave interval (overrides System.autosave when set).
    #[serde_as(as = "Option<serde_with::DurationSeconds>")]
    pub autosave_interval: Option<Duration>,

    pub save_on_shutdown: bool,
}

impl Default for Persistence {
    fn default() -> Self {
        Self {
            state_path: None,
            autosave_interval: None,
            save_on_shutdown: true,
        }
    }
}

impl Persistence {}
