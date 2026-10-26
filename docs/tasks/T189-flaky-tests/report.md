# T189 汇报

- 执行者：子 agent（T093 的执行者）
- 分支：`fix/T189-flaky-tests`（已合入 main 6b43eff）
- 完成日期：2026-10-26

**结论**：三处偶发失败都已查明根因并修好，带记录的 `ci pr` 通过。只剩一项：**E2E 50 次待定时段**。已经跑过一次 50 次，失败 1 次，且失败那次运行期间有用户输入；50 次里有 45 次运行期间有用户输入，没有输入的 5 次都通过。这不能算"没人用机器时连跑 50 次"，要另找时段补跑。

## 做了什么

1. **`unit_fake_scripts_faults_per_segment`**（f877e03）：
   - **根因：用例的等待写错了**。`Fake::exit` 本意是等 10 s，但假 agent 一关 stdout，`poll` 就立刻返回，200 次尝试几毫秒就用完了。实测三次失败分别只等了 1–5 ms。负载高时，假 agent 关掉 stdout 之后还要几毫秒才真正退出。
   - **修法**：stdout 关闭后，每次尝试之间用一个没人发送的通道再等 50 ms。没有放宽时限，只是让写好的 10 s 真的生效。
   - **数据**：修复前，整套 `scripts` 测试 100 次失败 1 次、30 次失败 2 次；修复后同样负载下 220 次 0 失败。
2. **E2E-SEC-real-input**（70f8950）：
   - **根因**：这条用例靠 App 启动时拿到的焦点。机器忙时焦点可能晚到，或者被拿走再还回来；只要获得焦点的时刻落在按键前 1 s 内，提示就会按规则 7 变成"请再按一次"。产品照设计在做，是用例的前提没守住。
   - **修法**：`focusWindow` 先抢焦点，等到窗口确实拿到焦点，再等 1.1 s；这条用例改为调用它。
3. **信任闸门挂住**（7597c77，产品修复，单独提交）：
   - **先用回归用例证实了竞态**。新用例 `it_orch_trust_gating_an_older_rescan_coming_last_changes_nothing` 借 T069 旁边新加的、只在测试里存在的钩子 `hold_reopen_result`，让一次看到"未信任"的旧复查结果在新复查（看到已信任）之后才到。
     - 在没改的产品代码上 20 次都失败，输出：
       ```
       left: [("project_config_changed", Null), ("project_config_changed", Null)]
       right: [("project_config_changed", Null)]
       ```
       旧结果把刚打开的闸门又关上，还多发一次通知。如果这时首个回合还在准备阶段，prompt 就永远发不出去。这就是 T064 那次挂 28 分钟的机理。
   - **修法**：
     - 复查编号；结果比已经回来的最新那次旧时，改用最新那次的结果，闸门不翻转、不重复通知，索引和审计也不按旧结果记。
     - 产品逻辑约 30 行，不改接口；测试钩子约 40 行，只在测试中编译。
4. **同类排查与等待上限**（cfb0189）：
   - `Fake::quiet` 在 agent 的 stdout 关闭后改为直接失败，避免已经退出的 agent 被当成"安静"而假通过。它的 5 个调用方都要求 agent 活着，修改后都通过。
   - 会话夹具的 `call`（`ok`、`err` 都经过它）也套上 `soon`，回复 30 s 不来就失败，并报出 "a reply to <方法>"。这条用例走到的 `events`、`attach`、`command`、`call` 现在都有上限。

## 关键取舍

- 第 1、2 处只改用例，因为产品行为符合设计；第 3 处是真实的产品竞态，有回归用例为证才改。
- 第 3 处让旧结果套用最新那次的结论，而不是直接丢掉：Check 和 Attach 仍要照常答复，只是依据换成更新的扫描。

## 验证方式

- **第 3 处**：新用例在修复前 20/20 失败、修复后通过；宿主整套 lib 测试（403 个）通过。后台 4 份 xtask 整套、负载约 19–24 时，连跑 40 次 0 失败。
- **E2E 50 次**：17:38:29–17:43:18，一次持锁调用跑完（`test:e2e:one ... --repeat-each=50`），没有等锁，用的是包含全部改动的同一份构建。每 0.5 s 采样 `HIDIdleTime`。

  | 运行 | 次数 | 失败 |
  |---|---|---|
  | 运行期间有用户输入 | 45 | 1（第 27 次，仍提示"请再按一次"，最短空闲 0 ms） |
  | 运行期间没有用户输入 | 5 | 0 |

  空闲时间多数时候每一两秒就归零一次，用户这段时间大部分时候还在用机器。
- **带记录的 `ci pr`**（7f34ab3，摘要 `pr-c857b87d5115.json`）：rust 2172、protocol 188、ts 483、license 5、testdata-scan 3、e2e 30，全部通过。ui 没有涉及的改动，跳过。
  - 之前有一次 CI 跑到一半，我为了先跑 50 次把它停了，留下的摘要不完整，已删除，没有提交。
- **规模**：`cargo xtask size` 为增 148、删 17，停止线 280。

## 提交列表

```
7f34ab3 docs(tasks): record T189 PR CI summary
c857b87 docs(tasks): T189 notes, E2E-SEC-real-input's 50 runs and the input during them
4c6d559 docs(tasks): T189 notes, the trust race proven, fixed and bounded
cfb0189 test: quiet needs the fake agent up, and every session fixture call has a limit
7597c77 fix(host): an older trust rescan coming back last changes nothing
6b43eff Merge branch 'main' into fix/T189-flaky-tests
fc00f3a docs(tasks): T189 notes, the focus wait and the trust gating hang
70f8950 test(e2e): the window's cooldown counts from when it has the focus
32e8b64 docs(tasks): T189 notes, the fake agent's exit wait
f877e03 test(fake-agent): waiting for the agent to exit waits as long as it says
```

## 遗留问题 / 范围外发现

- **E2E 50 次待定时段**：E2E-SEC-real-input 还要在真正没人用这台机器的时段补跑 50 次。做法照这次：一次持锁调用跑完，逐次采样 `HIDIdleTime`。
- **所有 `@focus` 用例都有同样的前提**：运行期间有人用机器时，焦点变化会让按键落进冷却期，抢到焦点的窗口还可能收到用户的输入。T099 的连续 20 次全绿，要安排在没人用机器的时段跑。
