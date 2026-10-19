# T074 修复第 1 轮：测试的临时目录没有清理

- 派发日期：2026-10-19
- 执行者：子 agent。与 T021 修复第 2 轮是同一位执行者。
- 分支：`fix/T074-test-tmpdir-cleanup`
- 依赖：T074（已合并）

## 背景

编排者排查用户临时目录时，发现今天留下了 48 个 `cupper-app-protocol-*` 目录，每次 `ts` 作业留一个。来源是 `apps/desktop/src/main/hardening.test.ts` 第 181 行附近：`app protocol` 一组用例在 `beforeAll` 里用 `mkdtemp` 建目录，但没有对应的清理。

## 范围

**做**：
- 给这组用例补上清理：用 `afterAll` 加 `rm(dir, { recursive: true, force: true })`，或者同等的写法。
- 排查 `apps/` 和 `packages/` 下所有测试里的 `mkdtemp`、`tmpdir()`，把同类遗漏一并补上。
- 排查 Rust 测试里的临时目录有没有同类问题：
  - 有 `Drop` 清理、只在进程被杀时才会残留的，不算问题，在汇报里说明即可；
  - 每次运行都必然残留的，要修。

**不做**：
- 删除用户临时目录里已有的残留，这件事由编排者在合并后做。
- 改测试的语义。

## 验收标准

- [ ] 修复后连跑 `pnpm test` 5 次，用户临时目录里不再新增 `cupper-app-protocol-*`。验证时按运行前后的目录清单比对，不要删除已有的目录。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T074-desktop-main-skeleton/ci/` 通过，摘要随任务提交。

## 规模

S，预计 50 行以内。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
