-- Add migration script here
-- Holds our metadata information. Any value must be inserted only once.
CREATE TABLE maps (
    id INTEGER PRIMARY KEY,
    update_time INTEGER NOT NULL,
    offset INTEGER NOT NULL,
    length INTEGER NOT NULL,
    path TEXT NOT NULL
);

CREATE TABLE exemaps (
    exe_id INTEGER NOT NULL,
    map_id INTEGER NOT NULL,
    prob REAL NOT NULL,
    FOREIGN KEY (map_id) REFERENCES maps (id),
    FOREIGN KEY (exe_id) REFERENCES exes (id),
    PRIMARY KEY (exe_id, map_id)
);

CREATE TABLE exes (
    id INTEGER PRIMARY KEY,
    update_time INTEGER,
    time INTEGER NOT NULL,
    path TEXT NOT NULL
);

CREATE TABLE badexes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    update_time INTEGER NOT NULL,
    path TEXT NOT NULL
);

CREATE TABLE markovs (
    exe_a INTEGER UNIQUE,
    exe_b INTEGER UNIQUE,
    time INTEGER NOT NULL,
    -- serialize as `bincode`
    time_to_leave BLOB NOT NULL,
    -- serialize as `bincode`
    `weight` BLOB NOT NULL,
    PRIMARY KEY (exe_a, exe_b),
    -- exe cannot build a markov chain with itself
    CHECK (exe_a != exe_b),
    FOREIGN KEY (exe_a) REFERENCES exes (id),
    FOREIGN KEY (exe_b) REFERENCES exes (id)
);
