# T260 评审

- 评审人：编排者
- 日期：2026-11-13
- 结论：**通过，合并**（PR #113）

## 核对

- **规模**：+55，停止线 150 以内。
- **CI**：PR #113 https://github.com/boris-w/cupper/pull/113 ，40e0da4a 上 8 个检查全过。
- **连跑**：
  - E2E-review-comment：main 基线一次一份、一次两份各 19/20（43379698230、43379703136），分支上各 20/20（43384898710、43384903110）；
  - helpers ×40：基线 40/40，分支 40/40（43384907162），复现不了；
  - nightly `job=differential`（43384911286）：收集到 901 个测试，全部通过，没有跳过，最慢一条 36.8 ms。

## 逐项意见

1. **review 的原因找准了**：pierre 先画纯文本、高亮后再画一遍，替换掉悬停时挂上"+"的行元素。执行者先用基线排除了"窗口互相遮挡"，再用临时用例量出替换的时机，改成等带颜色的 token 出现，没有用重试循环或换纯文本文件绕开。
2. **visibilityChanged 修在产品里**：窗口关闭时去掉五个可见性事件的监听，注释附上游 electron/electron#53971（我用 gh 核对过它存在，标着 needs-manual-bp/45-x-y）。复现不了，按栈修，以 nightly e2e 为准。T254 在 E2E-SEC-fake-host 见到的是同一个栈，这次一并解决。
3. **differential 去掉总时长的上限**：每个用例一个测试，单个用例只要几毫秒，失败时直接报出是哪一条；没有连跑 10 次 nightly。
4. **security-peer 的取消**是 GitHub 的 macOS runner 容量问题，只记录。

## 记下的点

- 写进交接事项：T107 升到 Electron 45 时，查 #53971 是否已回合，回合了就删掉 `dropVisibilityListenersOnClose`；连跑差分测试要派 nightly `job=differential`。
