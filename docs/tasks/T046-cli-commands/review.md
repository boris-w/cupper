# T046 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：新增 1,203 行、删除 12 行（不含 lock 和文档），在放宽后约 1,450 的上限之内；开工前估 1,435。
- **CI**：执行者的摘要 `pr-78431c8f29a0.json` 通过。合并 main 之后在本 worktree 重跑 `cargo xtask ci pr --record`：第一次通过；之后 main 先后合进 T059 和 T029，两者都动了 `supervisor/mod.rs` 或 `tests/lifecycle.rs`，编排者又合了两次（`routes.streams` 去重；`lifecycle.rs` 两边都在同一位置追加了完整用例，两边都保留），最后一次 `pr-6f95c7deeb1a.json` 通过（rust 1854、protocol 185、ts 431、license、testdata-scan；ui 被路径过滤跳过）。
- **中途一次 API 错误**：执行者因 API 错误中断过一次，从未提交的 IT 处续上，没有丢工作。

## 逐项意见

1. **命令齐全**：设计 5.2.14 的命令都做了，`--harness-key-fd` 用 `#[cfg(feature = "test-harness")]` 门控，发布版没有。参数解析是纯函数，`repl` 复用，UNIT-cli-args 覆盖。
2. **退出码有约定**：0、1、2、3、4，以及 `elevate` 的 10–13，表写进了 notes。`Eof`、`Refused(PeerExited)`、套接字不存在、连接被拒都报"宿主已退出"（码 3），符合 T049 的交接。
3. **`repl`**：在同一条连接上执行，`elevate` 的授权对后面的写操作有效。`--json` 下每条命令以 `{"repl": {"exit": n}}` 结束，便于自动化分隔。设计好。
4. **IT-cli-commands**：
   - 覆盖读命令、未提权的写被拒、`repl` 里 `elevate` 后写、`approve`/`reject`/`answer`、`new`/`send --wait`/`interrupt`、另起 `watch` 看到回合事件；
   - 批准提权照 IT-elevate 经 harness 连接发 `auth/elevation_respond`，生产代码里没有绕过路径；
   - 用 `Reaped` 守卫保证子进程在用例结束时被回收，回应了 T008 修复 1 提出的残留问题；
   - 连跑 10 次全绿。
5. **范围外的修复**：`Host::start` 一直没有调用 `routes.streams(...)`，真实宿主上流方法都回 `unsupported`。执行者补了 2 行（`e662e86`）。T059 也补了同一行。两个分支谁后合并，谁去掉重复，由编排者合并时处理。

## 记下的问题

- **`elevate` 的非批准路径**（`granted: []`、`timeout`、`rate_limited`、`lineage`）只靠 `exit_code()` 的映射，没有用例；人读格式、`sessions --watch`、`answer` 的类型转换也没有测。给 `exit_code()` 补单元测试，其余交 T090、T092 的 e2e，记交接。
- **`--json` 下 `elevate` 被拒时输出两行**（原始回复加 `{"error": {"code": "denied"}}`），是有意的。T090、T091 写断言时要知道，记交接。
- **手动时钟下索引要先推进**（合并窗口 100 ms），`sessions` 才看得到新会话。T090、T091 会碰到，记交接。
