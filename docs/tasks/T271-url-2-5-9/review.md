# T271 评审

- 评审人：编排者
- 日期：2026-11-23
- 结论：**通过，合并**（PR #132）

## 核对

- **规模**：只改 `Cargo.lock`，+4 −4，代码改动为 0。
- **CI**：PR #132 https://github.com/boris-w/cupper/pull/132 ，7fd7228f 上 8 个检查全过（run 45120283673），`license` 不再报 RUSTSEC-2026-0592。
- **合并时机**：它挡着 PR #128–#131 的 `license`，8 个检查一过就先合并，汇报随后存档；合并后我把 main 合进这四个分支重跑。

## 逐项意见

1. **`url` 是间接依赖**（reqwest、tower-http），用 `--precise` 只升补丁版，`Cargo.toml` 不动。
2. **`form_urlencoded` 1.2.3 是 url 2.5.9 的要求**，躲不开，已在汇报里说明。
3. **不在 `deny.toml` 加忽略项**：直接升级没有风险。

## 记下的点

- 本机没装 `cargo-about`，`cargo xtask ci license` 的 `notices` 一步在本机跑不了；以 CI 为准，不在本机补装。
