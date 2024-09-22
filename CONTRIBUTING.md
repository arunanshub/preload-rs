# Guidelines

TODO: add welcoming message

- Use `tracing::debug!()` macro in place of `println!`/`eprintln!`. In other words, wherever you would use
  a `println!`, just use `debug!` instead.

- Use [`pre-commit`](https://pre-commit.com) to perform checks before committing. Install pre-commit hooks
  by running `pre-commit install`.
