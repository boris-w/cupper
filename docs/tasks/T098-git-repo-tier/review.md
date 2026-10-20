# T098 Review

## 第 1 轮（2026-10-20，编排者）

- 结论：通过，合并。
- 过程：由 T060 拆出（计划 v1.3.16）。合并 main（没有冲突）后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过，摘要 `ci/pr-0a828202fa63.json`。执行者那次的 `pr-19ec03db400f.json` 一并保留。
- 核对：没有新依赖；用例只在临时目录里运行 git，`HOME` 和全局配置都在临时目录，系统配置用 `GIT_CONFIG_NOSYSTEM` 关掉；没有个人信息。
- 审阅：
  - **按 5.2.10 第 1–6 条执行**：读配置 → 来源检查 → 覆盖 → 前后指纹 → 判定。只读命令指纹一变就丢结果；写操作照常返回并带 notice；只有被置空的 `required=true` 过滤器导致失败才判 `unsafe_repo_config`，普通失败不误判。
  - **IT-git-repo-tier-malicious**：恶意配置覆盖第 4 条全部键和补的 5 类、fsmonitor、指向工作区的 hooksPath，加上常用 hooks 和让所有文件走恶意 filter/diff/merge 的 `.gitattributes`；依次调用只读命令、`update-index --refresh`（先改 mtime 逼 git 重算哈希）和 `worktree add`（检出），标记目录为空。变异检查（去掉覆盖）第一批调用就失败。
  - **路径统一用 realpath**：临时目录经 `/var` → `/private/var` 的链接，同一个 `.git/config` 曾在指纹里出现两次，`run` 一开始就取仓库的 realpath。好的发现。
  - **`GitCmd::global_args` 原样追加 `OsString`**：配置键可能不是 UTF-8，经 `String` 有损转换会让被置空的键对不上原键、等于没置空。这是一处真实的防护漏洞，修得对。
  - **写操作失败、配置也变了时**，变化路径放进 `Failed.changed`，`notice()` 照样给出。认可。
  - **指纹耗时**（合成仓库 5,000 个目录、20,000 个文件）每次约 70 ms；带大 `node_modules` 的仓库会到一秒级。M1a 仓库档调用不多，先不缓存；`git/status` 这类频繁调用的方法上线前要缓存，已记进交接。
- **规模与流程**：478 行（实现约 216、测试约 262），在约 500 的停止线以内但贴线。测试一次写完时明显超出估计，执行者没有先停下汇报，在汇报里如实说明了；功能完整，接受。
- 转交事项已写进 `handoff-notes.md`：T034、T044（调用方式、取 git 根目录、错误与 notice 的转换）、`git/status` 的任务（指纹缓存）、T097 的 `overrides` 与 T060 固定参数的重复（无害，以后可去重）。
