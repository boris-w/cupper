# T104: the review document in a cross-origin iframe (Q29)

The prototype of design 5.7.5 for Q29 and SP-6 scenario 9. Run it with
`npm ci --ignore-scripts && npm start` (one run, into `results/local.json`)
or `npm run measure -- <runs>` (cold runs, then `results/summary.json`),
after the workspace's `pnpm install`: it uses `apps/desktop`'s Electron and
imports its protocol, CSP, window, permissions and input guard. Findings:
`docs/tasks/T104-review-iframe/report-q29.md`.
