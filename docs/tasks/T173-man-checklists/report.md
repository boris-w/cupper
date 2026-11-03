# T173 汇报

- 执行者：子 agent（测试与文档）
- 分支：`docs/T173-man-checklists`（PR #56：https://github.com/boris-w/cupper/pull/56 ，提交 d42cc07）
- 完成日期：2026-11-03

**状态**：四份检查表、证据格式说明和 trace-ids 的小改动都已完成。PR 的 8 个检查过了 7 个；**rust 没过**，卡在 `cargo fmt --check`，原因与本分支无关，见"验证方式"。

## 做了什么

1. **`docs/manual/`**：新目录，只放检查表和格式说明；每次执行的结果写在执行任务的卷宗里。
   - `README.md` 是统一的证据格式，分四节：
     - 第 1 节，记录文件：放在 `docs/tasks/<任务>/<编号小写>.md`，依次写标题、执行信息、逐项结果、发现、证据片段。执行信息的固定字段：检查表及其提交、日期、执行人、构建种类与提交、版本、环境（不写路径和项目名）、判定依据。逐项结论只有三种：通过、不通过、未做（未做要写原因）。
     - 第 2 节，`[[evidence]]` 与登记表：每个组件的版本从哪里取；三种精度 `any`、`major`、`exact` 的含义；`check` 和 `status`/`verify` 各核对什么。规定只为通过的执行登记证据：有一项不通过或未做，记录照样提交，编号保持待完成。
     - 第 3 节，现有 11 个 MAN 编号逐一对照：记录放在哪里，登记表应要求哪些版本，套不上的写明原因。
     - 第 4 节，MAN-S 的共同约定：用自用构建 Cupper Local，在作者的真实仓库里另开一个可以丢弃的分支来走；用真实 agent，会消耗额度，按须知开沙箱；MAN-S2 至 MAN-S4 每一步 Claude、Codex 各走一遍；依赖的任务还没合并的步骤记为"未做"；T179 顺带检查的交互细节；不通过时怎么处理。
   - `man-s1.md` 至 `man-s4.md`：对应 PRD 的 S1 至 S4，分别 8、8、7、7 步。每一步写操作、预期与通过条件、依赖的任务号，并注明对应的需求编号和自动化版本 E2E-S1 至 S4。
     - S1：同时用两种 agent，所以只走一遍。依赖 T124、T144、T151、T153、T125。
     - S2：依赖 T141、T150；其余步骤用的是已合并的 T083、T134、T165。
     - S3：依赖 T132、T130、T131（或 T139）、T121、T123。
     - S4：依赖 T140、T149、T147。
2. **trace-ids**（`tools/trace-ids/src/registry.ts`、`evaluate.ts`，含测试）：
   - 新增精度 `any`：证据必须写出这个组件的版本，但不和当前版本比较，所以不会因版本变化而过期。
   - 组件从只有 `electron` 扩充为 `electron`、`cupper`、`macos`、`claude`、`claude-agent-acp`、`codex`、`codex-acp`。
   - `major`、`exact` 仍然只能用于 `electron`，因为只有它的当前版本 trace-ids 读得到（来自 `apps/desktop/package.json`）。其他组件写 `major` 或 `exact` 时，登记表报错。
3. **`tests/ids.toml`**：
   - MAN-S1 至 MAN-S4 改为要求证据写出六个组件，精度都是 `any`：`cupper`、`macos`、`claude`、`claude-agent-acp`、`codex`、`codex-acp`。这四个编号在 [T179] 段里，只改了字段，没有删行。
   - 表头对 `versions` 的说明同步更新，并指向 `docs/manual/README.md`。
4. **`docs/tasks/T173-man-checklists/notes.md`**：规模的估计与实际，以及取舍。

## 关键取舍

