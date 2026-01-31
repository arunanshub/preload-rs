# preload-rs Guide

This guide is for end users. It explains how to run preload-rs, how
configuration works, and what each setting does.

## What preload-rs does

preload-rs watches the programs you run and learns which binaries and shared
libraries are likely to be used next. It then prefetches those files into memory
so program startup feels faster. It is designed for Linux systems.

## Quick start

1. Copy the example config:

   ```bash
   cp docs/config.example.toml config.toml
   ```

2. Run preload-rs:

   ```bash
   cargo run -p cli --
   ```

3. Increase verbosity if you want more logs:

   ```bash
   cargo run -p cli -- -v
   ```

## Command-line flags (succinct)

- `-c, --config FILE` Load a single config file (and optional `--config-dir`).
- `--config-dir DIR` Load additional `.toml` files from a directory.
- `-s, --state FILE` Override the state database path.
- `--once` Run a single tick and exit.
- `--no-persist` Disable persistence entirely.
- `--no-prefetch` Disable prefetch I/O (observe/predict only).
- `-v, --verbose` Increase log verbosity (`-v`, `-vv`, `-vvv`).

## Configuration file locations and precedence

If `--config` is provided, that file is used first. If `--config-dir` is also
provided, any `.toml` files in that directory are appended (sorted by name).

If `--config` is not provided, preload-rs searches in this order (later files
override earlier ones):

1. `/etc/preload-rs/config.toml` (if it exists)
2. `/etc/preload-rs/config.d/*.toml` (sorted)
3. `$XDG_CONFIG_HOME/preload-rs/config.toml` (or `$HOME/.config/preload-rs/config.toml`)
4. `./config.toml` (current directory)
5. `--config-dir` (if provided, sorted)

## Runtime controls (signals)

- **SIGHUP**: Reload configuration.
- **SIGUSR1**: Dump current config + state summary to logs.
- **SIGUSR2**: Save state immediately.
- **Ctrl-C**: Shut down (and save if `save_on_shutdown = true`).

## Configuration reference

All values are in seconds unless noted.

### `[model]`

- `cycle`: Length of one observation/prediction cycle. Smaller values react
  faster but can increase CPU usage.
- `use_correlation`: Whether to use correlation between apps in prediction.
- `minsize`: Minimum total mapped bytes to admit an executable.
- `active_window`: Time window for the active-set (limits Markov edges to recent
  executables).
- `half_life`: Optional decay half-life. If set, it overrides `decay`.
- `decay`: Decay factor for exponential smoothing (ignored if `half_life` is set).

### `[model.memory]`

Controls the prefetch budget as a weighted sum of memory stats. Each value is a
percentage clamped to `-100..=100`.

- `memtotal`: Percent of total memory to include in budget.
- `memfree`: Percent of free memory to include.
- `memcached`: Percent of cached memory to include.

Example: `memfree = 50` means the planner can use 50% of currently free memory.

### `[system]`

- `doscan`: Enable or disable scanning of running processes.
- `dopredict`: Enable or disable prediction (planning still runs).
- `autosave`: Default autosave interval (seconds) if persistence is enabled.
- `exeprefix`: Allowed/denied executable prefixes. Use `!/path` to deny; the
  longest matching prefix wins.
- `mapprefix`: Allowed/denied map prefixes (same matching rules as `exeprefix`).
- `sortstrategy`: `none | path | block | inode`.
- `prefetch_concurrency`: Number of parallel prefetch workers. Omit the field
  for auto (CPU cores). `0` disables prefetch entirely.

### `[persistence]`

- `state_path`: Path to the SQLite state DB.
- `autosave_interval`: Optional override for autosave (seconds).
- `save_on_shutdown`: Save state when the process exits cleanly.

## Common recipes

- **Observe only (no I/O):**

  ```bash
  cargo run -p cli -- --no-prefetch
  ```

- **Disable persistence:**

  ```bash
  cargo run -p cli -- --no-persist
  ```

- **Run once (one tick):**

  ```bash
  cargo run -p cli -- --once
  ```

## Operational notes and safety

- **Linux only:** uses `/proc` and `posix_fadvise`.
- **Prefetching uses disk I/O:** it can increase I/O load on slow disks. Tune
  `prefetch_concurrency` and memory budget to fit your system.
- **Permissions:** prefetch uses `posix_fadvise` on files; lack of permission can
  cause warnings but should not crash the daemon.

## Troubleshooting

- **"no config files found"**: preload-rs falls back to defaults. Add a config
  file and rerun or pass `--config`.
- **No maps admitted**: check `minsize`, `exeprefix`, and `mapprefix` rules.
- **No state DB**: set `state_path` or pass `--state`.
