# AGENTS

This repository is a Rust rewrite of the "preload" daemon.  It is a Cargo
workspace located under `crates/` with three crates:

- `cli` – binary launcher
- `config` – configuration library
- `kernel` – core functionality and database layer

Read the [README.md](./README.md) for an introduction and
[CONTRIBUTING.md](./CONTRIBUTING.md) for contributor guidelines.

## Development setup

1. Install `sqlx-cli`.
2. Add a `.env` file at the repository root containing:
   ```
   DATABASE_URL="sqlite://./dev.db"
   ```
3. Create and migrate the development database:
   ```
   sqlx database create
   sqlx migrate run --source crates/kernel/migrations
   ```
4. When SQL queries change, refresh the offline data with:
   ```
   cargo sqlx prepare --workspace
   ```

## Working with the code

- Run `pre-commit install` once to enable formatting and lint checks.
- Use `tracing` macros instead of `println!` for logging.
- Avoid `unwrap`; prefer `Result<T, E>` error handling.
- Verify the workspace with:
   ```
   cargo fmt --check --all
   cargo check --workspace --all-features
   cargo clippy --all-targets -- -D warnings
   cargo test --workspace --all-features
   ```
  Tests rely on the development database set up earlier.

## Additional notes

- Migrations live in `crates/kernel/migrations`.
- CI uses `cargo llvm-cov nextest` for coverage; local testing can use
  `cargo test` if `cargo-nextest` is unavailable.

Happy hacking!
