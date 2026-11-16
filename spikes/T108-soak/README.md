# T108: SP-6 scenario 5, the soak, on the packaged e2e build

A turn with a tool call and some streaming every 5 minutes for about 5 hours
(design 5.9.6, 5.9.7), on `tools/e2e`'s fixture, run on a GitHub runner by the
`spike-run` workflow: dir `spikes/T108-soak`, command `bash run.sh`, minutes
360. After each turn it samples the main process, the main document's and the
review document's renderers and the host, into `results/5-soak.jsonl`; the
last line sums up the growth after the warm-up. Recorded only; the findings
are in `docs/tasks/T108-soak/report-soak.md`.

Its environment: `SOAK_MINUTES` (by default what the job has left, see
run.sh), `SOAK_EVERY_SECONDS` (300), `SOAK_WARMUP_MINUTES` (30), and
`SP6_SKIP_BUILD=1` for a local smoke run on the build there is.
