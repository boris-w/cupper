# M1a 同步会（检查点 K4）

- 日期：2026-11-02
- 参与：用户、编排者
- 形式：会话中的决定。议程照 T099 的 `record.md` 第 8 节；结果与缺口见 `record.md` 第 1–6 节

## 背景

- M1a 退出候选是 ec459fd。GitHub 上的四项都通过了：
  - `ci` 的 8 个检查（41413977102）；
  - nfr1 连跑 20 次，240/240（41414667873）；
  - 降速变体 5 次，60/60（41414671830）；
  - nightly 6/6（41414675363）。
- MAN-keyboard 由用户 2026-11-02 用实体键盘做完，7 项全部通过。
- 作者机器上的 nightly 按用户的决定没有跑（`docs/meetings/2026-11-02-perf-local/`）。所以 `trace-ids verify --milestone M1a` 有 7 个编号没完成：
  - PERF-host-cold-start、PERF-index-snapshot、PERF-host-memory、PERF-snapshot、PERF-bridge-gate；
  - IT-sys-peer-current，以及等它的 IT-sys-peer。
- 第一个候选 18b3640 因测试缺陷作废。验收过程中修掉了 T216（测试）、T217（请求面板的焦点）、T218（adapter 退出时丢消息的产品缺陷）。

## 决定

- **D1 性能闸门**：用户选"只记录、不卡闸门"。
  - PERF 照常在 GitHub 的 nightly 里跑，写进历史，只看趋势，不作为里程碑退出条件。
  - 有了自用构建、用户愿意时，再在真机上补测，届时再定是否恢复闸门。
- **D2 真机对端校验**：用户选"用 macOS 26 runner 的结果代替"。用户的 Mac 是 macOS 26.4，runner 是 macOS 26.6.2，同一个大版本已经覆盖，IT-sys-peer-current 改为不适用。
- **D3 MAN-keyboard**：不在 T217 之后重做。T217 只改了焦点规则，按键核对没变；下次 Electron 大版本升级（T107）时照常重做。
- **D4 真实 Claude 兼容结果**：不重跑。M1b 接 Codex（T111、T113）时再跑真实兼容，届时一并覆盖 T110、T218 的改动。
- **D5 K4**：M1a 算通过。用户原话："算通过，改完就开工 M1b"。
  - D1、D2 要改计划、设计和登记，由 T219 完成。T219 合并、`trace-ids verify --milestone M1a` 通过后，打 tag `gate/m1a`。
  - 之后派发 M1b 第一波（计划 4.2 的 B1，含自用构建 T160），并行约 4 个子 agent。

## 落实

- T219：PERF 改为只记录、不卡闸门；IT-sys-peer-current 改为不适用。编排者写任务书并派发。
- 计划：T099 打勾，K4 记为通过（D5），版本记录。
