# 最低支持的 macOS 版本

- 日期：2026-11-02
- 参与：用户、编排者
- 形式：会话中的决定

## 背景

GitHub 在 2026-11-02 停用了 macOS 14 的托管 runner：10-30 起，nightly 里 `security-macos14` 在 `macos-14` 上的那一份就因停用前的限流被取消了，以后也跑不了（runner-images 的公告 #13518）。cupper 的最低支持版本原来是 macOS 14（U-T1，2026-10-16），对端校验（Q24）靠 nightly 在 macOS 14、15 上各跑一次来覆盖。仓库不用自托管 runner（计划 1.8），CI 上从此测不到 macOS 14。

## 决定

- **D1**：只支持最近两三年的 macOS，用户原话："只测试最新版本或者最近2-3年的macos版本，我对版本要求不高"。按 2026-11 的版本，是 macOS 15 和 macOS 26；最低支持版本从 macOS 14 提到 macOS 15，替换 U-T1 里的 14。
- **D2**（编排者据 D1 定）：
  - 对端校验的 CI 作业改名为 `security-peer`，每晚在 `macos-15` 和 `macos-26` 上各跑一次；改动 `cupper-sys` 的 PR 按路径过滤在 `macos-26` 上跑（macOS 15 由每个 PR 的 `rust` 作业覆盖）。
  - IT-sys-peer-macos14 标为不适用；新增 IT-sys-peer-macos26。
  - 构建产物的最低系统版本（`LSMinimumSystemVersion`、electron-builder 的 `minimumSystemVersion`）改成 15.0。
  - 以后新的 macOS 大版本发布、GitHub 停用更老的 runner 时，照同一原则往前移，不另问。

## 落实

- 设计、PRD 的界面规格、计划、`CLAUDE.md`、`tests/ids.toml`：编排者改（本次提交）。
- workflow、xtask、构建配置、README：T211。