- **MAN-S 为什么只要求写版本、不和当前版本比较**：
  - 要比较，trace-ids 得知道当前版本，目前只有 Electron 做得到。CLI 是用户自己装的，版本随时会变，仓库里查不到。适配器的版本虽然在 `runtime/manifest.json` 里，但会随兼容工作（T113、T066）不断升级。
  - 如果按 `exact` 绑定，M1b 退出之后只要升级一次适配器或 CLI，MAN-S1 至 MAN-S4 就会从"完成"变成"问题"。这几条证据要说明的是"M1b 退出时在这些版本上走通了"，而不是"对以后的所有版本都有效"。
  - 所以只要求写上，版本留在证据和记录里供日后对照。以后如果想在升级后强制重做，可以加 `max_age_days`，或者让 trace-ids 读 manifest 里的适配器版本，都不用改现在的格式。
- **其他 MAN 编号为什么先不动**：
  - 已有的两条证据 MAN-compat-claude、MAN-notification-spike 都没写版本。如果现在给它们的登记加上 `versions` 要求，`check` 会立刻报错，等于把已经存档的验收作废。
  - 其余编号分属别的产出任务（T108、T150、T152、T148、T113、T183、T180、T157、T162）。按计划 1.5，各任务只管自己那一段。README 第 3 节给了每个编号建议登记的 `versions`，由各任务派发时登记，这样它们的第一条证据就会按新格式写。
  - MAN-keyboard 的 `electron = "major"` 绑定保持原样，它本来就该随 Electron 大版本重做。
- **检查表和记录分开**：检查表以后可以修订，不影响旧记录；每份记录写明当时用的是哪个提交上的检查表。`man-keyboard.md` 是两者写在一起的先例，作为参照保留，不改。

## 验证方式

- [x] 四份检查表和格式说明已写好，请审阅（你已看过 man-s1）。
- [x] trace-ids 本机验证：
  - `node tools/trace-ids/src/main.ts check`：442 个编号，与文档一致；
  - `status`：MAN-S1 至 MAN-S4 显示"no evidence"，已有的三条证据仍为完成；
  - `pnpm --dir tools/trace-ids run test` 24 个全过，新增用例覆盖三点：`any` 被接受；`claude = "exact"` 被拒；当前版本未知时，`any` 的证据仍算完成；
  - tsc、eslint、prettier 都干净。
- [ ] PR 的 8 个检查：https://github.com/boris-w/cupper/actions/runs/41619427327 ，过了 7 个。
  - **rust 卡在 `cargo fmt --check`**：`crates/cupper-host/src/orchestrator/session/tests.rs` 第 301 行的 `SettingsHandle::load(...)` 需要折行。这是 T126 合并后留在 main 上的格式问题，main 已由 2b52d12（`style(host): format the session test fixture after the T126 merge`）修好。
  - 本分支基于 8a33238，没有碰这个文件。请把 main 合进来，或在含 2b52d12 的 main 上重跑；我按守则没有自己 rebase 或 merge。
- 规模：文档约 205 行（停止线 400，硬停止线 500）；代码另计，`cargo xtask size` 为 +45 −14（trace-ids 含测试，加 ids.toml）。

## 提交列表

```
d42cc07 docs(manual): the MAN-S1 to MAN-S4 checklists and one evidence format for every MAN ID
651b209 test(ids): MAN-S1 to MAN-S4 evidence names the build, macOS, the CLIs and the adapters
42beb1a feat(trace-ids): manual evidence can be required to name a version of any component
```

（notes.md 也在 d42cc07 里。）

## 遗留问题 / 范围外发现

1. 设计 5.9.10 的"人工验证清单"一节和计划 1.6"格式由 T173 定"一句，建议补上"格式见 `docs/manual/README.md`"。
2. 其他 MAN 编号的 `versions` 登记：按 README 第 3 节的建议，由 T108、T150、T152、T148、T113、T183、T180、T162 派发时各自登记，建议写进这些任务的任务书。
3. 以后可选的小改进：trace-ids 从 `runtime/manifest.json` 读适配器的当前版本，`claude-agent-acp`、`codex-acp` 就能用 `exact`。需要时再做。
4. 检查表里有两处具体操作要看实际界面：S2 第 6 步 Claude 的 plan 模式在输入框底栏切换（取决于 T141 怎么实现），S4 第 4 步"跟进消息默认排队"在设置页（T147）。T179 走之前请对照实际界面核一遍。
