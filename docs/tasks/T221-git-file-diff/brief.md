# T221 Git 服务 IIIb：DiffCache、file_diff 与 hunk 切分

- 派发日期：2026-11-03
- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`feat/T221-git-file-diff`
- 依赖：T120（`git/diff` 的四种范围）。T121（暂存、取消暂存与还原）依赖本任务（M1b 关键链）

## 背景

T120 开工后越过硬停止线，拆成三份（计划 v1.3.86，卷宗 `docs/tasks/T120-git-diff-status/report-r1.md`）。T221 是第二份：审查面板按文件看 diff，T121 按 hunk 暂存和还原，都要 `git/file_diff` 给出的操作单位。代码在 T120 的提交 38f85e0、0e1c0f0 里已经写过一版（`git/diff/hunks.rs`、`git/diff/cache.rs` 等），T120 合并前移出了，取回清单在 T120 的 `notes.md`，本任务从那里取回、补齐用例。`DiffCache` 也在这里：只有 `git/file_diff` 读它，T120 里只写不读，按"没人用的代码不进 main"一起移了过来；T120 的 `git/diff` 已经返回 `IdGen` 生成的 `diff_token`，本任务把它接上缓存。

## 范围

**做**（计划 T221 一行，5.2.10 的"diff 与 hunk"，5.3.4）：
1. `DiffCache`：四种范围共用 `diff_token`，每个工作区最近 32 个，`diff_cache_ttl` 10 分钟，存 tree 对和文件列表，不存补丁；`git/diff` 把结果放进去。过期或不认识的 token 返回 `stale_diff`。
2. `git/file_diff`：三种操作单位 `hunk`、`mode`、`whole_file`，覆盖新增、删除、二进制、符号链接、子模块、LFS 指针。
3. `hunk_id` 按内容寻址；单个文件的补丁按 `DiffCache` 里的 tree 对重新生成（T120 的裁定：缓存不存补丁）；补丁用 `--no-renames`，重命名是旧路径的删除加新路径的新增。
4. 大 hunk 转 blob（超过单帧时）。
5. 用例 **IT-git-file-diff**（已登记，挂在 T221 下）：三种单位、上面列的文件种类、重命名、大 hunk 转 blob、缓存命中与 `stale_diff`、其余错误映射。
6. 协议照 ADR-04；契约已冻结，只能附加，`schema-compat` 0 breaking。

**不做**：写操作（T121）；`git/status`、`git/branches`、`cupper diff`（T222）；界面。

## 验收标准

- [ ] IT-git-file-diff 通过。
- [ ] `schema-compat` 0 breaking。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：450；硬停止线：550

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.2.10（diff 与 hunk；T120 合并时按 report-r1 的取舍改写）、5.3.4、ADR-10、ADR-04。
- 计划：T221 一行，第 3 节 T120 的注。
- 卷宗：T120（report-r1.md、report.md、review.md）、SP-5（hunk 用例的原型实测）、T068、T069。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）；不在用户机器上跑性能或高负载测试。
- git 子进程一律经 `cupper-sys::spawn`；时间经 `Clock`；有状态的决策逻辑写成纯核心。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
