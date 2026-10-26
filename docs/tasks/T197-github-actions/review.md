# T197 评审

- 评审人：编排者
- 日期：2026-10-26
- 结论：**通过，两个 PR 都已合并**（PR #1 → 8df33d4，PR #4 → 5aa8955）

## 核对

- **规模**：两轮合计新增 608 行，越过停止线 600，没到硬停止线 700。多出来的主要是编排者中途加的 e2e 急修。执行者写完映射才合计出来，没有每写完一个模块统计一次（守则第 8 条）。编排者接受，不拆分，上限改为 620/700。
- **CI**（全部在 GitHub 上）：
  - PR #1：https://github.com/boris-w/cupper/actions/runs/40311848751 ，8 个检查全过；
  - PR #4：https://github.com/boris-w/cupper/actions/runs/40323986038 ，ts 第一次挂在 T198 记下的 `settle()` 固定等待上，编排者只重跑了这一个作业，第二次全过；
  - 分支上的 nightly：https://github.com/boris-w/cupper/actions/runs/40323977621 ，全部通过，e2e 全集 48 个用例；
  - E2E-SEC-real-input × 50：https://github.com/boris-w/cupper/actions/runs/40315247047 ，50/50，补完了 T189。
- **安全**：推送前编排者逐个核对了 action 的 SHA 与官方 tag；权限只读；没有 `pull_request_target`；`workflow_dispatch` 的输入经环境变量传入脚本。仓库只允许 GitHub 官方的 action 和 `Swatinem/rust-cache`，并要求锁定到 SHA。

## 逐项意见

1. **一个 `ci` workflow 加 `--in pr`**：跳过规则只在 xtask 一处；检查名 `ci / <作业> (<runner>)` 每个名字只出现一次，main 的保护规则就勾这 8 个。
2. **托管 runner 上身份名单为空**：只按 `RUNNER_ENVIRONMENT=github-hosted` 判断，本机照旧；通用模式的扫描两边都照旧。
3. **ui 基线与 PERF**：两条规则改动都经编排者认可。夹具自带字体，像素与 macOS 版本无关，这次逐字节相同就是证据；绝对预算只在参考硬件上判（设计 5.9.6）。
4. **e2e 急修**：根因查到了 rust-cache 清理 `target/` 时留下目录骨架这一层。"就绪"改为按清单核对文件与哈希，与 host-bundle 共用一处检查，本机和 CI 走同一份代码。stage 不进任何缓存，ADR-11 照旧。
5. **IT-sys-peer-macos14/15**：由 xtask 按 `facts::os` 归类，不靠 workflow 传参；新的 nightly 能推导出两项都已完成，计划 R7 的缺口补上了。
6. **security-macos14 失败时不再开 issue**：开 issue 要写权限，与"全部只读"冲突，认可。夜间失败由编排者每天看运行结果。

## 记下的点

- **计划 1.8 按有远端改写**：执行者在 `notes.md` 里逐行写了建议；T084 的"macOS 只比对大版本"在计划里有三处（1.8 的截图基线行、T118 行、R9），设计 5.1.6 的"截图基线由专用脚本在本机生成"已过时。编排者下一次整理文档时一并改。
- **T099**：20 次连跑用 nightly 的 e2e 全集或 `e2e-repeat` 在 runner 上做；PERF 闸门在作者机器上判。
- **下载缓存的键由几个作业共用**：只用 git-lfs 的作业会先存下一份不完整的缓存，runtime-pack 会补下载，只是多花时间。以后按作业分键。
