# T097 Git 服务 I-b：仓库档的规则与指纹

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师）。T043、T088 的执行者接手
- 分支：`feat/T097-git-repo-rules`
- 依赖：T022（已合并）。与 T060 并行；T098（仓库档的执行）依赖本任务和 T060。**本任务在剩余关键路径上，没有松弛**

## 背景

宿主运行在所有 agent 沙箱之外，而仓库里的配置可能被 agent 写过，所以宿主自己发起的 git 调用不执行仓库配置指定的任何程序（设计 5.2.10，安全 #1，纪要 D8）。T060 开工前估计约 1,335 行，拆成三份（计划 v1.3.16，见 `docs/tasks/T060-git-cmd/brief.md` 末尾的补充裁定）：
- T060：`GitCmd` 与三档类型（拉起、环境、命令表）；
- **本任务**：仓库档的规则与指纹，全是纯函数，不拉起任何进程；
- T098：把两者接起来执行。

T060 的执行者在它自己的分支上写了方案草图和模块划分，请用 `git show feat/T060-git-cmd:docs/tasks/T060-git-cmd/notes.md` 只读地参考，模块文件名与它错开。

## 范围

**做**（设计 5.2.10"仓库档：每次调用前读配置"第 1–5 条的规则部分、"仓库可执行配置指纹"；2026-10-20 按 T060 的实测补充的几处）：
1. **配置输出的解析**：`git config --list --show-origin --includes -z` 的输出 → 每个键的取值和来源文件（含 `command line:`、`blob:` 等非文件来源的处理）。输入是字节，本模块不运行 git。
2. **来源检查**：任何来源位于工作区内或其他 agent 可写的位置（调用方传入的项目、工作区根目录，比较前取 realpath）时拒绝，返回 `unsafe_repo_config` 及其原因。仓库自己的 git 目录（`.git/config`、`config.worktree`）除外；`include.path` 指向 git 目录之外的工作区文件时照样拒绝；`core.hooksPath` 指向工作区不受此限（总被覆盖）。
3. **覆盖参数的计算**：
   - 无条件覆盖：`core.fsmonitor=false`、`core.hooksPath=<数据目录>/git/empty-hooks`；生成补丁的命令加 `--no-ext-diff --no-textconv`（由调用方说明命令种类）；
   - 可执行类键（5.2.10 第 4 条的清单，含 2026-10-20 补的 `diff.external`、`core.alternateRefsCommand`、`gpg.ssh.defaultKeyCommand`、`submodule.*.update`、`pager.*`）：取值不在白名单里的用 `-c <键>=` 置空；白名单只有 git-lfs 的标准取值，程序改写成调用方给出的 git-lfs 绝对路径；
   - **子段里带 `=` 的可执行类键置空不掉，直接拒绝**（`unsafe_repo_config`）；
   - 记下哪些过滤器设了 `required=true` 并被置空，供 T098 判断"覆盖后失败"的原因（只有这种情况判为 `unsafe_repo_config`）。
4. **仓库布局与可执行配置指纹**（UNIT-git-fingerprint）：仓库的 `config` 及其 include 的文件、worktree 的 `config.worktree`、`hooks/` 和 `core.hooksPath` 指向的目录下的文件（名称和哈希）、`info/attributes`、工作区里的全部 `.gitattributes`。提供"前后比较并给出变化路径"的函数。布局（git 目录、common dir、worktree 的 git 目录）由调用方传入或从文件系统读出（`.git` 是文件时按 `gitdir:` 解析），本模块不运行 git。

**不做**：
- 拉起 git、环境、命令表（T060）；
- 执行与 notice 信号（T098）。

## 验收标准

- [ ] 新增 UNIT-git-repo-rules：配置输出的解析（含 `-z` 下值里有换行、`=`、非 UTF-8 字节）；来源检查的每种情形（工作区内、其他项目内、git 目录例外、include 到工作区、`core.hooksPath` 不受限、realpath 逃逸）；覆盖参数（每一类可执行键、git-lfs 标准取值改写、非标准取值置空、子段带 `=` 的键被拒绝、`required=true` 的记录）。
- [ ] UNIT-git-fingerprint：指纹对每一类内容都敏感，对无关文件不敏感；变化时给出路径；worktree 的布局（`.git` 是文件）也覆盖到。
- [ ] 全部是纯函数和只读的文件访问；用例只在临时目录里造仓库布局（可以不用真的 git），不读本机真实的仓库和全局配置。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T097 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T097-git-repo-rules/ci/` 通过，摘要随任务提交。

## 注意

- 代码约束：`cupper-host` 不用 `unsafe`；错误用 thiserror；时间只经 `Clock`（如果需要）。导出给其他语言的产物不依赖 serde_json 的 map 键顺序。
- 模块放在 `cupper-host` 的 `git/` 下，文件名与 T060 的草图错开；`git/mod.rs` 的模块声明合并时可能有一处相邻行冲突，正常。
- 用例和 notes 里不出现本机的用户名和路径。

## 规模

计划估为 M，T060 的执行者估这一份约 465 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 550 行（包括贴线）时，必须先停下来汇报**；硬停止线 650。

## 参考资料

- 设计：`02-host.md` 5.2.10（含 2026-10-20 补充的几处）；`08-security.md` 5.8.5；`13-open-issues.md` Q32。
- 安全 #1、纪要 D8、ADR-10。
- `docs/tasks/T060-git-cmd/brief.md`（补充裁定）；`docs/tasks/SP-5-git-snapshot/`。

## 工作方式

在原来的 T043 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T097-git-repo-rules main`。T088 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
