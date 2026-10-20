# T092 工作笔记

- 执行者：子 agent（测试工程师；T090、T167、T091 的执行者接着做）

## 开工前读过的

- 任务书；计划 T092 一行；设计 5.9.4 安全一节；`handoff-notes.md` 第一列含 T092、e2e 的行。
- 请求面板的待命逻辑（`RequestPanel.tsx`、`logic.ts`）；输入守卫的规则（`main/input/guard.ts`）。
- 假 agent 的权限选项：取自人设数据，是固定的一组。里面有特权选项（`allow-with-updates` 的 `requires: privileged`），没有"效果未知"的选项。
- 宿主的握手：`initialize` 的 `auth` 有 `peer` 和 `dev_key`，后者带证明。

## 开工前的规模估计

行数指格式化后的行，含注释。用例按 1.1 倍放大，其余代码按 1.15 倍。

| # | 编号或事项 | 内容 | 草估 |
|---|---|---|---|
| 1 | E2E-SEC-csp | 新会话页、会话页、请求面板、弹出菜单等界面上，`securitypolicyviolation` 计数为 0；注入的内联脚本不执行；创建名为 `default` 的 Trusted Types 策略失败 | 45 |
| 2 | E2E-SEC-real-input（`@focus`） | Playwright 自己的按键不算真实输入，宿主拒绝；窗口获得焦点后冷却期内的 Enter 被拒，出现"请再按一次"，之后成功；`page.click` 算真实输入 | 70 |
| 3 | E2E-SEC-panel-arming（`@focus`） | 八种情况：打字中或有草稿时不自动获得焦点；自动获得焦点后按字母键撤销，接着的 Enter 不提交；第一下 ⌘Enter 不提交；"请再按一次"只在冷却期出现；待命期内的按键和点击被吞掉；面板出现前按下、出现后松开的点击不算；连续两个请求，第二个重新待命；弹出菜单遮住时不响应 | 140 |
| 4 | E2E-SEC-panel-numbering（`@focus`） | 编号等于 `options` 的序号；按效果未知选项的编号无效；按特权选项的编号弹出原生确认框；伪造不相符的 `digit` 被宿主拒绝。<br>假 agent 要加一个能自定义选项的权限剧本（约 25 行），才有效果未知的选项。<br>伪造 `digit` 要从窗口发一条 `interaction/respond`：建议在 e2e 分支里给 `__cupperTest` 加 `command(method, params)` | 90 |
| 5 | E2E-SEC-privileged | 渲染进程经窗口的端口发特权方法（如 `host/exit`）被主进程拒绝；特权操作都经原生确认框，框里是宿主核实过的内容（提权、信任） | 55 |
| 6 | E2E-SEC-double-initialize | 窗口端口上发第二次 `initialize`，主进程回 `forbidden/already_initialized` | 25 |
| 7 | 原始协议客户端（Node） | 分帧编解码，按不同 `auth` 发 `initialize`；用例 8、9 共用 | 50 |
| 8 | 假宿主脚本（Node） | 先占住套接字，回一个证明不对的 `initialize` 结果，并记录对方是否发来了令牌或证明 | 50 |
| 9 | E2E-SEC-fake-host | 假宿主占着套接字时启动 App：窗口连接失败（`host_unverified`），弹出重启确认（替换后的确认框记录到），令牌没有发出 | 45 |
| 10 | E2E-SEC-cli-fake-host | 同样的假宿主，CLI 以退出码 4 拒绝 | 20 |
| 11 | E2E-SEC-no-key-client | 原始客户端：<br>• 不带密钥（`peer`）最多得到只读的 `cli`，写操作被拒；<br>• 声称 `window` 角色也一样；<br>• `dev_key` 证明错误或缺失时，得到 `unauthenticated` 并被断开 | 60 |
| 12 | E2E-SEC-egress（`nightly`）与 Q27 | 一个会话跑完的过程中多次采样整棵进程树（App、宿主、适配器）；断言没有非回环的流；记录 nettop 是否稳定采到每个进程、采样耗时，作为 Q27 的实测数据。不可行时换方法 | 70 |
| 13 | `exit_code()` 单元测试 | `elevate` 的 10–13，以及 1–4 | 45 |
| 14 | 上传缓冲的空条目 | 评估；需要改的话，给网关加"每连接未决上传数"的上限，含 Rust 用例 | 70 |
| 15 | 夹具注释 | `processes()` 的注释改正 | 1 |
| | 合计 | | **约 836**，放大后约 **920** |

