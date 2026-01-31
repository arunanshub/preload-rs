CREATE TABLE IF NOT EXISTS state (
    id INTEGER PRIMARY KEY CHECK(id = 1),
    schema_version INTEGER NOT NULL,
    app_version TEXT,
    created_at TEXT,
    model_time INTEGER NOT NULL,
    last_accounting_time INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS exes (
    path TEXT NOT NULL PRIMARY KEY,
    total_running_time INTEGER NOT NULL,
    last_seen_time INTEGER
);

CREATE TABLE IF NOT EXISTS maps (
    path TEXT NOT NULL,
    offset INTEGER NOT NULL,
    length INTEGER NOT NULL,
    update_time INTEGER NOT NULL,
    PRIMARY KEY (path, offset, length)
);

CREATE TABLE IF NOT EXISTS exe_maps (
    exe_path TEXT NOT NULL,
    map_path TEXT NOT NULL,
    map_offset INTEGER NOT NULL,
    map_length INTEGER NOT NULL,
    prob REAL NOT NULL,
    PRIMARY KEY (exe_path, map_path, map_offset, map_length)
);

CREATE TABLE IF NOT EXISTS markovs (
    exe_a TEXT NOT NULL,
    exe_b TEXT NOT NULL,
    time_to_leave BLOB NOT NULL,
    transition_prob BLOB NOT NULL,
    both_running_time INTEGER NOT NULL,
    PRIMARY KEY (exe_a, exe_b)
);
