# T092 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：新增 501、删除 31（不含 lock 和文档），在收窄后的上限约 600 以内。开工前估约 920，执行者在写代码前停下，按方案一拆出 T176（计划 v1.3.44）。
- **CI**：编排者把 main 合进分支（`ids.toml` 一处冲突取 main 的版本，保留本任务加的 `UNIT-cli-exit-codes`），执行者在合并结果上跑 `ci pr --record` 通过：`pr-d410921a3c8f.json`（rust 1967、protocol 185、ts 471、license、testdata-scan、e2e 20；ui 按路径过滤跳过）。之后 main 进了 T093（机器锁），两边各自做过的"`ci pr` 的 `@focus` 分段"在 `crates/xtask/src/ci/jobs/e2e.rs` 与 `tools/e2e/playwright.config.ts` 冲突，编排者取 main（T093）的写法、注释里保留"同样分两段"一句，再在合并结果上跑 `ci pr --record` 通过：`pr-5f36d7cbf0ab.json`（rust 1993、protocol 185、ts 473、license、testdata-scan、e2e 26；ui 按路径过滤跳过）。
- **验收**：六个编号共 7 条用例全部通过，靠真实输入的五条打了 `@focus`；经 `cargo xtask ci e2e`（持锁、两段跑法）连跑 10 次全绿，期间没有别的 worktree 的 e2e App；`exit_code()` 单元测试和夹具注释也做了。

## 逐项意见

1. **E2E-SEC-csp** 两种口径都做：控制台与页面的 `securitypolicyviolation` 为 0，注入的内联脚本不执行、名为 `default` 的 Trusted Types 策略创建失败；这两项违规确实被上报，证明计数看得到违规，不是空断言。
2. **E2E-SEC-real-input**：Playwright 的按键能到页面但不算真实输入；窗口刚获得焦点时 Enter 被拒并提示"请再按一次"，1 s 后通过。
3. **面板待命与编号**：覆盖了草稿与打字、⌘Enter、自动获得焦点后的撤销、待命期吞键、跨元素的点击、弹出菜单遮挡、连续请求；编号 1–4 与"按效果未知的编号无效、按特权编号走原生确认、伪造 `digit` 被宿主以 `user_activation_required` 拒绝"。伪造的做法（焦点离开面板、留下真实按键证据、经 `__cupperTest.command` 发出序号不符的回答）只在 e2e 构建里可行。
4. **E2E-SEC-privileged**：窗口发 `host/exit`、`auth/elevation_respond` 在主进程被 `forbidden`；提权确认框写着只有宿主核实得到的 CLI 可执行文件路径；在框里拒绝后 CLI 以 10 结束。
5. **`__cupperTest.command`** 只在 e2e 分支，经窗口的 client 和主进程的检查发出，`flavors.test.ts` 照样守住。
6. **被遮挡窗口的降频**：只在 e2e 的启动参数里关掉，没改产品；真实用户的窗口被盖住时推迟刷新与待命，在产品上可以接受。

## 记下的点

- **`ci pr` 的 `@focus` 分段与 T093 重复**：两边各自把"同时带 `@smoke` 和 `@focus` 的用例放到最后单 worker 跑"做了一遍（本任务 129f102，T093 35120fe）。T093 先合并，本任务合并时编排者在冲突处只留一份。
- **设计 5.9.2 的剧本命令表**要补 `permission_options`，编排者补。
- **开发中曾直接跑 playwright、没有持锁**，可能干扰了 T093；T093 加的 `cargo xtask lock` 合并后都经它跑。
