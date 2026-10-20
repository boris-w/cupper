# T092 汇报

- 执行者：子 agent（测试工程师）
- 分支：`test/T092-e2e-security`（编排者已把 main 合入，合并提交 d410921）
- 完成日期：2026-10-20

## 做了什么

按方案一的裁定，T092 收窄为窗口内的安全。六个编号共 7 条用例全部通过；`exit_code()` 的单元测试和夹具注释也一并做了。

**用例**（`tools/e2e/src/security.spec.ts`，共用步骤在 `security.ts`）

| 编号 | 断言 | 标签 |
|---|---|---|
| E2E-SEC-csp | 从文档第一行脚本起，在会话页、请求面板、弹出菜单上，控制台和页面自己的 `securitypolicyviolation` 都没有违规；注入的内联脚本不执行；创建名为 `default` 的 Trusted Types 策略失败。这两项都会被上报，证明计数确实看得到违规。 | `@smoke` |
| E2E-SEC-real-input | Playwright 自己的按键能到页面，但不算真实输入，被拒；窗口刚获得焦点时 Enter 被拒，面板提示"请再按一次"；1 s 后同一个 Enter 通过。 | `@focus` `@smoke` |
| E2E-SEC-panel-arming（第一条） | 有草稿、或刚打过字时，面板不自动获得焦点，数字键无效；激活后第一下 ⌘Enter 只提示"按 Enter 提交"，第二下才提交；输入框空着且 2 s 没打字时面板自动获得焦点，按字母键撤销，紧接着的 Enter 不提交。 | `@focus` `@smoke` |
| E2E-SEC-panel-arming（第二条） | 待命期内的按键和点击被吞掉；在输入框上按下、到选项上才松开的点击不算；弹出菜单遮住时按键不作用于面板；连续两个请求，第二个面板出现时没有待命完（用 MutationObserver 记录）。 | `@focus` `@smoke` |
| E2E-SEC-panel-numbering | 普通、效果未知、特权、拒绝四个选项依次编号 1–4，并带相应说明；按 2 无效；按 3 弹出原生确认框；焦点离开面板后，真实按 4 再发"选项 1"的回答，被宿主以 `user_activation_required` 拒绝；真实按 1 再发则通过。 | `@focus` `@smoke` |
| E2E-SEC-privileged | 窗口发 `host/exit`、`auth/elevation_respond` 被主进程以 `forbidden` 拒绝；横幅打开的提权确认框写着 CLI 的可执行文件路径（只有宿主核实得到）；在框里拒绝后，CLI 以退出码 10 结束。 | `@focus` `@smoke` |
| E2E-SEC-double-initialize | 窗口连接上的第二次 `initialize` 在主进程被拦下：`forbidden/already_initialized`。 | `@smoke` |

**支撑代码**（编排者认可的两项都做了）
- **假 agent 的 `permission_options <ACP 选项 JSON>` 剧本**：带单元测试。`kind` 写成没人认识的值时，宿主按 5.3.6 把它当作效果未知。
- **`__cupperTest.command(method, params)`**：只在渲染进程入口的 e2e 分支里有。它经窗口的 client 和主进程的检查发出命令，返回结果或错误；`flavors.test.ts` 仍确认它不进非 e2e 构建。
- **UNIT-cli-exit-codes**：覆盖退出码 1–4 和 `elevate` 的 10–13。"宿主已退出"的几种情形（流结束、对端已退出、没有套接字、连接被拒）都映射到 3；对端 exec 了别的程序、权限被拒则是 1。编号已登记在 `ids.toml` 的 T092 段。
- **夹具注释改正**：宿主由 App 主进程拉起，但会在 App 退出后留下来。

## 关键取舍

