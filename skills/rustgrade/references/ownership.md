# Ownership, borrowing and API shape

Rules: `ref-owned-param`, `clone-in-loop` (see also [performance.md](performance.md)).

## Take the borrowed form

`&String` forces the caller to own a `String`. `&str` accepts a `String`
(by deref coercion), a literal, a slice of a larger buffer, and more.

| Instead of | Take | Why |
|---|---|---|
| `&String` | `&str` | literals and substrings work |
| `&Vec<T>` | `&[T]` | arrays, slices, `SmallVec` work |
| `&Box<T>` | `&T` | the box is an allocation detail |
| `&PathBuf` | `&Path` | `&str` and `OsStr` paths work via `AsRef<Path>` |
| `&OsString` | `&OsStr` | same reason |

```rust
// Before
fn greet(name: &String) -> String { format!("hello {name}") }
fn total(xs: &Vec<u64>) -> u64 { xs.iter().sum() }

// After
fn greet(name: &str) -> String { format!("hello {name}") }
fn total(xs: &[u64]) -> u64 { xs.iter().sum() }
```

`&mut Vec<T>` and `&mut String` are fine: the callee may need to grow them.

## Constructors: take ownership once

If a function stores the value, take it by value (or `impl Into<T>`) so the
caller decides whether to move or clone. Taking `&str` and calling
`.to_owned()` inside forces a copy even when the caller had a `String` to give.

```rust
pub struct User { name: String }

impl User {
    // Caller with a String moves it; caller with a &str pays one allocation.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}
```

For paths, take `impl AsRef<Path>` when you only read, or
`impl Into<PathBuf>` when you store.

## Clone deliberately

A `.clone()` is an ownership decision, not a borrow-checker silencer.
Before cloning, try in order:

1. **Borrow**: pass `&x` and shorten the borrow's scope.
2. **Move**: if `x` isn't used afterwards, move it.
3. **Restructure**: split the struct so the borrow checker sees disjoint
   fields, or take what you need with `std::mem::take` / `Option::take`.
4. **Share**: if many owners truly need it, `Arc<T>` (threads) or `Rc<T>`.
   Write `Arc::clone(&x)`, not `x.clone()`, so readers see a refcount bump.
5. **Clone**, and keep it outside loops.

```rust
// Before: clones every name to satisfy the borrow checker
for user in &users {
    let name = user.name.clone();
    index.insert(name, user.id);
}

// After: the map borrows from `users` for as long as it lives
let index: HashMap<&str, u64> = users.iter().map(|u| (u.name.as_str(), u.id)).collect();
```

## Cow when it is usually borrowed

```rust
use std::borrow::Cow;

/// Allocates only when something actually needs escaping.
fn escape(input: &str) -> Cow<'_, str> {
    if input.contains('<') {
        Cow::Owned(input.replace('<', "&lt;"))
    } else {
        Cow::Borrowed(input)
    }
}
```

## Return types

- Return owned data from constructors and from functions that compute new
  values. Return `&T` / `&str` / `&[T]` from getters.
- Return `impl Iterator<Item = &T>` instead of building a `Vec` the caller
  will only iterate.
- Don't return `&String` / `&Vec<T>` from getters either; return `&str` /
  `&[T]` so you can change the representation later.

## `Rc<RefCell<T>>` and `Arc<Mutex<T>>` are not defaults

They move borrow checking to runtime and hide ownership. Reach for them only
after a clear owner plus `&mut` access, or message passing, has been ruled out.
See [concurrency.md](concurrency.md).
