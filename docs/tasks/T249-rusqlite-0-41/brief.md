# T249 升级 rusqlite 到 0.41（RUSTSEC-2026-0459）

- 派发日期：2026-11-09
- 执行者：子 agent（Rust 工程师，宿主存储）
- 分支：`fix/T249-rusqlite-0-41`
- 依赖：无。所有 PR 的 `license` 检查都被它挡住，优先做

## 背景

2026-11-09 起，PR 上的 `license` 作业里 `cargo-deny` 报 RUSTSEC-2026-0459（CVE-2026-119375）：

- SQLite 3.54.0 之前的版本，同时打开一个数据库文件和页大小不同的 `-wal` 文件时，可能返回未初始化的页缓存内存；
- `libsqlite3-sys` 0.6.0 到 0.38.2 打包的 SQLite 都受影响，修法是升到 0.39.0 以上。

宿主经 `rusqlite =0.40.2`（`bundled`）依赖 `libsqlite3-sys 0.38.2`。宿主只打开自己数据目录（0700）里的库，暴露面很小；但 `license` 是 main 的保护规则要求通过的检查，不修所有 PR 都合并不了。

rusqlite 0.41.1（2026-10-28 发布）依赖 libsqlite3-sys 0.39.x。

## 范围

**做**：

1. `crates/cupper-host/Cargo.toml`：`rusqlite` 改为 `=0.41.1`（精确版本，照代码约束），更新 `Cargo.lock`。
2. 0.40 → 0.41 的接口变化照改，不加兼容分支。先读 rusqlite 的 CHANGELOG，把影响到我们的变化记进 `notes.md`。
3. 核对 SQLite 的实际版本 ≥ 3.54.0。宿主启动时会读 `sqlite_version()` 的话，查一下现有的版本断言或记录。
4. `cargo deny check` 在本机通过（advisories、bans、licenses、sources）。
5. 宿主的全部测试通过，包括迁移、WAL、持久性相关的用例。

**不做**：改用系统 SQLite；其他依赖的升级。

## 验收标准

- [ ] `cargo xtask ci license` 通过，RUSTSEC-2026-0459 不再出现。
- [ ] 宿主的 lib 和集成测试都通过；clippy、fmt 干净。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：80；硬停止线：120（lock 文件不计）。接口变化比预想的大时先报 `main`。

## 参考资料

- https://rustsec.org/advisories/RUSTSEC-2026-0459
- 设计：5.2.8（事件存储、WAL 与持久性）、5.4.6（迁移）。

## 注意

- 本机只单份跑测试（守则第 10 条）。
- `gh` 只用只读命令；要推送时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
