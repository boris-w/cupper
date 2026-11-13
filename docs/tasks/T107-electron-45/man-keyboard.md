# MAN-keyboard：实体键盘确认（T107，Electron 45）

Electron 升了大版本（44.9.0 → 45.2.3），按 `docs/manual/README.md`，MAN-keyboard 要重做一次。

**步骤与 T099 的记录完全相同**，见 `docs/tasks/T099-m1a-exit/man-keyboard.md` 的"用哪个构建"、"构建"、"准备与启动"、"逐项"（1 至 7）、"收尾"五节。T099 记录里提到的两处缺陷，T217 已经修好，所以照"逐项"开头那段的写法做：先切到窗口、点一下空的输入框，再等请求到达。

与 T099 不同的有三处：

- **构建**：在 T107 的分支上构建（合并前的提交，`git status` 为空）。命令照旧是 `pnpm install --frozen-lockfile`、`cargo xtask runtime-pack`、`cargo build --locked -p cupper-fake-agent`、`pnpm build:e2e`。
- **核对 Electron 版本**：在"收尾"一节，`grep -m1 '"electron"' "$R/logs/main.log"` 应得到 45.2.3。
- **发请求的延时**：`later` 里的 `sleep 15` 改成 `sleep 45`。编排者回消息要几秒，15 秒时请求会在用户切回窗口之前到达（2026-11-13 实测，见结果第 1 项）。

## 结果

- 日期：2026-11-13
- 提交：`7eff08b4`（worktree T054 里的 e2e 构建，`git status` 为空）
- Electron：45.2.3（取自 `main.log`）
- macOS：26.4
- 键盘与输入法：实体键盘；中文输入法，组词时按 Enter 会把字母原样上屏，与 T099 的表现相同（用户没有另说是哪一款）
- 执行人：作者亲手按键；编排者照本文发请求，并查宿主 `host.db` 的 `interactions` 表
- 判定依据：作者的现场观察，加上 `outcome_json` 和请求到回答的时间（`resolved_at − created_at`）
- **发请求的延时用的是 45 秒，不是 15 秒**：编排者回消息要几秒，15 秒时请求会在用户切回窗口之前到达（见第 1 项）

| 项 | 结论 | 看到的现象 |
| --- | --- | --- |
| 1 Enter 批准 | 通过 | `allow-once`，14.0 s。前两次是步骤问题，不算：请求先到，用户把面板第 4 行"否，并告诉 Claude 应该怎么做"的空输入框当成了消息输入框点了进去，Enter 落在这个空框里，什么也不做；一次随后点了"拒绝"（`reject`，28.6 s），一次 Tab 到"提交"再按 Enter（`allow-once`，32.8 s）。改成 45 秒后，用户先点消息输入框，面板接管键盘，直接按 Enter 提交 |
| 2 Esc 拒绝（有特权选项） | 通过 | `reject`。用的是清掉上面那次请求时按的 Esc：请求带特权选项 2，焦点在第 4 行的空输入框里，按 Esc 立即拒绝，没有确认框。用户认为这已验证，没有在焦点位于选项列表时重做；两种焦点下 Esc 走的是同一个 close 回答 |
| 3a 数字键提交 | 通过 | 按 3 得 `reject`，52.2 s |
| 3b 效果未知的编号没反应 | 通过 | 按 2 没有反应，再按 1 得 `allow-once`（请求到回答 846.0 s，中间停了较久） |
| 4a 输入框里选词不发送 | 通过 | 消息没有发出（宿主的回合数没有增加） |
| 4b "在此输入"里选词不提交 | 通过 | 第一次 Enter 没有提交，第二次才提交，回答 "nihao"，15.0 s。与 T099 一样，是输入法把字母原样上屏 |
| 4c 打拼音时来请求，选词与随后 1 秒内的 Enter 不算 | 通过 | 用户回"好了"；按 Tab 之后再按 Enter，提交 `allow-once`，27.7 s |
| 5 打字时面板不抢焦点，Tab 后可提交 | 通过 | 用户回"好了"；按 Tab 之后再按 Enter，提交 `allow-once`，22.9 s。期间没有误发消息：回合共 10 个，与发出的请求一一对应 |
| 6 切回前台 1 秒内提示"请再按一次" | 通过 | 用户确认"符合"，随后提交 `allow-once`，32.9 s |
| 7 特权选项的编号不直接提交 | 通过 | 按 2 后请求仍是 pending（用户确认"符合"），收尾时 interrupt |

收尾：App 和宿主已退出，`dev.cupper.app.e2e` 的偏好和临时根目录已删除。截图不入库。

全部通过，证据已登记在 `tests/ids.toml` 末尾（T107）：

```toml
[[evidence]]
id = "MAN-keyboard"
date = "2026-11-13"
record = "docs/tasks/T107-electron-45/man-keyboard.md"
versions = { electron = "45.2.3" }
```
