# Guidelines

TODO: add welcoming message

- Use `tracing::trace!()` macro in place of `println!`/`eprintln!`. In other words, wherever you would use
  a `println!`, just use `trace!` instead.

- Use [`pre-commit`](https://pre-commit.com) to perform checks before committing. Install pre-commit hooks
  by running `pre-commit install`.
