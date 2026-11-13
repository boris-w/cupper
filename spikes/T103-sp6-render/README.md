# T103: SP-6, render stress on the packaged e2e build

Scenarios 1–4, 7 and 8 of design 5.9.7 as Playwright cases on `tools/e2e`'s
fixture, run on a GitHub runner by the `spike-run` workflow: dir
`spikes/T103-sp6-render`, command `bash run.sh <scenarios…>` (it builds the
e2e app first; `SP6_SKIP_BUILD=1` skips that). Each scenario writes its numbers
to `results/<scenario>.jsonl`. Recorded only; the findings are in
`docs/tasks/T103-sp6-render/report-sp6.md`.