推算约 920 行，超过约 850 的线（离硬线 950 也只差 30），按任务书开工前停下。

## 拆分建议

**方案一（建议）：按"窗口内"与"连接层"拆两份**

| 拆分 | 内容（上表编号） | 估计 |
|---|---|---|
| T092（收窄）窗口内的安全 | 1–6、13、15：CSP、真实输入、面板待命、面板编号、特权操作、二次 `initialize`；`exit_code()` 单元测试；夹具注释 | 约 520 |
| T092b 连接层与外发 | 7–12、14：原始协议客户端、假宿主、E2E-SEC-fake-host、E2E-SEC-cli-fake-host、E2E-SEC-no-key-client、E2E-SEC-egress（Q27）、上传缓冲 | 约 400 |

- 两份没有共用的新代码。第二份的原始客户端和假宿主只有它自己用。
- Q27 的结论在第二份；如果要更早拿到，可以先做第二份。

**方案二**：不拆，上限提到约 950，硬线约 1,050。

## 请一并确认

1. **假 agent 加一个权限剧本**：用剧本给出选项，这样才有效果未知的选项。这是测试替身里的代码，不是用例文件。
2. **`__cupperTest.command(method, params)`**：只在 e2e 分支里有，用来在窗口里伪造不相符的 `digit`。`flavors.test.ts` 照样会检查它不进非 e2e 构建。备选做法是经 React fiber 取 client，不建议。
3. **CSP 的口径**：设计写的是"注入的脚本不执行、`default` 策略失败"，任务书写的是"违规为 0"。两样都做。

## 裁定之后（方案一：窗口内的安全；上限约 600，硬线 680）

### 实际规模

按 `git diff --numstat main...HEAD`（不含 `pnpm-lock.yaml` 和文档）：新增 501 行，删除 35 行。

| 模块 | 新增 |
|---|---|
| `security.spec.ts`：六个编号的 7 条用例 | 298 |
| `security.ts`：`asWindow`、`resolved`、`permission`、`refocus`、`focusOptions`、`digest` | 53 |
| `exit_code()` 单元测试（UNIT-cli-exit-codes） | 60 |
| 假 agent 的 `permission_options` 剧本（含单元测试 12 行） | 32 |
| `__cupperTest.command`（e2e 分支） | 15 |
| 夹具：关掉被遮挡窗口的降频；注释改正 | 14 |
| 等待放宽（配置、CLI 包装） | 8 |
| `ci pr` 的 `@focus` 分段 | 7 |
| `onScreen` 挪到 `nfr1.ts`；`ids.toml`；类型声明 | 13 |

### 用例

