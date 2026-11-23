# T271 汇报

- 执行者：子 agent（CI 与工具）
- 分支：`fix/T271-url-2-5-9`
- 完成日期：2026-11-23
- PR：https://github.com/boris-w/cupper/pull/132
- CI：https://github.com/boris-w/cupper/actions/runs/45120283673 。`7fd7228f` 上 8 个检查全部通过；`license`（job 137512232564）不再报 RUSTSEC-2026-0592。

## 做了什么

1. **确认 `url` 只是间接依赖**：
   - 所有 `Cargo.toml` 里都没有 `url`。
   - `cargo tree -i url` 显示它只经 `reqwest 0.12.28` 和 `tower-http 0.6.11`（后者也是 reqwest 带进来的）进入宿主。
2. **`cargo update -p url --precise 2.5.9`**：只改了 `Cargo.lock`，+4 −4。
   - `url` 2.5.8 → 2.5.9。
   - `form_urlencoded` 1.2.2 → 1.2.3，这一处躲不开：url 2.5.9 在自己的 `Cargo.toml` 里要求 `form_urlencoded` 至少 1.2.3。它另有一条路径是 `serde_urlencoded`（也经 reqwest），用 1.2.3 不受影响。
   - 没有别的包变动。没有升级 reqwest，也没有改 `deny.toml`。

## 关键取舍

- **只改 lock 文件**：`url` 不是直接依赖，用 `--precise` 把升级限制在补丁版本内，`Cargo.toml` 不用动，reqwest 等上游也保持原版本。
- **不加忽略项**：宿主不对不可信输入调用 `Url::origin()`，暴露面虽然很小，但直接升级既简单又没有风险，没有理由在 `deny.toml` 里加忽略。

## 验证方式

- **PR #132 上 8 个检查全部通过**（run 45120283673）：license、rust、e2e、ui、protocol、ts、testdata-scan、compat-framework。
- **本机**（都是单份）：
  - `cargo deny check`：advisories ok、bans ok、licenses ok、sources ok。
  - `cargo clippy -p cupper-host --all-targets --all-features --locked -- -D warnings`：干净。
  - 宿主 lib 用例 537 条通过。
  - 本机跑 `cargo xtask ci license` 时，cargo-deny、license-gate、metafile 等 6 步通过；只有 `notices` 一步因为本机没装 `cargo about` 而失败，与改动无关。CI 上这一步通过了。
- **对照验收标准**：
  - [x] `cargo deny check` 通过，`Cargo.lock` 里 `url` 是 2.5.9。
  - [x] PR 上 8 个检查全部通过，链接见上。

## 规模

只改了 lock 文件，代码改动为 0。

## 提交列表

```
7fd7228f fix(deps): url 2.5.9 for RUSTSEC-2026-0592
```

## 遗留问题 / 范围外发现

- **PR 排队很久**：PR #132 的 macOS 检查排了一个多小时才开始，`license`（ubuntu）也等了很久，可能是几个 PR 同时排队。这和本任务无关，只记一笔。
- **本机缺 `cargo about`**：`cargo xtask ci license` 的 `notices` 一步在本机跑不了。要在本机跑全套 license 检查，需要装 `cargo-about`。
- **T269**：PR #128 等这个修复合并后，关掉重开或重跑检查（PR 重跑会用旧的 merge ref，见记忆里那条）即可收尾。T269 的 worktree T052 没有动，分支停在 `b8be56c0`。
- T054 worktree 保留着。
