# Error handling

Rules: `unwrap-outside-tests`, `anyhow-in-lib-api`.

## Panics are for bugs, `Result` is for failures

A failure the caller can cause or the environment can produce (bad input,
missing file, network down, poisoned lock in a server) returns `Result`.
A panic means "this program has a bug". Request handlers, daemons and
libraries must not panic on input.

| Situation | Use |
|---|---|
| Caller or environment can trigger it | `Result` + `?` |
| Invariant you established and can name | `.expect("invariant: ...")` |
| Tests, examples, benches | `.unwrap()` is fine |
| Prototype you will delete | `.unwrap()`, but not in a PR |

```rust
// Before: any malformed request kills the worker
fn handle(req: &str) -> Response {
    let (name, age) = req.split_once(':').unwrap();
    let age: u32 = age.trim().parse().unwrap();
    Response::ok(name, age)
}

// After
fn handle(req: &str) -> Result<Response, RequestError> {
    let (name, age) = req.split_once(':').ok_or(RequestError::MissingSeparator)?;
    let age: u32 = age.trim().parse().map_err(RequestError::BadAge)?;
    Ok(Response::ok(name, age))
}
```

`.expect()` is acceptable when the message states the invariant, not the
symptom: `expect("regex literal is valid")`, not `expect("failed")`.

## Libraries: typed errors

Callers of a library need to **match** on failures (retry on timeout, 404 on
not-found). `anyhow::Error` erases that. Use an enum, usually with `thiserror`:

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    #[error("reading {path}")]
    Io { path: PathBuf, #[source] source: std::io::Error },
    #[error("invalid config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("missing key `{0}`")]
    MissingKey(&'static str),
}

pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let text = std::fs::read_to_string(path)
        .map_err(|source| ConfigError::Io { path: path.to_owned(), source })?;
    Ok(toml::from_str(&text)?)
}
```

- `#[non_exhaustive]` lets you add variants without a breaking change.
- Keep the source error (`#[source]` / `#[from]`), don't stringify it.
- One error enum per module boundary is usually enough; avoid one giant enum.
- `Box<dyn Error + Send + Sync>` in a public API has the same problem as
  `anyhow`: callers can't match.

## Binaries: `anyhow` with context

In `main.rs`, CLI commands and glue code, nobody matches on the error; humans
read it. Add context at each layer so the chain explains itself:

```rust
use anyhow::Context;

fn main() -> anyhow::Result<()> {
    let cfg = config::load(&path).with_context(|| format!("loading {}", path.display()))?;
    run(cfg).context("running server")?;
    Ok(())
}
```

Use `with_context(|| ...)` when the message allocates, so it is only built on
failure.

## Poisoned locks

`Mutex::lock()` returns `Err` only if another thread panicked while holding
the lock. Options, in order of preference:

1. Avoid the shared lock (see [concurrency.md](concurrency.md)).
2. Recover the data if it is still consistent:
   `.lock().unwrap_or_else(PoisonError::into_inner)`.
3. `.expect("lock poisoned: a worker panicked")` if a panic elsewhere means
   the process must stop anyway. Say so in the message.

## Checklist

- [ ] No `.unwrap()` on input-dependent paths
- [ ] `.expect()` messages state the invariant
- [ ] Public library errors are typed enums with sources preserved
- [ ] `main` returns `Result` and errors carry context
- [ ] No error swallowed with `let _ =` or `.ok()` without a comment saying why