| 编号 | 用例 | 标签 |
|---|---|---|
| E2E-SEC-csp | 从文档第一行脚本起，在会话页、请求面板、弹出菜单上：控制台和页面自己的 `securitypolicyviolation` 都没有违规；注入的内联脚本不执行；创建名为 `default` 的 Trusted Types 策略失败。这两项都会被上报，证明前面的计数确实能看到违规 | `@smoke` |
| E2E-SEC-real-input | Playwright 自己的按键能到页面，但不算真实输入，被拒，面板提示"按 Tab 或点击"；窗口刚获得焦点时 Enter 被拒，提示"请再按一次"；1 s 后同一个 Enter 通过 | `@focus` `@smoke` |
| E2E-SEC-panel-arming | 有草稿、或刚打过字（又删掉了），面板都不自动获得焦点，数字键无效；激活后第一下 ⌘Enter 只提示"按 Enter 提交"，第二下才提交；输入框空着且 2 s 没打字时面板自动获得焦点，按字母键撤销，紧接着的 Enter 不提交 | `@focus` `@smoke` |
| E2E-SEC-panel-arming | 待命期内的按键和点击被吞掉；在输入框上按下、到选项上才松开的点击不算；弹出菜单遮住时按键不作用于面板；连续两个请求，第二个面板出现时没有待命完（用 MutationObserver 记下每个面板出现时有没有 `data-ready`） | `@focus` `@smoke` |
| E2E-SEC-panel-numbering | 四个选项（普通、效果未知、特权、拒绝）依次编号 1–4，并带"效果未知""需要确认"的说明；按 2 无效；按 3 弹出原生确认框（取消后没有回答）；焦点离开面板后，真实按 4 再经窗口发"选项 1"的回答，被宿主以 `user_activation_required` 拒绝；真实按 1 再发同样的回答则通过 | `@focus` `@smoke` |
| E2E-SEC-privileged | 窗口发 `host/exit`、`auth/elevation_respond` 被主进程以 `forbidden` 拒绝；CLI 申请提权，横幅打开的原生确认框写着 CLI 的可执行文件路径（只有宿主核实得到）；在框里拒绝，CLI 以退出码 10 结束 | `@focus` `@smoke` |
| E2E-SEC-double-initialize | 窗口连接上的第二次 `initialize` 在主进程被拦下：`forbidden/already_initialized` | `@smoke` |

- 自定义选项的剧本：`permission_options <ACP 选项的 JSON>`。选项的 `kind` 写成没人认识的 `allow_maybe` 时，宿主按 5.3.6 归为效果未知、需要特权。
- 伪造 `digit`：主进程把守卫记下的输入证据写进窗口发出的命令。所以先让焦点离开面板（面板就不会自己回答），再用真实按键留下证据，接着经 `__cupperTest.command` 发回答。
- 面板的激活用"把焦点移到选项列表上"来做：DOM 焦点不是输入，不会产生证据。用 Tab 激活的话焦点可能落到"告诉 agent"那个输入框里，Enter 就交给了它。

### 取舍与发现

- **被遮挡的窗口会降频**：
  - 现象：并行跑时，被另一个 App 窗口（或本机别的窗口）盖住的窗口，Chromium 会暂停它的帧、压低它的计时器。面板 500 ms 的待命和视图的 rAF 刷新因此可能 10 s 都等不到，T091 的用例也偶发失败。
  - 处理：夹具启动 App 时带 `--disable-backgrounding-occluded-windows`、`--disable-renderer-backgrounding`、`--disable-background-timer-throttling`。只改了启动参数，生产代码没改。
- **等待放宽**：期望的等待从 10 s 放宽到 20 s，用例上限 90 s，CLI 包装的默认等待 20 s。机器负载高（平均负载 8–17）时，flood 跑完、回合结束都会变慢；通过的用例不会因此多等。
- **`ci pr` 的 `@focus` 分段**：原来 `ci pr` 用两个 worker 一段跑完 `@smoke`，要焦点的用例会被另一个 worker 的 App 抢走焦点。现在和 `ci e2e` 一样分两段，同时带 `@smoke` 和 `@focus` 的用例最后单 worker 跑。这样标为 `pr` 的六个编号在每个 PR 上都会跑。
- **别的 worktree 的 e2e 会互相干扰**：开发中我直接跑过 playwright，没有持机器锁，可能影响了 T093 的连跑。编排者提醒后，10 次连跑全部改用 `cargo xtask ci e2e`（持锁），并在每次跑完时检查进程表里有没有别的 worktree 的 e2e App。

### 验证

- 10 次 `cargo xtask ci e2e` 连跑（持机器锁，两段）全绿：每次 35 项（3 项检查、20 条并行、12 条 `@focus`），128–226 s。每次跑完：本 worktree 没有残留进程，`$TMPDIR/cupper-e2e/` 为空，plist 不在，进程表里也没有别的 worktree 的 e2e App。
- 加 `ci pr` 的 `@focus` 分段、给五条用例补 `@smoke` 标签，是在这 10 次之后。它们不改变用例本身，`ci pr --record` 会把新的 PR 两段跑一遍。
