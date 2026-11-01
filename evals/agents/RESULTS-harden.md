# Agent evals: `/harden`

Claude Code headless, Sonnet. Each run writes the code from a prompt that states the goal
without listing the traps (`prompt-goal.md`), with the edit hook on, then runs
`/harden src/lib.rs` in a fresh session. Runs `20261031-150852` and `20261031-154421`.

The hidden tests passed before and after `/harden` in every run, so they cannot measure it.
This counts bugs instead: each one below was reproduced with a separate test, outside the
agent, on the code before and after.

| Task | Valid runs | Runs with a real bug | Reported but not reproducible |
|---|---:|---:|---:|
| `pool` | 3 | 3 | 0 |
| `chunked` | 3 | 0 | 0 |
| `flight` | 1 | 0 | 0 |

Bugs in `pool`:

| Bug | Run 1 (pilot) | Run 2 | Run 3 |
|---|---|---|---|
| `submit` from a job with the queue full deadlocks | fixed | mentioned, not tested or fixed | confirmed, left open for the user |
| `shutdown` from two jobs at once deadlocks | not reported | fixed | not present in that code |
| `Pool::new(usize::MAX, 1)` panics | not reported | fixed | not reported |

**Limits.** Seven runs is few, and they are uneven: `flight` lost one run to a permissions
problem in the eval, and a usage limit ended the rest of the planned runs. The same task
gives different findings from one run to the next. `/harden` cost about $0.15 to $0.31 per
session on these tasks.
