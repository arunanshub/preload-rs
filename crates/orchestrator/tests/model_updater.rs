#![forbid(unsafe_code)]

use config::Config;
use orchestrator::{
    ModelUpdater,
    domain::MapSegment,
    observation::{DefaultAdmissionPolicy, DefaultModelUpdater, ObservationEvent},
    stores::Stores,
};
use std::path::PathBuf;

#[test]
fn admits_exe_and_maps() {
    let config = Config::default();
    let policy = DefaultAdmissionPolicy::new(&config);
    let mut updater = DefaultModelUpdater::new(&config);
    let mut stores = Stores::default();

    let exe_path = PathBuf::from("/usr/bin/app");
    let map_path = PathBuf::from("/usr/lib/libfoo.so");
    let map = MapSegment::new(map_path, 0, config.model.minsize, 0);

    let observation = vec![
        ObservationEvent::ObsBegin {
            time: 0,
            scan_id: 1,
        },
        ObservationEvent::ExeSeen {
            path: exe_path.clone(),
            pid: 1,
        },
        ObservationEvent::MapSeen {
            exe_path: exe_path.clone(),
            map,
        },
        ObservationEvent::ObsEnd {
            time: 0,
            scan_id: 1,
            warnings: Vec::new(),
        },
    ];

    let delta = updater.apply(&mut stores, &observation, &policy).unwrap();

    assert_eq!(delta.new_exes.len(), 1, "delta: {:?}", delta);
    assert_eq!(delta.new_maps.len(), 1, "delta: {:?}", delta);
    assert_eq!(stores.exes.iter().count(), 1);
    assert_eq!(stores.maps.iter().count(), 1);
}
