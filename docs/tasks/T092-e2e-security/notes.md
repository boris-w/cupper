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
