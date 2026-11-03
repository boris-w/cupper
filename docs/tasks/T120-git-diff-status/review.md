# T120 评审

- 评审人：编排者
- 日期：2026-11-03
- 结论：**通过，合并**（PR #42）

## 核对

- **规模**：开工后越过硬停止线，按执行者的建议拆成三份（计划 v1.3.86，`report-r1.md`）。收窄后 +860 −54，在放宽后的停止线 1000 以内。
- **CI**：PR #42 https://github.com/boris-w/cupper/pull/42 ，b0380c6 上 8 个检查全绿（run 41583305929）；0e1c0f0 那次（41582793610）被新推送取消。
- **个人信息**：改动的文件过了敏感词清单，干净。

## 逐项意见

1. **拆分处理得干净**：移出的代码在 38f85e0、0e1c0f0 里，`notes.md` 有取回清单，没有 rebase。T221、T222 都从那里取回。
2. **评审意见一条，执行者的处理比我说的更好**：我只要求移出没人调用的 `DiffCache::get`。执行者指出，缓存只写不读，`insert` 和淘汰同样没人用，于是把整个 `DiffCache` 移给了 T221。我接受，计划和 T221 的任务书已改。现在 `git/diff` 返回的 `diff_token` 不指向任何东西，T221 接上缓存后才有用，这期间也没有读 token 的方法。
3. **HEAD 的取法**：两次调用都选了在任何 HEAD 状态下都以 0 退出的命令（`branch --show-current`，再用 `for-each-ref` 或 `rev-parse --verify -q`）。原因是配置了 `required` 过滤器的仓库里，仓库档会把非零退出判成 `unsafe_repo_config`（T098 规则 5）。`for-each-ref` 的结果按完整 refname 过滤，避开了 `refs/heads/<branch>/…` 的误配。
4. **用例**：IT-git-diff-scopes 覆盖四种范围、第一次提交前、运行中的回合、重命名、二进制、过滤器、`snapshot_missing`、`not_found`、`index_unmerged`，还做了两处变异检查。

## 记下的点

- **设计**（本次合并时改）：5.2.10 仓库档的只读命令补上 `branch --show-current`；`DiffCache` 的写法（存 tree 对和文件列表、不存补丁）等 T221 合并时改。
- **交接事项**（已写入 handoff-notes）：
  - r1 的范围外发现：子目录项目没有 `.git`、仓库档拒绝时会话流里没有 notice、文件列表没有 blob 兜底、git-crypt 类过滤器在 `unstaged` 里误报"已变化"；
  - T124：`git/diff` 对回合范围一律加 `mixed_changes`，worktree 模式出现后要只在本地模式下加（GT-1）。
