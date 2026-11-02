# T211 评审

- 评审人：编排者
- 日期：2026-11-02
- 结论：**通过，合并**（PR #28）

## 核对

- **规模**：10 个文件，+49 −35，共 84 行，停止线 120 以内。
- **CI**：
  - PR #28 https://github.com/boris-w/cupper/pull/28 ，ca71602 上 8 个检查全绿（run 41379207914）。
  - nightly https://github.com/boris-w/cupper/actions/runs/41378837111 （workflow_dispatch，ca71602）6 个作业全部通过：
    - `security-peer (macos-15)`：macOS 15.8.2，摘要记作 IT-sys-peer-macos15；
    - `security-peer (macos-26)`：macOS 26.6.2，记作 IT-sys-peer-macos26；
    - 另有 differential、e2e、compat-framework、codex-drift。
- **个人信息**：改动的文件过了敏感词清单，命中的只有仓库链接 `boris-w/cupper`（已知误报）。

## 逐项意见

1. **按"是否在托管 runner 上"归类，不只看大版本**：作者机器就是 macOS 26，只看大版本会把本机结果记作 -macos26，和计划 1.8 冲突。这条判断是编排者同意的，5 种组合都有单元测试覆盖。
2. **只跑一次 nightly**：同时核实了 runner 可用并完成了验收，省掉一次全量。
3. **提交拆分合理**：xtask 改名和 workflow 改名放在同一个提交里，每个中间提交上 nightly 都能跑。

## 记下的点

- **`cupper-sys.yml` 在 macos-26 上还没真正跑过**：它只在改动 `crates/cupper-sys/**` 的 PR 上触发。已写进交接事项：下一个改 cupper-sys 的 PR 合并前，要看到它在 macos-26 上通过。
- **Rust 二进制的最低版本**：没有设置 deployment target，Mach-O 里的 minos 还是 rustc 的默认值，和 `LSMinimumSystemVersion` 15.0 不一致。这不影响运行，先不另开任务，交接给 T160（自用版打包）一起定。
- **编排者负责的文档里过时的三处**（10-release 第 11 行、01-repo-build-ci 第 125 行、ids.toml 第 116–117 行的注释）：已在 main 上改掉（298b2da）。
