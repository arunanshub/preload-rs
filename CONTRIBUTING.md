# Guidelines

TODO: add welcoming message

- Use `tracing::trace!()` macro in place of `println!`/`eprintln!`. In other words, wherever you would use
  a `println!`, just use `trace!` instead.

- Use [`pre-commit`](https://pre-commit.com) to perform checks before committing. Install pre-commit hooks
  by running `pre-commit install`.

- Avoid using `unwrap` wherever possible. Rely on `Result<T, E>`.

## Setting up the dev environment

We use `sqlx` to communicate with the sqlite database. As a result, you must do the following:

1. Install the [sqlx-cli](https://crates.io/crates/sqlx-cli/).

2. Set the database URL in a `.env` file:

   ```
   DATABASE_URL="sqlite://./dev.db"
   ```

3. Create the database that be used by sqlx:

   ```
   sqlx database create
   ```

4. Apply the migrations to the dev database.

   ```shell
   sqlx migrate run --source crates/kernel/migrations
   ```

   All the migration scripts for the application are stored in
   [crates/kernel/migrations](crates/kernel/migrations/).

For a more in-depth guide, please check [launchbadge/sqlx](https://github.com/launchbadge/sqlx).

## If you add any SQL queries...

If you add any SQL queries, make sure to run:

```shell
cargo sqlx prepare --workspace
```

This will prepare the queries for offline mode checking. This helps in testing
in our CI/CD environment.
