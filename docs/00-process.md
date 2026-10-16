# 开发流程

```
① PRD → ② 技术设计 → ③ 任务拆分 → ④ 迭代开发 → ⑤ 验收发布 → ⑥ 复盘
```

| 阶段 | 产出 | 负责 | 闸门（用户确认） | tag |
|---|---|---|---|---|
| ① PRD | `docs/01-prd.md` | 编排者 + 用户 | MVP 范围冻结 | `gate/prd` |
| ② 技术设计 | `docs/02-design.md`、`docs/decisions/` | 编排者（调研可派子 agent） | 方案和关键选型 | `gate/design` |
| ③ 任务拆分 | `docs/03-plan.md` | 编排者 | 任务顺序和粒度 | `gate/plan` |
| ④ 迭代开发 | 代码 + 测试 | 子 agent（每个任务独立 worktree） | 每个任务通过 review 后合并 | — |
| ⑤ 验收发布 | 发布记录 | 编排者 | 逐条对照 PRD 验收标准 | `release/vX.Y.Z` |
| ⑥ 复盘 | `docs/retro.md` | 编排者 + 用户 | 下一版输入 | — |

## 任务生命周期（阶段 ④）

1. 编排者从 `docs/03-plan.md` 选出依赖已满足的任务，派给子 agent（互不依赖的任务可以并行）。
2. 子 agent 在 `<type>/<任务ID>-<slug>` 分支的独立 worktree 中开发、自测、小步提交。
3. 编排者 review diff，跑测试；不通过就打回给同一个子 agent 修改。
4. 通过后 `git merge --no-ff` 合入 `main`，在 `docs/03-plan.md` 中把任务标记为完成，然后清理 worktree。

## 当前进度

- [x] 初始化仓库和协作约定
- [ ] ① PRD：收集需求中
- [ ] ② 技术设计
- [ ] ③ 任务拆分
- [ ] ④ 迭代开发
- [ ] ⑤ 验收发布
- [ ] ⑥ 复盘
