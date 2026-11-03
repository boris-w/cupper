# T156 评审

- 评审人：编排者
- 日期：2026-11-03
- 结论：**通过，合并**（PR #43）

## 核对

- **规模**：+436 −16。执行中推算会越过停止线 400，执行者停下请示，我同意不拆、放宽到 470（硬停止线 500 不变）。
- **CI**：PR #43 https://github.com/boris-w/cupper/pull/43 ，af423cb 上 8 个检查全绿（run 41586118768）；改了 `crates/cupper-sys`，`cupper-sys.yml` 的 security-peer (macos-26) 也通过（run 41586118784）。这是 T211 把 cupper-sys.yml 移到 macos-26 之后它第一次真正跑（T211 的交接事项）。runner 上用 `pmset -g assertions` 按进程号核对断言可行。
- **个人信息**：改动的文件过了敏感词清单，干净。断言名只收编译期常量，用户数据进不了 `pmset`。

## 逐项意见

1. **结构对**：`PowerCore` 是纯核心；每个会话一份 `Share`，在 `input()` 末尾报"回合 + 待处理请求"，drop 时归还，不轮询；断言由唯一的监护 actor 持有，符合 5.2.2 的"一个 actor"。
2. **退出顺序**：`Host::exit` 在写线程停下之后、删套接字之前释放，退出后核心不再取得，退出中会话的变化不会把断言拿回来。
3. **用例**：IT 断言 `power_assertion` 的完整历史 `[0,1,0,1,0]`，中途误释放会被抓到；SIGKILL 后系统里不残留，重启后为 false。"只有请求、没有回合"在宿主里到不了（协议 5.3 的裁定），由纯核心单测覆盖，我同意。
4. **`unsafe`**：只在 cupper-sys，两处都有 `SAFETY:` 注释；`Drop` 里释放失败无事可做，进程结束时系统会回收。

## 记下的点

- **合并**：main 上已有 T111、T120，`supervisor/mod.rs` 和会话测试由 git 自动合并；合并后在 worktree 上跑了 clippy。
- **设计**（本次合并时改）：5.2.13"防休眠"补实现要点（计数来源、由监护 actor 持有、退出释放、`host.updated`），以及设置页开关还没有接。
- **交接事项**（已写入 handoff-notes）：T147 做设置页"通用"里的防休眠开关时，按 T156 汇报的建议接（`host.prevent_idle_sleep`，`PowerCore` 加 `Enabled(bool)`，约 15 行）。
