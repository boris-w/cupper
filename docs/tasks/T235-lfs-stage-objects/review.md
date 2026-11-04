# T235 评审

- 评审人：编排者
- 日期：2026-11-04
- 结论：**通过，合并**（PR #67）

## 核对

- **规模**：+294 −8。停止线 250 原本就写明"查清后可以调整"，我又加了 `core.sharedRepository` 的处理（约 15 行），所以接受，没有越过硬停止线 300。
- **CI**：PR #67 https://github.com/boris-w/cupper/pull/67 ，9f2cfb71 上 8 个检查全绿（run 41771240738）。rust 作业先下载 git-lfs 再跑测试。
- **连跑**：
  - 第一次（run 41771238835）作废：test-repeat 没有下载 git-lfs，用例被跳过，跳过也算通过。执行者从耗时看了出来。
  - 我给 test-repeat 加了下载 git-lfs 的一步（PR #69），在临时分支 `test/T235-repeat` 上重跑（run 41772123987）。40 次全过，约 51 秒，确实是真跑。
- **合并**：和 T124 在 `git/service.rs` 有冲突：T124 把锁改成了 `self.deps.locks`，T235 加了 `objects()` 调用，两边都保留，我手工合并。推送前在合并提交上跑过 fmt、clippy 和宿主 lib 测试。
- **个人信息**：改动的文件过了敏感词清单，干净。

## 逐项意见

1. **复现扎实**：
   - 先用 `git lfs fsck` 证明提交里的指针缺对象；
   - 再证明还原时宿主确实去下载了（stderr 里的 `Downloading a.bin`），比 T229 评审时我担心的更严重。
2. **修法选得对**：复制加 oid 校验，不运行任何程序，大文件流式处理。存储位置的规则用 `git lfs env` 实测核对过，权限照 git 的 `core.sharedRepository` 规则。
3. **失败安全**：复制或校验失败时回 `git_error`，不执行 apply，账本照常 reject。
4. **范围守住了**：`lfs.extension.*`、smudge 联网、smudge 失败被报成 `stale_diff` 这三件事都留给 T237，没有顺手改。

## 记下的点

- **设计已在合并时改**：
  - 5.2.10：仓库档写操作前放 LFS 对象的规则；
  - 5.8.5：宿主不经 git、直接写用户仓库 `.git/lfs` 的这条路径。
- **用例缺口**交给 T237：`lfs.storage` 的相对、绝对路径和 `core.sharedRepository` 的其余取值目前没有单元用例，T237 在同一模块里一并补上（UNIT-git-lfs-store）。
- 临时分支 `test/T235-repeat` 在合并后删掉。
