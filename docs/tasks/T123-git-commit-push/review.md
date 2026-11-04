# T123 评审

- 评审人：编排者
- 日期：2026-11-04
- 结论：**通过，合并**（PR #63）

## 核对

- **规模**：+557 −52。开工前估 430；第一次提交 +525，越过停止线 500，执行者停下报告，我接受，硬停止线仍是 600。之后按我的两条意见修改，到了 +557。
- **CI**：PR #63 https://github.com/boris-w/cupper/pull/63 ，d58de3e 上 8 个检查全绿（run 41763342554）。
- **连跑**：IT-git-commit-push 在 runner 上 10 次（run 41765718139）。
- **协议**：只做附加，`schema-compat` 0 breaking：`git/status` 的 `repo_config_changed`、`ErrorReason::signing_failed`，以及两条 x-errors。
- **个人信息**：改动的文件过了敏感词清单，干净；用例只用本地裸仓库和脚本扮演的 ssh、gpg，没有外发。

## 逐项意见

1. **开工前定下的五项都照做了**：
   - 确认参数用方案 B，宿主不强制；
   - 加了 `signing_failed`；
   - 新增 `git_commit` 超时，5 分钟；
   - 提交拿锁，推送不拿；
   - 用户档的环境照 5.2.10。
   其中确认参数在 actor 内存里维护，加载时从日志恢复。比估算多了约 50 行，但每次 `git/status` 都不必查库，这个取舍是对的。
2. **第一版的两处问题已改**：
   - `core.quotepath=false` 原先经 `GIT_CONFIG_PARAMETERS` 带进了用户的 hooks，现在只留在仓库档和快照档；
   - 推送的 remote 原先按第一个 `/` 切分，现在改读 `%(upstream:remotename)`，并补了 remote 名为 `my/fork` 的用例。
3. **超时的语义**与崩溃重试一致，回 `outcome_unknown`。没有比对 HEAD 的理由站得住：hook 也可能移动 HEAD，推送又没有本地可比的东西。
4. **失败分类**靠 `LC_ALL=C` 下的英文措辞和退出码，规则有表驱动的单元用例守着。启发式判断难免有边角，例如输出里恰好出现 `ssh-keygen` 的 hook。判错也只是 reason 不准，不影响正确性。

## 记下的点

- **设计已在合并时改**：
  - 5.3.4：`git/status` 的字段、`signing_failed`、两条 x-errors；
  - 5.2.10："提交与推送"的落实细节、`GitCmd` 的 `quotepath` 只给前两档、`commit` 超时、推送不拿锁。
- **交给 T233**（已写入 handoff-notes）：
  - `signing_failed` 给 pinentry-mac 的指引；
  - `auth_failed` 提示检查 ssh agent 或凭据；
  - 没有上游时直接推送，回的是不带 reason 的 `git_error`，界面在没有上游时默认带 `set_upstream`。
- **Q23 的手动验证说明**在本目录的 notes 里，作者可选。
