#![forbid(unsafe_code)]

pub mod clock;
pub mod domain;
pub mod engine;
pub mod error;
pub mod observation;
pub mod persistence;
pub mod prediction;
pub mod prefetch;
pub mod stores;

pub use engine::{ControlEvent, PreloadEngine, ReloadBundle, Services, TickReport};
pub use observation::{
    AdmissionDecision, AdmissionPolicy, AdmissionPolicyStats, CandidateExe, Completeness,
    DefaultAdmissionPolicy, DefaultModelUpdater, ModelDelta, ModelUpdater, Observation,
    ObservationEvent, ProcfsScanner, RejectReason, ScanWarning, Scanner,
};
pub use persistence::{NoopRepository, SqliteRepository, StateRepository, StoresSnapshot};
pub use prediction::{MarkovPredictor, Prediction, PredictionSummary, Predictor};
pub use prefetch::{
    GreedyPrefetchPlanner, NoopPrefetcher, PosixFadvisePrefetcher, PrefetchPlan, PrefetchPlanner,
    PrefetchReport, Prefetcher,
};

pub use clock::{Clock, SystemClock};
pub use domain::{Exe, ExeId, ExeKey, MapId, MapKey, MapSegment, MarkovEdge, MarkovState, MemStat};
pub use stores::Stores;
