# T002 汇报

- 执行者：子 agent（工程师，CI 与工具）
- 分支：`build/T002-test-ids`
- 完成日期：2026-10-19

## 做了什么

1. **`tools/trace-ids`**（TS 工具，跟 `tools/gen-protocol-ts` 一样由 Node 直接运行源码，新增开发依赖 `smol-toml` 1.9.0，BSD-3-Clause）：
   - `check`：做登记表的一致性检查。
     - 文档（`docs/03-plan.md`、`docs/design/*.md`）里每个形似编号的词都要登记，或者列进 `[not-ids]`。
     - 检查字段、套件、状态、里程碑的格式；检查任务确实出现在计划里。
     - 子编号要求：已登记，以父编号加 `-` 开头，只属于一个父编号，里程碑不晚于父编号；形似子编号却没被父编号列出时也报错。
     - MAN 证据要求：编号已登记，记录文件在 `docs/` 下，写明了所绑定组件的版本。
   - `status`：根据结果来源推导每个编号的状态，分为 done、settled、pending、problem 四类，另列出未登记编号的结果。结果来源有三种：`xtask ci --record` 写的摘要、cargo-nextest 的 JUnit 报告、Playwright 的 JSON 报告，后两种由调用方写明套件。
   - `verify --milestone <M> | --task <Txxx>`：所选编号必须全部完成或已了结，否则失败。
     - `--milestone` 把更早的里程碑也算进去。
     - `--milestone` 只接受已入库、未改动的摘要，而且必须来自同一个提交。
   - 推导规则：
     - 任何一处失败（包括 flaky）都算问题。
     - 在登记的套件里通过才算完成；只在别的套件里通过，记为"套件不符"。
     - 只有被跳过的用例，记为"被跳过"；什么都没跑，记为"没有测试"。
     - MAN 看最新一份证据：没有证据是待完成；证据日期晚于今天、超过 `max_age_days`、或与绑定组件的版本对不上，都算过期。MAN-keyboard 绑定 Electron 大版本，当前版本从 `apps/desktop/package.json` 读。
     - 子编号没全部了结，父编号就是待完成；父编号自己有测试结果，也报问题。
     - `gap`、`conditional` 的编号是待完成，并附原因；`na`、`gap-confirmed` 算已了结。
2. **`tests/ids.toml`**：按计划 v1.3.6 任务表的顺序登记 365 个编号，每个产出任务一段。
   - 已合并任务按实际产出登记，包括拆分新增的 UNIT-proto-events-index/-session、UNIT-proto-methods-core-stream/-session、UNIT-queue-core、UNIT-cli-client。
   - 按计划 1.6 登记了 13 个父编号，另加 UNIT-proto-events 和 UNIT-proto-methods-core 两个拆分产生的父编号。
   - IT-sys-peer-macos14、-macos15 登记为 `gap`；IT-q8-fallback-claude 为 `na`（K1 已通过）。
   - 条件任务（T079、T119、T139、T158、T162）的编号登记为 `conditional`。
   - `[unplanned]` 里放了 IT-n-minus-1-cli（later）和 MAN-tcc（M2）。
   - `[not-ids]` 里放了 PRD 的需求编号 UI-0 至 UI-7、两个旧名，以及 IT-K11 这个简写。
   - 文件头说明了格式。
3. **`xtask ci testdata-scan`**：新文件 `jobs/testdata_scan.rs`，先 `pnpm install`，再跑 `trace-ids check`。不再是 not implemented。
   - 另有一处一行的改动：`ids.rs` 的 `harvest` 也读 `tests/ids.toml`。这样任务自己新增、计划还没写进去的编号，摘要里也能拿到结果。
4. **UNIT-trace-ids**：19 个 Vitest 用例，`ts` 作业会自动运行。覆盖了 brief 要求的五种报错：
   - 没有测试；
   - 被跳过；
   - 套件不符；
   - MAN 证据过期（版本对不上、超龄、日期晚于今天、当前版本未知）；
   - 子编号未齐。
   此外还覆盖了失败、各种状态、未登记结果、按里程碑和任务筛选，以及登记表的格式和一致性错误，摘要、JUnit（含 flaky、CDATA、实体转义）、Playwright 三种结果的读取。

## 关键取舍

- **用 TS 写在 `tools/` 下，xtask 只加作业文件**：设计 5.1.2 把 trace-ids 列在 `tools/`，计划把它划在区域 C，brief 也要求 xtask 的改动放在作业自己的文件里。代价是"测试名对应到编号"的规则在 TS 里又实现了一遍，与 `ids::owner` 一致，两边各有对应的测试。
- **没有把 CI 改成 nextest**：本机没装 cargo-nextest。装它要改作者机器上的 `~/.cargo/bin`，应当由用户决定；而且 `rust`、`protocol`、`security-macos14` 三个作业都要跟着改，验收也不要求。所以 JUnit 读取按格式实现，用样例测试，与 Playwright 相同；样例是按 nextest 的格式手写的，没有拿真实报告核对。改用 nextest 时，按交接事项替换 `parse_libtest`，摘要格式不变，trace-ids 不用改。
- **套件就是 `xtask ci` 的调用目标**（`pr`、`nightly`、`release`），另加 `compat` 和 `man`。结果的套件必须与登记的完全一致：e2e 在 PR 上只跑冒烟子集，所以在 nightly 里通过不能代替 pr。
- **摘要不记被跳过的用例**：没有改 T001 的摘要格式。从摘要读时，只有被跳过用例的编号显示为"没有测试"，同样算没完成；"被跳过"这个原因只在读原始报告时给出。
- **一个编号里有跳过也有通过时算完成**：现有代码里有被 `#[ignore]` 的子进程辅助用例，严格判定会误报。

