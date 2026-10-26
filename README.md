# cupper

A macOS desktop client for coding agents. It recreates the interface of the Codex desktop app and drives the Claude Code and Codex CLIs installed on your machine through the [Agent Client Protocol](https://agentclientprotocol.com). An iOS companion with end-to-end encryption is planned.

Status: early development (milestone M1a). Nothing is released yet.

## Layout

| Path            | What                                                                                                  |
| --------------- | ----------------------------------------------------------------------------------------------------- |
| `crates/`       | Rust: the session host, its protocol, system interfaces, the CLI, a fake agent for tests, and `xtask` |
| `apps/desktop/` | The Electron app                                                                                      |
| `packages/`     | TypeScript libraries shared by the app                                                                |
| `protocol/`     | The host protocol's schema and samples                                                                |
| `docs/`         | PRD, technical design, plan, decisions and the record of every task (in Chinese)                      |

## Building and testing

```sh
pnpm i
pnpm gen          # export the protocol schema and generate the TypeScript types
pnpm test
cargo xtask ci pr # every check a pull request runs
```

macOS 14 or later is required.

## License

[Apache-2.0](LICENSE). Anthropic's SDKs and the Claude Code binary are never part of this repository or its builds.
