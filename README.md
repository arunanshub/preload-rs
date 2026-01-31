# preload-rs

[![CI](https://github.com/arunanshub/preload-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/arunanshub/preload-rs/actions/workflows/ci.yml)
[![Docs](https://github.com/arunanshub/preload-rs/actions/workflows/docs.yml/badge.svg)](https://arunanshub.github.io/preload-rs)
[![codecov](https://img.shields.io/codecov/c/github/arunanshub/preload-rs?logo=codecov)](https://codecov.io/github/arunanshub/preload-rs)

`preload-rs` is a daemon process that prefetches binary files and shared libraries
from the hard disc to the main memory of the computer system to achieve faster
application startup time. `preload-rs` is adaptive: it monitors the application that
the user runs, and by analyzing this data, predicts what applications he might
run in the near future, and fetches those binaries and their dependencies into
memory.

It builds a Markov-based probabilistic model capturing the correlation between
every two applications on the system. The model is then used to infer the
probability that each application may be started in the near future. These
probabilities are used to choose files to prefetch into the main memory. Special
care is taken to not degrade system performance and only prefetch when enough
resources are available.

> [!NOTE]
> Preload-rs is not a "rewrite" of the original preload anymore. While it
> follows the logic as mentioned in preload's thesis, preload-rs is a
> reimplementation rather than a rewrite.

## Design

`preload-rs` has been divided into the following crates, with each crate serving a specific purpose. They are:

- `cli`: Responsible for launching `preload-rs` process. It is a binary crate.
- `config`: Manages configuration for `preload-rs`. It is a library.
- `orchestrator`: Manages core functionality and persistence. It is a library.

All crates reside under `crates/` directory.

## Configuration

An example configuration is available at `docs/config.example.toml`.

## Guidelines

Please see [CONTRIBUTING](./CONTRIBUTING.md).

## How to use

Please see [GUIDE](./GUIDE.md)

## How to develop/extend

Please see [DEVELOPING](./DEVELOPING.md)
