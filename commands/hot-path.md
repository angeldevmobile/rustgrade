---
description: Optimize a critical Rust function, proving each change with a benchmark
argument-hint: "<function name or path::to::fn>"
---

Optimize the Rust function `$ARGUMENTS` as a performance engineer would: no
change ships without a measured win. Use the rustgrade skill and read its
`references/performance.md` first.

1. **Locate** the function and its callers. Read it fully. Note its inputs,
   allocations, clones, locks and loops. If `$ARGUMENTS` is empty or ambiguous,
   ask which function.
2. **Scan it.** If `rustgrade` is installed, run
   `rustgrade scan <file> --format json` and keep the findings whose `span`
   falls inside the function. Treat performance findings there as high
   priority.
3. **Baseline.** Find an existing criterion benchmark for the function, or
   write one in `benches/` (add `criterion` as a dev-dependency and a
   `[[bench]]` entry with `harness = false`). Use realistic input sizes. Run
   it and record the baseline.
4. **Profile** if the cause is not obvious: `cargo flamegraph` for time,
   `dhat` for allocations.
5. **Change one thing at a time**, highest expected impact first (algorithm >
   allocations > clones > bounds checks > micro-tuning). After each change:
   run the tests, re-run the benchmark, keep it only if it wins beyond the
   noise, otherwise revert it.
6. **Preserve behavior.** Public signatures and results stay the same unless
   the user agrees otherwise. Tests must pass at every step.
7. **Report** a table:

   | Change | Before | After | Δ |
   |---|---|---|---|

   with criterion's numbers, plus what was tried and reverted. Suggest
   `hot = ["<path>"]` in `rustgrade.toml` so future scans weigh this code.
