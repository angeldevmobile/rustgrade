# Performance

Rules: `clone-in-loop`, `alloc-in-loop`. Mark hot code in `rustgrade.toml`
(`hot = ["src/engine/"]`) and these findings are raised to `high` there.

## Measure first, and after

No optimization is "done" without numbers. Pick the tool for the question:

| Question | Tool |
|---|---|
| Is version B faster than A? | `criterion` benchmark |
| Where does the time go? | `cargo flamegraph` (perf / dtrace) |
| How many allocations, and where? | `dhat` heap profiler |
| Did the compiler vectorize / inline this? | `cargo asm`, `cargo-show-asm` |
| Is it the algorithm? | count operations; try a 10x larger input |

Benchmark in release mode, on realistic inputs, and report the change with its
noise (criterion prints the confidence interval). An unmeasured "should be
faster" is not a finding.

## Allocations in loops

Each `Vec::new()` + push, `String::new()`, `format!`, `.to_string()`,
`.to_vec()` or `Box::new` inside a loop body is a trip to the allocator per
iteration. Hoist and reuse:

```rust
// Before: one String per line, plus one per item
for order in orders {
    let mut line = String::new();
    for item in &order.items {
        line.push_str(&format!("{item}, "));
    }
    out.push(line);
}

// After: one reusable buffer, written in place
use std::fmt::Write;

let mut line = String::new();
for order in orders {
    line.clear();
    for item in &order.items {
        let _ = write!(line, "{item}, "); // writing to a String cannot fail
    }
    out.push(line.clone()); // or emit `&line` directly if the consumer can borrow
}
```

The scanner does not flag values passed to `push` / `insert` / `send`: that
allocation is the thing being stored. It does not flag code under `return` /
`break` or in error-path closures (`map_err`, `with_context`, ...).

## Clones in loops

See [ownership.md](ownership.md) for the alternatives to cloning. In a loop,
the usual fixes are: iterate by reference, collect references
(`Vec<&T>`, `HashMap<&str, _>`), or move out of the source with
`into_iter()` / `drain(..)` when it is not needed afterwards.

## Size things up front

```rust
let mut out = Vec::with_capacity(input.len());
let mut map = HashMap::with_capacity(expected);
let mut s = String::with_capacity(64);
```

Prefer `extend` / `collect` from an exact-size iterator; they reserve once.

## Iterate, don't index

`for x in &v` and iterator chains let the compiler drop bounds checks;
`for i in 0..v.len() { v[i] }` often keeps them. Use `chunks_exact`, `zip`,
`windows` instead of manual index arithmetic.

## Avoid intermediate collections

```rust
// Before: allocates a Vec just to count
let n = items.iter().filter(|i| i.active).collect::<Vec<_>>().len();
// After
let n = items.iter().filter(|i| i.active).count();
```

## Hashing

`HashMap` uses SipHash, which resists HashDoS. For internal keys that
attackers do not control, `rustc-hash` (`FxHashMap`) or `ahash` is usually
2-5x faster. Measure.

## Dispatch

Generics (`impl Trait`) monomorphize and inline; `Box<dyn Trait>` costs an
allocation and an indirect call. Use `dyn` for heterogeneous collections and
to cut compile times, generics in hot inner loops.

## Release profile

```toml
[profile.release]
lto = "thin"        # or "fat" for the last few percent
codegen-units = 1   # better optimization, slower builds
panic = "abort"     # if you never catch panics
```

Measure each change; `opt-level = 3` is already the default.

## `/hot-path` checklist

1. Write or find a criterion benchmark for the function; record the baseline.
2. Profile (flamegraph / dhat) to find where time and allocations go.
3. Change one thing at a time; re-run the benchmark after each.
4. Keep only changes that win beyond the noise; revert the rest.
5. Run the tests. Report a before/after table with the numbers.
