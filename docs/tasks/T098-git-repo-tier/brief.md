# T098 Git 服务 I-c：仓库档的执行

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主安全）。T054、T060 的执行者接手
- 分支：`feat/T098-git-repo-tier`
- 依赖：T060（已合并）、T097（合并中）。T034（索引）、T044 依赖本任务。**本任务在剩余关键路径上**

## 背景

T060 开工前拆成三份（计划 v1.3.16）：T060 做了 `GitCmd` 与三档类型，T097 做了仓库档的规则与指纹（纯函数）。本任务把两者接起来：带防护的仓库档执行入口。仓库档的每一次调用都要经过它（设计 5.2.10"仓库档：每次调用前读配置"第 1–6 条）。

## 范围

**做**：
1. **`RepoTier::run`**（放在 `git/` 下；T060 把执行入口命名为 `prepare`，`run` 留给本任务）：
   1. 调用前用 `GitCmd::repo(RepoOp::ConfigList)` 读有效配置与来源；
   2. `parse` → `check_sources`（来源在工作区内或其他 agent 可写的位置时拒绝）；
   3. `overrides` 算出覆盖，经 `.config(键, 值)` 加进命令；
   4. 调用前取一次指纹；
   5. `tier.prepare(cmd).run(clock, durations)` 执行；
   6. **覆盖后失败**：只有被置空的过滤器设了 `required=true`（`overrides` 的 `required` 不为空）时判为 `unsafe_repo_config`，其余按普通 git 失败返回；**都不去掉覆盖重试**；
   7. 调用后再取一次指纹：变了时，只读命令（`Prepared::read_only`）丢弃结果、返回 `repo_config_changed`；写操作照常返回结果；两种情况都给出要写 `notice` 的信号，带变化的 `paths`。
2. **`GitError`**：把 T097 的 `Unsafe` 各变体映射成 `git_error`（`reason: unsafe_repo_config`）；`repo_config_changed`；普通的 git 失败（stderr 末尾，脱敏后）；超时。错误码按协议的 `ErrorReason`。
3. **notice 信号**：结构化地交给调用方（会话 actor、git 方法的执行者），由调用方写进会话流；本任务不写会话流。
4. `core.hooksPath` 以 `~/` 开头时，传给 T097 的是展开后的路径（T097 的纯函数不读环境）。

**不做**：快照档（T061）；用户档（T123）；`RepoLock`；写会话流的 notice。

## 验收标准

- [ ] IT-git-repo-tier-malicious（由 T060 移来）：在临时目录里造出恶意仓库，覆盖 5.2.10 列出的每一类可执行配置（含 2026-10-20 补的 5 类，以及子段带 `=` 的键、`include.path` 指向工作区、`core.hooksPath` 指向工作区、`required=true` 的过滤器），每个"程序"一运行就留下标记文件；断言经 `RepoTier::run` 之后**没有任何标记文件**，并且该拒绝的拒绝（`unsafe_repo_config`）、该置空的置空、`required=true` 的过滤器报错后不重试、普通失败不被误判；调用中途改了配置时，只读命令得到 `repo_config_changed`、写操作照常返回，两者都给出 notice 信号和路径。
- [ ] 顺带记一次合成大仓库（例如几千个目录、若干 `.gitattributes`）上指纹前后两次的耗时，写进 `notes.md`；需要缓存时在汇报里提出（T060 的发现第 4 条）。
- [ ] git-lfs 用假的 git-lfs 脚本核实标准取值改写成绝对路径后 `clean`、`smudge` 能用；`filter-process` 记为未实测（Q32）。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T098 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T098-git-repo-tier/ci/` 通过，摘要随任务提交。

## 交接事项（已在 `handoff-notes.md`）

- T060、T097：`prepare`、`ConfigList`、`.config`、`read_only`；`parse` → `check_sources` → `overrides` 的调用方式；`global` 与 `command` 两组参数的位置；`Fingerprint::take` 与 `changed`。

## 注意

- **用例只在本任务自己建的临时仓库里运行 git**，绝不对本机真实的仓库（包括本仓库）运行；`HOME`、`GIT_CONFIG_GLOBAL`、`GIT_CONFIG_SYSTEM` 或 `GIT_CONFIG_NOSYSTEM` 都指向临时目录，免得读到本机的全局配置。仓库档的环境带 `HOME`（5.2.10），用例里的 `HOME` 就是临时目录。
- 恶意配置里的"程序"只是写标记文件的小脚本，不做任何其他事。
- 代码约束：子进程一律经 `cupper_sys::spawn`；时间只经 `Clock`；错误用 thiserror。

## 规模

计划估为 S，T060 的执行者估约 330 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 500 行（包括贴线）时，必须先停下来汇报**。

## 参考资料

- 设计：`02-host.md` 5.2.10（含 2026-10-20 的补充）；`03-protocol-v1.md` 的错误码与 notice；`13-open-issues.md` Q32。
- `docs/tasks/T060-git-cmd/`、`docs/tasks/T097-git-repo-rules/`（`notes.md`、`review.md`）。
- `crates/cupper-host/src/git/`。

## 工作方式

T097 合并之后，在原来的 T054 worktree 里从最新的 main 开新分支：`git checkout -b feat/T098-git-repo-tier main`。T060 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
