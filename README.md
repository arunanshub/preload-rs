# preload-rs

## Design

`preload-rs` has been divided into the following crates, with each crate serving a specific purpose. They are:

- `cli`: Responsible for launching `preload-rs` process. It is a binary crate.
- `config`: Manages configuration for `preload-rs`. It is a library.
- `kernel`: Manages the core functionality of `preload-rs`. It is a library.

All crates reside under `crates/` directory.

## Guidelines

Please see [CONTRIBUTING](./CONTRIBUTING.md).
