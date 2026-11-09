# T237 评审

- 评审人：编排者
- 日期：2026-11-04
- 结论：**通过，合并**（PR #74）

## 核对

- **规模**：+500 −82。任务书的停止线是 300；审查之后新增了钩子和 worktree 预检两块，我先放宽到 350/400，后来又把硬停止线放到 500；最终正好在 500。
- **CI**：PR #74 https://github.com/boris-w/cupper/pull/74 ，ad6e101a 上 8 个检查全绿（run 41788238829）。rust 作业先下载 git-lfs 再跑测试，LFS 用例是真跑的。
- **连跑**：
  - `it_git_lfs_repo_config` 10 次 × 4 份，40 次全过（run 41788316700），每次约 12 秒，是真跑；
  - T235 的 `it_git_lfs_stage_objects` 也重跑了，40 次全过（run 41788320393）。
- **协议**：只附加了 `ErrorReason::lfs_object_missing`。
- **合并**：和 main（T125、T236）在 `git/cmd.rs`、`mod.rs`、`service.rs`、`worktree.rs` 有重叠，git 自动合并。推送前在合并提交上跑过 fmt、clippy、宿主 lib 测试 517 个，以及 crash_concurrency 的 git 用例，全过。
- **个人信息**：改动的文件过了敏感词清单，干净；假端点只绑 127.0.0.1，没有外发。

## 逐项意见

1. **审查做得透，结论有对照**：
   - 每一项都先用用户自己的 git 证明夹具确实会执行程序、会去请求端点，再证明宿主不会；
   - 修复前的五条失败都复现出来了，其中有真的联网、执行了 16 次 transfer agent，也有悄悄把指针文本写进工作区。
2. **钩子目录是这次审查最重要的发现**：git-lfs 往 cupper 的空钩子目录装钩子，T124 合并后，仓库档的 `worktree add` 就会执行 post-checkout。改成只读加启动时清空，已有数据目录里已经被写进去的钩子也能清掉。
3. **"宿主从不 smudge 本地没有的对象"这条原则对 revert 和 worktree add 一起落实**，没有拆开、留下联网的口子。端点改写挡不住 insteadOf 和代理，所以选预检，不改写端点，这个取舍是对的。
4. **顺带发现**：读配置时混进了宿主自己的 `-c` 覆盖，导致预检被跳过。已修，并有断言守着。

## 记下的点

- **设计已在合并时改**：
  - 5.2.10：空钩子目录、离线覆盖、程序类键、宿主不 smudge 缺失对象、错误映射、滤掉 command line 来源；
  - 5.3：`lfs_object_missing`，`git/revert` 的 errors；
  - 5.8.5：git 一行。
- **交接事项**（已写入 handoff-notes）：
  - 界面按 `lfs_object_missing` 提示"在终端里运行 `git lfs fetch`"，归 T144、T238；
  - alternates、超大仓库的 64 MB 上限、`.lfsconfig` 让 clean 失败（只会让快照失效）、退出码 128 的 revert，四条留给以后改 LFS 的任务。
