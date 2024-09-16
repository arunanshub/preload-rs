# proptest-rs

## Design

`preload-rs` has been divided into four crates, with each crate serving a specific purpose. They are:

- `cli`: Responsible for launching `preload-rs` process. It is a binary crate.
- `config`: Manages configuration for `preload-rs`. It is a library.
- `db`: Manages data storage operations necessary for `preload-rs` to function correctly. It is a library.
- `kernel`: Manages the core functionality of `preload-rs`. It is a library.

All crates reside under `crates/` directory.
