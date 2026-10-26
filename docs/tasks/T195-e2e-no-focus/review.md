# T195 评审

- 评审人：编排者
- 日期：2026-10-26
- 结论：**通过，合并**（PR #2）

## 核对

- **规模**：+69 −9，停止线 300 以内。
- **CI**：PR #2 https://github.com/boris-w/cupper/pull/2 。最后一轮在 ea7f1ec 上 8 个检查全绿（run 40326991029）。e2e 冒烟子集里 17 个后台用例都过了前台采样和 Dock 检查，10 个 `@focus` 用例照常跑完。前几轮的 ts 失败是 T198 记下的 `settle()` 固定等待，e2e 失败是 T197 修好的缓存问题，都与本任务无关。
- **反向检查**：让主进程不认开关，smoke 用例报出 "took the foreground in 10 of 10 samples" 和 "in the Dock"，证明检查有效。远端的临时分支已删。
- **验证改到 GitHub runner 上做**：开工时本机屏幕已锁；runner 上没有人操作，正适合"前台从不是测试 App"这种检查。

## 逐项意见

1. **先实测再选做法**：四种显示方式的前台采样数据清楚，`accessory` 加 `showInactive()` 两个都要用，各管一件事。
2. **只在 e2e 构建里生效**：以 `__CUPPER_FLAVOR__ === "e2e"` 为条件，发布和自用构建里是死分支；变量名加进了 `flavors.test.ts` 的入口名检查。
3. **由标签决定后台模式，两头都不会漏**：漏标 `@focus` 的会在 `focusWindow` 失败，没标却抢前台的会被采样抓出来。
4. **抓到了测试代码自己的问题**：`gate.spec.ts` 的 `cover()` 建遮挡窗口时没写 `show: false`，默认显示就激活了 App。修好后 gate 在 runner 上重跑通过，`throttled` 测量照常输出数字。

## 记下的点

- `showMainWindow()` 在窗口已存在时（`second-instance`、`activate`）仍调 `show()`。现有用例走不到这里，以后有后台用例触发时，前台检查会直接报出来。
- 全量 e2e 还没在后台模式下跑过，下一次 nightly 会跑到。
