#![forbid(unsafe_code)]

use config::Config;
use orchestrator::domain::{ExeKey, MapSegment, MarkovState};
use orchestrator::prediction::{MarkovPredictor, Predictor};
use orchestrator::stores::{EdgeKey, Stores};
use std::path::PathBuf;
use std::time::Duration;

#[test]
fn predictor_scores_non_running_exe_from_edge() {
    let mut config = Config::default();
    config.model.use_correlation = false;
    config.model.cycle = Duration::from_secs(1);

    let mut stores = Stores::default();
    let exe_a = stores.ensure_exe(ExeKey::new(PathBuf::from("/usr/bin/a")));
    let exe_b = stores.ensure_exe(ExeKey::new(PathBuf::from("/usr/bin/b")));

    stores.model_time = 10;
    stores.exes.get_mut(exe_a).unwrap().running = false;
    stores.exes.get_mut(exe_b).unwrap().running = true;

    let now = stores.model_time;
    stores.ensure_markov_edge(exe_a, exe_b, now, MarkovState::BOnly);
    let edge_key = EdgeKey::new(exe_a, exe_b);
    let edge = stores.markov.get_mut(edge_key).unwrap();
    edge.time_to_leave[MarkovState::BOnly.index()] = 1.0;
    edge.transition_prob[MarkovState::BOnly.index()][MarkovState::AOnly.index()] = 1.0;

    let map_id = stores.ensure_map(MapSegment::new("/usr/lib/libfoo.so", 0, 2048, now));
    stores.attach_map(exe_a, map_id);

    let predictor = MarkovPredictor::new(&config);
    let prediction = predictor.predict(&stores);

    let expected = 1.0 - (-1.0f32).exp();
    let a_score = prediction.exe_scores.get(&exe_a).copied().unwrap();
    let b_score = prediction.exe_scores.get(&exe_b).copied().unwrap();

    assert!((a_score - expected).abs() < 1e-4);
    assert_eq!(b_score, 0.0);

    let map_score = prediction.map_scores.get(&map_id).copied().unwrap();
    assert!((map_score - a_score).abs() < 1e-6);
}
