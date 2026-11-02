# T099 notes

## Candidate and branch

- Exit candidate: `18b3640` on main (T215 merged and archived).
- This branch starts at `18b3640`. Against it, the branch may change only workflows and test tooling, and this task's folder; the nfr1 runs on the branch test the product code of `18b3640`. Each run's commit is checked with `git diff --stat 18b3640 <commit>` before it is counted.

## Estimate

- `e2e-repeat` (the only code change): the tests tagged `@focus` run after the rest one at a time, as `xtask ci e2e` runs them, so that the whole nfr1 suite runs at CI's parallelism in one call; a `load` input starts busy processes while the tests run (the slow variant). About +30 −10.
- The record, the notes and the committed summaries do not count.
- Total about 40, well under the stop line 200.
