# T228 Git 服务 IV-b：SP-5 的 82 个 hunk 用例

- 派发日期：2026-11-03
- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`test/T228-git-hunk-cases`
- 依赖：T121（暂存、取消暂存与还原）

## 背景

T121 开工前估算后拆分（计划 v1.3.86）：写操作本身已合并（PR #54），SP-5 原型实测时的 82 个 hunk 用例（原 T122，后并入 T121）拆到本任务，改写成 Rust 测试，即 IT-git-hunk（GT-3）。

## 范围

**做**：
1. 把 `spikes/SP-5-git-snapshot/` 里的 hunk 用例（`edge_hunk.py`、`hunks.py` 等）改写成 Rust 测试，走产品路径：`git/diff` → `git/file_diff` → `git/stage`、`git/unstage`、`git/revert`。spike 代码只作参考，产品代码不依赖它。
2. 共用设施：按编辑表建仓库、算期望内容、读 index 的 blob 和 mode 以及工作区、经 service 执行操作、按标记挑单位。
3. 产品里不适用的几行（T121 估算时列过）：libgit2 那两列；`-M` 补丁取消暂存 hunk（产品用 `--no-renames`）；"不带 `--binary` 暂存二进制"；"未跟踪文件在 git diff 里看不见"（产品的 unstaged 包含它们）。按产品的等价行为改写，对应关系写进 notes。
4. 补一条 T121 的交接：revert 拿 staged token 的组合，只靠范围核对拦下。
5. 用例：IT-git-hunk（已登记，挂在 T228 下）。

**不做**：Q32、LFS、故障点（T229）；改产品的写路径。用例查出产品缺陷时，先报给 `main`。

## 验收标准

- [ ] IT-git-hunk 通过，82 个用例与 SP-5 的对应关系写进 notes。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：550；硬停止线：650

## 参考资料

- 设计：5.2.10（hunk 操作、并发）、5.9.8（其他关键测试）。
- 卷宗：T121（report、review）、SP-5（spikes/SP-5-git-snapshot）、T120、T221。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- 测试只在临时目录的仓库上操作。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