## 验证方式

- 在干净 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T002-test-ids/ci/`，全部通过：rust 947、protocol 176、ts 251（含 test-trace-ids）、license 1、testdata-scan 2；ui、e2e 尚未实现。摘要 `pr-fe3959c331e6.json` 已提交，其中 UNIT-trace-ids 为 pass。
- `trace-ids check`：365 个编号，与文档一致。手动删掉一行、写一个错误的套件，都会报错并以 1 退出。
- 对 brief 指定的 main CI 摘要（T042 合并后，`pr-8d509e5d3848.json`）跑 `status`：
  - 已完成 57 个：55 个有结果的编号，加上两个父编号 UNIT-proto-events 和 UNIT-proto-methods-core；
  - 已了结 1 个（IT-q8-fallback-claude）；
  - 待完成 307 个，其中 IT-sys-peer 在等两个缺口；
  - 问题 0 个。
  对现在 main 上最新的摘要（T041）跑，已完成 63 个，T039、T041 的编号都在里面。
- `verify --milestone M1a` 读同一份摘要：226 个里有 168 个没完成，以 1 退出。传入未入库的摘要、不同提交的两份摘要，都会被拒绝。
- 对本分支的摘要跑 `verify --task T002`：通过。
- 已检查本分支的全部 diff，没有本机路径、用户名或邮箱。

## 提交列表

```
c3dd47f docs(task): record T002's pr CI summary
fe3959c docs(task): T002 line counts, uncertain registrations and the v1.3.9 sync
281c1bc build(trace-ids): lay out the verify options of the usage text
3ac4012 ci(xtask): run the test ID registry check in testdata-scan
40e2332 test: register every test ID of the plan and the design in tests/ids.toml
b21215f build(trace-ids): check the test ID registry and derive done and pending IDs
6a98d41 docs(task): T002 approach and line estimates
```

规模：不含 lock 文件和 `tests/ids.toml`（941 行数据），共新增 1,443 行、删除 4 行，其中测试 509 行。开工前估计约 1,210 行，超过了计划的 M，但在约 1,500 行的上限以内。工具写完后统计为 1,430 行，按剩余部分推算约 1,470，所以没有停下来。逐次统计见 `notes.md`。

## 遗留问题 / 范围外发现

1. **合并前要和计划 v1.3.9 同步**（需要编排者处理）：本分支从 v1.3.6 开出，main 上已拆出 T019、T029、T058、T087，并新增了 IT-gw-blob。合并 main 后 `testdata-scan` 会报 IT-gw-blob 未登记。`notes.md` 末尾有可以直接贴的 TOML：
   - IT-gw-subscribe 移到 T019；
   - IT-gw-blob 登记到 T029；
   - PROP-autonomous-dequeue-guard、PROP-turn-snapshot-immutable 移到 T058；
   - UNIT-hc-sync 移到 T087。
2. **拿不准的登记**（详见 `notes.md`），请编排者确认：
   - IT-handshake 由 T030、T039 两个任务完成，但计划没有拆子编号。现在登记在 T030，摘要里的"通过"不能说明密钥部分也在；需要的话拆成子编号。
   - 5.12.4 没标套件的 E2E、IT-selfuse-build、UNIT-signing-check 等，是我自己定的套件。
   - COMPAT 记为 `compat` 套件，而 5.12.4 标的是〔人〕。在 T064 给 trace-ids 加上读兼容报告的入口之前，COMPAT 编号会一直是待完成。
   - 父编号登记在完成它的任务下。
3. **交给后续任务**：
   - T064：给 trace-ids 加 `--compat <报告>`。
   - T173：MAN 证据的格式现在先定为 `[[evidence]]`，包括 id、date、record、versions，可以在此基础上改。
   - 改用 nextest 的任务：替换 `parse_libtest`，并考虑在摘要里记"skip"。
   - T053、T110：自行追加"其余录制"的 GOLD-A 编号。
4. **文档不一致**（范围外，没改）：
   - 设计 5.1.6 的 testdata-scan 一行仍写 `tests/pending-ids.toml`。
   - 计划 5.3、5.4 仍把 PROP-queue-fifo、PROP-interrupt-10s、PROP-turn-single-prompt、PROP-prompt-inflight-no-end 归给 T040，任务表从 v1.3.3 起已归 T048。
   - 计划 T140 的注里写的是简写 IT-K11，改成完整编号后可以从 `[not-ids]` 删掉。
5. **要不要装 cargo-nextest**，由用户决定（计划 1.6 写的是 nextest）。

## 追加：与计划 v1.3.9 同步

已就绪。新摘要是 `docs/tasks/T002-test-ids/ci/pr-122d2d837568.json`，旧摘要 `pr-fe3959c331e6.json` 保留。

- **v1.3.9 同步**（`122d2d8`）：按你列的四处改好了。逐条对过当前任务表：除了按你的裁定只登记一次的编号以外，`tests/ids.toml` 与任务表一致，没发现别的变化。T033、T071、T041 的验收与你说的一致。各段标题已改成当前的任务名。
- **IT-handshake**：留在 T030，不拆子编号，注释写"T039 补上了密钥认证部分"。
- **notes.md**：同步一节标为已应用，第 1 条按你的裁定改写。
- **验证**：现在登记了 366 个编号，`trace-ids check` 通过。干净 worktree 里的 `cargo xtask ci pr` 全部通过：rust 1035、protocol 176、ts 251、license 1、testdata-scan 2，ui 和 e2e 尚未实现。license 作业这次没有因网络失败。
- **新摘要的 status**：完成 66、已了结 1、待完成 299、问题 0。
- 摘要提交是 `e86d463`，worktree 干净。
