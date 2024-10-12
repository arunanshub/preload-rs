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
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    map_id INTEGER NOT NULL,
    prob REAL NOT NULL,
    FOREIGN KEY (map_id) REFERENCES maps (id)
);

CREATE TABLE exes (
    id INTEGER PRIMARY KEY,
    update_time INTEGER,
    time INTEGER NOT NULL,
    path TEXT NOT NULL
)
-- CREATE TABLE markovs (
--     id INTEGER PRIMARY KEY AUTOINCREMENT,
--     exe_a INTEGER NOT NULL,
--     exe_b INTEGER NOT NULL,
--     time TIMESTAMP NOT NULL,
--     time_to_leave BLOB NOT NULL, -- serialize as `msgpack`
--     weight BLOB NOT NULL         -- serialize as `msgpack`

--     PRIMARY KEY (exe_a, exe_b),
--     CHECK (exe_a != exe_b) -- exe cannot build a markov chain with itself
-- );
