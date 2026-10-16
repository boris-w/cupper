# SP-2 probe runs (acp-probe.mjs)

All runs: cwd = throwaway git repo under <tmp>, launchd-like env (PATH=/usr/bin:/bin:/usr/sbin:/sbin, no developer env vars),
client advertises no fs/terminal capability, permission requests would be answered "cancelled" (none occurred).
Prompt text: "Reply with exactly one word: pong." plus "Do not use any tools / run any commands."
Times in ms, measured by the probe from its own spawn of the adapter.

| run | adapter / packaging | runtime | agent CLI | scope | init ms | new ms | prompt ms | result | error (sanitized) |
|---|---|---|---|---|---|---|---|---|---|
| c1-noexe | claude-agent-acp 0.91.0, npm tree (omit optional) | system node 24.13 | none (CLAUDE_CODE_EXECUTABLE unset) | init + new | 165 | 13 |  | FAIL | session/new: code -32603 "Internal error" details: Claude native binary not found for darwin-arm64. Reinstall @anthropic-ai/claude-agent-sdk without --omit=optional, or set CLAUDE_CODE_EXECUTABLE. |
| c1-local-init | claude-agent-acp 0.91.0, npm tree (omit optional) | system node 24.13 | local claude 2.1.296 | init + new | 169 | 769 |  | OK |  |
| c1-local-prompt | claude-agent-acp 0.91.0, npm tree (omit optional) | system node 24.13 | local claude 2.1.296 | 1 turn (haiku) | 153 | 730 | 1941 | end_turn, text="pong" |  |
| c1-old2100-isolated | claude-agent-acp 0.91.0, npm tree (omit optional) | system node 24.13 | claude 2.1.200 (temp, isolated CLAUDE_CONFIG_DIR, not logged in) | init + new | 192 | 865 |  | OK |  |
| c1-old2100-isolated-prompt | claude-agent-acp 0.91.0, npm tree (omit optional) | system node 24.13 | claude 2.1.200 (temp, isolated CLAUDE_CONFIG_DIR, not logged in) | 1 turn attempt | 131 | 280 | 27 | FAIL | prompt: code -32000 "Authentication required" |
| c2-shim-cleanpath | codex-acp 2.2.2, npm tree (omit optional) | system node 24.13 | CODEX_PATH = npm shim (#!/usr/bin/env node), GUI PATH | init | 65 |  |  | FAIL | initialize: code 1001 "Codex process has exited with code 127: env: node: No such file or directory" |
| c2-0159-init | codex-acp 2.2.2 | system node 24.13 | local codex 0.159.2 native | init + new | 206 | 190 |  | OK |  |
| c2-0159-prompt | codex-acp 2.2.2 | system node 24.13 | local codex 0.159.2 native | 1 turn (gpt-5.4-mini, read-only) | 87 | 138 | 8508 | end_turn, text="{\"error\":{\"message\":\"The 'gpt-5.4-mini' model is not supported when using Codex with a ChatGPT account.\",\"type\":\"invalid_request_error\"}}" |  |
| c2-0159-prompt2 | codex-acp 2.2.2 | system node 24.13 | local codex 0.159.2 native | 1 turn (default model, effort low, read-only) | 231 | 204 | 7623 | end_turn, text="pong" |  |
| c2-0163-prompt | codex-acp 2.2.2 | system node 24.13 | codex 0.163.0 (temp, npm) | 1 turn (default model, effort low, read-only) | 160 | 117 | 10677 | end_turn, text="pong" |  |
| c2-0150-prompt | codex-acp 2.2.2 | system node 24.13 | codex 0.150.1 (temp, npm) | 1 turn (default model, effort low, read-only) | 223 | 191 | 5486 | end_turn, text="pong" |  |
| c2-0140-prompt | codex-acp 2.2.2 | system node 24.13 | codex 0.140.0 (temp, npm) | 1 turn attempt | 109 | 4 |  | FAIL | session/new: code -32000 "Authentication required" |
| c3-claude-E1node-bundle | claude esbuild single file | official node 24.22 re-signed, hardened runtime, allow-jit only | local claude 2.1.296 | 1 turn (haiku) | 276 | 831 | 1927 | end_turn, text="pong" |  |
| c3-codex-E1node-bundle | codex-acp single file | official node 24.22 re-signed, hardened runtime, allow-jit only | local codex 0.159.2 | 1 turn (low, read-only) | 278 | 225 | 29071 | end_turn, text="pong" |  |
| c3-claude-E0-jitless | claude esbuild single file | official node 24.22 re-signed, no entitlements, --jitless | local claude 2.1.296 | init + new | 256 | 807 |  | OK |  |
| c3-codex-E0-jitless | codex-acp single file | official node 24.22 re-signed, no entitlements, --jitless | local codex 0.159.2 | init + new | 432 | 241 |  | OK |  |
| c3-claude-sea | claude Node SEA | node 24.22 SEA, allow-jit only | local claude 2.1.296 | 1 turn (haiku) | 107 | 811 | 1725 | end_turn, text="pong" |  |
| c3-codex-sea | codex Node SEA | node 24.22 SEA, allow-jit only | local codex 0.159.2 | 1 turn (low, read-only) | 363 | 211 | 5437 | end_turn, text="pong" |  |
| c3-claude-bun | claude bun --compile | bun 1.4.3, allow-jit only | local claude 2.1.296 | 1 turn (haiku) | 343 | 890 | 1930 | end_turn, text="pong" |  |
| c3-codex-bun | codex bun --compile | bun 1.4.3, allow-jit only | local codex 0.159.2 | 1 turn (low, read-only) | 187 | 181 | 6128 | end_turn, text="pong" |  |
| c4-claude-bundle-clobber | claude esbuild single file; file replaced after session/new | system node 24.13 | local claude 2.1.296 | 1 turn (haiku) | 101 | 855 | 2144 | end_turn, text="pong" |  |
| c4-codex-bundle-clobber | codex single file; file replaced after session/new | system node 24.13 | local codex 0.159.2 | 1 turn (low, read-only) | 181 | 152 | 7468 | end_turn, text="pong" |  |
| c4-claude-multi-clobber | claude npm tree; node_modules moved away after session/new | system node 24.13 | local claude 2.1.296 | 1 turn (haiku) | 164 | 811 | 1920 | end_turn, text="pong" |  |
| c5-claude-runtime-final | claude single file from first-run installer | node 24.22 from installer (upstream signature) | local claude 2.1.296 | init + new | 2643 | 895 |  | OK |  |
| c5-codex-runtime-final | codex single file from first-run installer | node 24.22 from installer (upstream signature) | local codex 0.159.2 | init + new | 136 | 162 |  | OK |  |
