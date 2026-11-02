# T211 汇报

- 执行者：子 agent（工程师）
- 分支：`ci/T211-macos15-floor`
- 完成日期：2026-11-02（只差 PR 和它的 8 个检查：按编排者的安排，nightly 核实完再开 PR）

## 做了什么

1. **核实 `macos-26` runner 可用**：可用。nightly 运行 https://github.com/boris-w/cupper/actions/runs/41378837111 （workflow_dispatch，提交 ca71602）里，`security-peer (macos-26)` 通过：https://github.com/boris-w/cupper/actions/runs/41378837111/job/125064943081 。日志显示镜像 `macos-26-arm64`，镜像版本 20261026.0455.2，系统 macOS 26.6.2（25G83）。
2. **作业改名为 `security-peer`**：
   - xtask 的作业表和 `nightly` 组合（`crates/xtask/src/ci/jobs/mod.rs`），以及 `jobs/security.rs` 的文档注释。
   - `nightly.yml`：改成在 `macos-15`、`macos-26` 上各跑一次，注释同步。
   - `cupper-sys.yml`：作业改名为 `security-peer (macos-26)`，`runs-on: macos-26`，注释写明 macOS 15 由每个 PR 的 `rust` 作业覆盖。
   - 权限仍是 `contents: read`，action 的 SHA 锁定没动。
3. **摘要的编号归类**（`ci/summary.rs`）：
   - 在 GitHub 托管的 runner 上（`RUNNER_ENVIRONMENT=github-hosted`），macOS 15 记作 IT-sys-peer-macos15，macOS 26 记作 IT-sys-peer-macos26。
   - 其他地方不论版本一律记作 IT-sys-peer-current。
   - 实现方式：`summary::Run` 加了 `hosted: bool` 字段，在 `ci/mod.rs` 里填。
   - 单元测试 `unit_ci_summary_peer_checks_count_for_the_macos_they_ran_on` 覆盖 5 种组合：托管 15 → macos15，托管 26 → macos26，托管 14 → current，本机 26 → current，本机 15 → current。
4. **构建产物的最低系统版本改为 15.0**：
   - `crates/xtask/src/host_bundle.rs` 的 `LSMinimumSystemVersion`，连同它的测试。
   - `apps/desktop/electron-builder.dev.yml`、`electron-builder.e2e.yml` 的 `minimumSystemVersion`。
   - 仓库里没有其他写着 14.0 的最低版本。
5. **README** 改为 "macOS 15 or later is required."

## 关键取舍

- **按是否在托管 runner 上归类，不只看大版本**：作者机器就是 macOS 26.4，只看大版本的话，本机跑的 `security-peer`（包括编排者本机的 `xtask ci nightly`）会记作 -macos26，和计划 1.8"本机只记作 IT-sys-peer-current"冲突。编排者已同意这个做法，并要求有单元测试覆盖，现在已覆盖。环境变量只读取一次，没有单独写测试：Rust 里设置环境变量是 unsafe，这条判断本身在纯函数 `on_os` 里测。
- **只跑一次 nightly**：在 ca71602 上只触发一次 nightly，同时完成第 1 步（核实 runner）和第 5 步（验收）。
- **提交拆分**：xtask 改名和 workflow 改名放在同一个提交里，否则中间那个提交上 nightly 会调用一个不存在的作业。

## 验证方式

- 本机：
  - `cargo test -p xtask --locked`：61 + 1 个测试全部通过。
  - `cargo clippy -p xtask --locked --all-targets -- -D warnings` 干净；`cargo fmt --check` 干净。
  - 改动的 yml 和 README 过了 `prettier --check`。
- 验收标准逐条：
  - [x] **`macos-26` 可用的证据**：上面的运行链接，系统 macOS 26.6.2。
  - [x] **`security-peer` 在两个 runner 上都通过，摘要编号正确**：
    - macos-26：18 个测试通过；摘要的 `os` 是 "macOS 26.6.2"，`results` 是 IT-sys-peer-macos26 pass 和 UNIT-sys-uniqueid pass，suite 是 nightly。
    - macos-15（https://github.com/boris-w/cupper/actions/runs/41378837111/job/125064943109）：18 个测试通过；摘要的 `os` 是 "macOS 15.8.2"，`results` 是 IT-sys-peer-macos15 pass 和 UNIT-sys-uniqueid pass，suite 是 nightly。
    - 同一次运行里 differential、codex-drift、compat-framework 也都通过了；交接时 e2e 还在跑。
  - [x] **构建产物的最低系统版本都是 15.0**：见上面第 4 项。
  - [ ] **PR 上 8 个检查都通过**：还没有 PR。等编排者推 ca71602 开 PR 后再看。
- 规模：10 个文件，+49/−35，共 84 行。停止线 120。

## 提交列表

```
ca71602 build: raise the minimum system version to macOS 15
9d0910b ci: rename security-macos14 to security-peer, run on macOS 15 and 26
b2f1d5b ci(nightly): run the peer checks on macos-26 instead of macos-14
```

## 遗留问题 / 范围外发现

- **还没收尾的一项**：开 PR，看 8 个检查；nightly 的 e2e 结果也还要看。
- **`cupper-sys.yml` 这次没有真正跑过**：它只在改动 `crates/cupper-sys/**` 的 PR 上触发，这个分支没改 cupper-sys。下一次改 cupper-sys 的 PR 才会第一次在 macos-26 上跑它。
- **文档里过时的说法**（编排者负责的文件，我没有改）：
  - `docs/design/10-release.md` 第 11 行还写着 "LSMinimumSystemVersion 14.0"。
  - `docs/design/01-repo-build-ci.md` 第 125 行还写着 "`security-macos14` 在 `macos-14`、`macos-15` 上都跑"。
  - `tests/ids.toml` 第 116–117 行的注释还写着 "macOS 14、15 … 记作 -macos14、-macos15"。
- **为 macOS 14 写的兼容分支**：在产品代码里没有找到。
- **测试样例**：`tools/trace-ids` 的测试样例里用 IT-sys-peer-macos14 / "macOS 14" 当示例数据。这些只是解析器的样例，所以没动。
- **Rust 二进制的最低版本**：没有设置 deployment target，Mach-O 里记录的 minos 还是 rustc 的默认值，不是 15.0。如果想让它和 `LSMinimumSystemVersion` 一致，需要另开任务。
