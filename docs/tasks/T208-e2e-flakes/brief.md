# T208 e2e 的两处偶发失败：退出后的 status 与重载后的历史

- 派发日期：2026-10-30
- 执行者：子 agent（测试工程师）
- 分支：`fix/T208-e2e-flakes`
- 依赖：无（T206 发现）。T099 依赖本任务

## 背景

两个 e2e 用例在 GitHub runner 上各失败过一次：

1. **`E2E-quit-confirm: idle, quitting asks nothing and leaves the host to its idle rule`**（`tools/e2e/src/app.spec.ts:86`）
   - 10-28 的 nightly e2e 全集：https://github.com/boris-w/cupper/actions/runs/40779645990 ；
   - `app.spec.ts:96` 调 `cupper status`，它以退出码 3 结束（`cli.ts:122` 抛出 "cupper status: exit 3"）；
   - 失败用例附了 main.log、host.log、adapter-claude.log，在这次运行的构件里。
2. **`E2E-NFR1-4: after a reload, earlier history comes in front without moving the view`**（`tools/e2e/src/nfr1.spec.ts:215`）
   - main 的 push 运行：https://github.com/boris-w/cupper/actions/runs/40651531549 ；
   - `locator.click` 等满 30 s，附近是 `nfr1.spec.ts:236` 的 `expect(main.getByText("say t3", { exact: true })).toHaveCount(1)`。

## 范围

**做**：
1. 两处都查明根因，写进 `notes.md`，附证据（runner 上的日志和附件，本机单份复现）。
   - 第 1 处：先弄清退出码 3 在 CLI 里是什么意思，再看那一刻宿主处在什么状态：已经退出、正在退出，还是没响应。用例要验的是"空闲时退出不弹确认，宿主照空闲规则自己退出"，判断 status 的时机对不对；
   - 第 2 处：点不到的是哪个元素、为什么。
2. 修复：根因在测试就修测试，等的东西要等对；根因在产品就修产品，并补能稳定复现的用例。产品问题超出一个小改动时，停下汇报。
3. 在 runner 上用 `e2e-repeat` 连跑证明：两个用例修复前各量一次失败率，修复后各连跑 100 次以上没有失败。要触发时，把参数和提交发给 `main`。

**不做**：别的 e2e 用例。

## 验收标准

- [ ] 两处的根因都有证据。
- [ ] 修复后 runner 上两个用例各 100 次以上全部通过，附运行链接。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：150；硬停止线：200

<!-- 执行者开工前在 `notes.md` 里写估计。每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.7.1（退出与宿主的空闲规则）；5.2.13；5.9.4（e2e）；5.9.1（屏障要来自被断言的那条流）。
- 卷宗：T007（退出确认与重启宿主）；T090、T093（e2e 套件）；T199、T200（e2e 的偶发失败和 `e2e-repeat` 的用法）；T195（不带 `@focus` 的用例不抢焦点）。
- `docs/03-plan.md`：T208 一行。

## 注意

- 不运行真实的 Claude Code、Codex CLI；不读、不改 `~/.claude`、`~/.codex`。
- 本机 e2e 只单份跑；带 `@focus` 的用例运行期间要求没人操作机器，本机不跑这类用例，只在 runner 上跑（守则第 10 条）。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
