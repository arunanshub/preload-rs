# preload-rs

[![Coverage Status](https://coveralls.io/repos/github/arunanshub/preload-rs/badge.svg?branch=master)](https://coveralls.io/github/arunanshub/preload-rs?branch=master)

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


## Design

`preload-rs` has been divided into the following crates, with each crate serving a specific purpose. They are:

- `cli`: Responsible for launching `preload-rs` process. It is a binary crate.
- `config`: Manages configuration for `preload-rs`. It is a library.
- `kernel`: Manages the core functionality of `preload-rs`. It is a library.

All crates reside under `crates/` directory.

## Guidelines

Please see [CONTRIBUTING](./CONTRIBUTING.md).
