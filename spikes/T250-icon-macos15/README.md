# T250 图标在 macOS 15 上的样子

原型实测代码，不是产品代码。设计师只在 macOS 26.4 上看过拿铁图标（`docs/meetings/2026-11-10-app-icon/`）：系统按新的图标形状重新裁边，大小和自带 App 一致，没有被套进灰色底板。这里在 GitHub 的 `macos-15` runner 上做同样的检查。

`check.swift` 做一个只带 icns 的探针 App，向 NSWorkspace 要它和几个系统 App 的图标（访达和 Dock 拿到的就是这个），并排画进 `results/compare.png`（一行 256 px、一行 64 px，最左是探针），系统版本写进 `results/info.txt`。

经 `spike-run` workflow 运行（由编排者触发）：

- dir：`spikes/T250-icon-macos15`
- command：`mkdir -p results && swift check.swift ../../apps/desktop/build/icon.icns`

只在 runner 上跑：向系统要一个新 App 的图标可能让 LaunchServices 记下它，不在本机运行。
