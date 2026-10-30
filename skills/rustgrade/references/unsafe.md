# Unsafe code

Rule: `undocumented-unsafe` (medium; `low` in test code). `// SAFETY:` in any
case counts, and blocks inside an `unsafe fn` are covered by its
`/// # Safety` section.

`unsafe` does not turn off the borrow checker; it lets you do five extra
things (dereference raw pointers, call `unsafe fn`, access `static mut`,
implement `unsafe trait`, access union fields), and makes **you** responsible
for upholding the invariants the compiler can no longer check. Undefined
behavior anywhere can break the program everywhere.

## First: do you need it?

Most `unsafe` in application code has a safe, equally fast alternative:

| Unsafe | Safe alternative |
|---|---|
| `get_unchecked(i)` | iterators, `chunks_exact`, an `assert!(i < len)` before the loop (lets LLVM drop checks) |
| `transmute` between ints/bytes | `u32::from_le_bytes`, `to_ne_bytes`, `bytemuck` |
| `static mut` | `AtomicUsize`, `OnceLock`, `LazyLock`, `Mutex` |
| raw pointers for aliasing | split borrows (`split_at_mut`), indices, `Cell` |
| `String::from_utf8_unchecked` | `String::from_utf8` (measure the check first) |

Use `unsafe` only for FFI, for building a safe abstraction that can't be
expressed otherwise, or for a **measured** win. Put
`#![forbid(unsafe_code)]` in crates that don't need it.

## Every `unsafe` block gets a `// SAFETY:` comment

The comment names the invariant and why it holds **here**. It is written for
the reviewer who must decide whether the block is sound.

```rust
// Before
fn first(v: &[u8]) -> u8 {
    unsafe { *v.get_unchecked(0) }
}

// After
fn first(v: &[u8]) -> Option<u8> {
    if v.is_empty() {
        return None;
    }
    // SAFETY: `v` is non-empty (checked above), so index 0 is in bounds.
    Some(unsafe { *v.get_unchecked(0) })
}
```

Bad comments restate the code ("calling get_unchecked") or assert without
reason ("this is safe"). Good ones cite the check, the type invariant or the
caller contract that makes it sound.

`unsafe impl Send` / `Sync` need the same: say why the type is safe to move
or share across threads.

## Every `unsafe fn` documents its contract

```rust
/// Returns the element at `index` without bounds checking.
///
/// # Safety
///
/// `index` must be less than `self.len()`.
pub unsafe fn get_unchecked(&self, index: usize) -> &T {
    // SAFETY: the caller guarantees `index < self.len()`.
    unsafe { self.data.get_unchecked(index) }
}
```

Since edition 2024, operations inside an `unsafe fn` still need their own
`unsafe {}` block (`unsafe_op_in_unsafe_fn`), and each block needs its own
`SAFETY` comment.

## Encapsulate

Keep `unsafe` in a small module whose **public API is safe**: every input a
safe caller can provide must be unable to cause UB. Fields that the unsafe
code relies on (a length, a capacity) must be private, so no safe code can
break the invariant.

## Common sources of UB to check for

- Creating a reference (`&T`/`&mut T`) to invalid, unaligned or
  uninitialized memory, even briefly.
- Two `&mut` to the same data alive at once (aliasing), including via raw
  pointers converted back to references.
- Reading uninitialized memory: use `MaybeUninit`, never `mem::uninitialized`.
- Out-of-bounds pointer arithmetic (`ptr.add` past the allocation end + 1).
- Data races through raw pointers or `static mut`.
- `transmute` to a type with invalid bit patterns (`bool`, `char`, enums,
  references).
- FFI: wrong ABI, lifetimes or ownership of returned pointers, unwinding
  across `extern "C"` (use `extern "C-unwind"` or catch the panic).

## Test it

- `cargo +nightly miri test` runs tests under an interpreter that detects
  most UB (aliasing, uninit reads, out-of-bounds, leaks). Run it on every
  module with `unsafe`.
- Sanitizers for FFI-heavy code: `RUSTFLAGS="-Zsanitizer=address"`.
- Fuzz the safe API around the unsafe core (`cargo fuzz`).

## Checklist

- [ ] Each `unsafe` block is necessary and has a `// SAFETY:` comment with a real argument
- [ ] Each `unsafe fn` / `unsafe trait` has a `# Safety` section
- [ ] The safe API cannot be used to trigger UB; invariant-carrying fields are private
- [ ] Miri passes on the tests that exercise the unsafe code
- [ ] Crates without unsafe declare `#![forbid(unsafe_code)]`
