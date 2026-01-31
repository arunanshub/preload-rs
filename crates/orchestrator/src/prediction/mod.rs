#![forbid(unsafe_code)]

mod predictor;
mod types;

pub use predictor::{MarkovPredictor, Predictor};
pub use types::{Prediction, PredictionSummary};