- **规模**：新增 501 行、删除 31 行（不含 lock 和文档），上限约 600。
- **伪造 `digit` 的做法**：先让焦点离开面板，这样面板不会自己回答；再用真实按键留下输入证据；接着经 `__cupperTest.command` 发出选项序号不符的回答。主进程把证据写进命令，宿主据此拒绝。
- **面板激活用"把焦点移到选项列表上"**：DOM 焦点不产生输入证据。用 Tab 激活时，焦点可能落进"告诉 agent"那个输入框，Enter 就交给它处理了。
- **被遮挡窗口的降频**：并行时被别的窗口盖住的 App，Chromium 会暂停它的帧、压低计时器，面板 500 ms 的待命和视图的刷新会等很久。夹具启动时加了 `--disable-backgrounding-occluded-windows`、`--disable-renderer-backgrounding`、`--disable-background-timer-throttling`，只改启动参数，没改生产代码。
- **等待放宽**：期望的等待从 10 s 放宽到 20 s，用例上限 90 s，CLI 包装的默认等待 20 s。本机负载高（平均负载 8–17）时，回合结束会变慢；通过的用例不会因此多等。
- **`ci pr` 的 `@focus` 分段**：
  - 原来 `ci pr` 在两个 worker 上一段跑完 `@smoke`，要焦点的用例会被另一个 worker 的 App 抢走焦点；
  - 现在和 `ci e2e` 一样分两段，同时带 `@smoke` 和 `@focus` 的用例最后单 worker 跑；
  - 这样标为 `pr` 的六个编号在每个 PR 上都会跑，不需要改成 `nightly`；
  - 本次 PR 的 e2e 作业共 112 s，其中构建 38 s、并行段 21 s、焦点段 28 s。

## 验证方式

- **六个编号**：全部通过。靠真实输入的五条打了 `@focus`。
- **10 次连跑**：经 `cargo xtask ci e2e`，持机器锁、两段跑法，全绿。每次 35 项（3 项检查、20 条并行、12 条 `@focus`），128–226 s。每次跑完都检查过：本 worktree 没有残留进程，`$TMPDIR/cupper-e2e/` 里没有本用例的根目录，plist 不在，进程表里没有别的 worktree 的 e2e App。
- **10 次之后的改动**：`ci pr` 的分段和五条用例补的 `@smoke` 标签是在 10 次连跑之后加的，不改变用例本身；下面的 `ci pr` 把新的两段跑了一遍。
- **`cargo xtask ci pr --record docs/tasks/T092-e2e-security/ci/`**：在干净的 worktree 里（合并 main 之后）通过，运行期间没有写工作区。摘要 `pr-d410921a3c8f.json` 已提交，七个编号均为 pass。

  | 作业 | 结果 |
  |---|---|
  | rust | 1,967 通过 |
  | protocol | 185 通过 |
  | ts | 471 通过 |
  | license | 1 通过 |
  | testdata-scan | 3 通过 |
  | ui | 本次跳过（截图依赖的路径未改） |
  | e2e | 20 通过 |

## 提交列表

```
ce0d4a9 docs(task): T092 CI summary
d410921 Merge branch 'main' into test/T092-e2e-security
97de1d6 docs(task): T092 notes: sizes, cases, findings, runs
129f102 ci: the PR's smoke cases that need the focus run last, one at a time
57f7a20 test(e2e): keep covered windows running, and wait longer under load
0d3d36b test(e2e): E2E-SEC-panel-arming, panel-numbering, privileged, double-initialize
a01fc06 test(e2e): E2E-SEC-csp and E2E-SEC-real-input
b3bb73e test(e2e): correct the fixture's note on where the host comes from
c593b4d feat(desktop): send a command as the window in e2e builds
9838a18 test(fake-agent): a permission with the options the script gives
adb4815 test(cli): exit codes by failure
eca1a0e docs(task): T092 size estimate and split proposal
```

## 遗留问题 / 范围外发现

1. **设计 5.9.2 的剧本命令表要补一项**：新增的 `permission_options` 需要编排者加进去。
2. **e2e 期间请留意别的 worktree**：`$TMPDIR/cupper-e2e/` 是各 worktree 共用的。T004 正在跑的用例在那里留有一个根目录，属主是它自己的 worker，不是本任务的残留。机器锁合并之前，各 worktree 直接跑 playwright 仍会互相抢焦点、遮挡窗口。
3. **被遮挡窗口的降频**：真实用户的窗口被盖住时，视图刷新和面板待命也会推迟，等窗口露出来才继续。这在产品上可以接受，所以只在 e2e 的启动参数里关掉了降频，没有改产品。
4. **这一轮没有发现新的产品缺陷，没有就地修复**。
