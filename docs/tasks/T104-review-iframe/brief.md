# T104 Q29 审查文档可行性（SP-6 场景 9）

- 派发日期：2026-11-04
- 执行者：子 agent（前端工程师，桌面与渲染进程安全）
- 分支：`spike/T104-review-iframe`
- 依赖：T074（`app` 协议与 CSP，已合并）。T131（审查文档，M1b 关键链）依赖本任务的结论（K9）；不通过时改做 T139

## 背景

设计 5.7.5 把审查面板的 diff 视图放进一个单独源的 iframe（`app://cupper-review/`），原样使用 @pierre/diffs；5.7.8 定了它的 CSP 和 sandbox。这个方案能不能落地是待定问题 Q29：
- constructable stylesheet 受不受 CSP 约束；
- 跨源 iframe 里 Worker 和模块脚本能不能加载；
- 桌面确认的 8 点；
- 5,000 行 diff 的性能。

本任务用原型验证，结论决定 T131 还是 T139（计划 K9）。v1.3.86 起 SP-6 的测量都在 GitHub runner 上跑，只记录（`docs/meetings/2026-11-03-sp6-runner/`）。

## 范围

**做**（计划 T104 一行；设计 5.7.5"审查文档"、5.7.8、5.9.7 场景 9）：
1. 在 `spikes/T104-review-iframe/` 搭双源原型，**协议注册和 CSP 与 `apps/desktop` 相同**：
   - `app` 协议注册为 `standard`、`secure`，`app://cupper` 与 `app://cupper-review` 两个源；
   - 两份 CSP 照 5.7.8 的表，审查文档不启用 `require-trusted-types-for`；
   - iframe 加 `sandbox="allow-scripts allow-same-origin"`、`allow="clipboard-write"`；
   - 用 `will-frame-navigate` 拦截导航；
   - preload 只在主框架运行。
   能直接引用 `apps/desktop` 现有代码的就引用，不要复制一份走样的。
2. 在 iframe 里原样使用 @pierre/diffs（`workerFactory`、`shiki-js`），验证：
   - constructable stylesheet 在审查文档的 CSP 下能用；
   - Worker、模块脚本在跨源 iframe 里能加载；
   - `securitypolicyviolation` 计数为 0。
3. 桌面确认的 8 点（5.7.5 的表）逐点给结论。能在原型里实测的就实测：
   - 进程外 iframe 与内存；
   - 焦点与快捷键（菜单加速键在焦点位于 iframe 时仍生效）；
   - `key_forward` 不算真实输入；
   - 主题、语言同步；
   - MessageChannel 的建立与"只认第一个端口"；
   - 剪贴板；
   - 滚动联动。
   不能实测的写明理由。
4. **场景 9 的测量**：5,000 行 diff 在发布 CSP 下的冷启动时间、内存、滚动帧时间、违规数，在 GitHub 的 `macos-15` runner 上跑，只记录不判定。怎样在 runner 上跑（例如一个手动触发的 workflow），在估算里一起提出；如果 workflow 文件要先进 main 才能触发，告诉我，由我处理。
5. **Q29 判定报告**：写进本目录的 `report-q29.md`：
   - 每项验证的结果和证据；
   - 测量数据；
   - K9 的建议：通过（做 T131）或不通过（做 T139），以及理由。
   - 功能项（CSP、Worker、模块脚本、违规数、8 点里的硬性项）决定通过与否；性能数据只记录，对照 5.9.6 的预算作参考。

**不做**：产品里的审查文档（T131）；主文档的审查面板（T130、T232、T233）。

## 验收标准

- [ ] 原型在本机能跑；场景 9 在 runner 上至少跑一次，有数据。
- [ ] `report-q29.md` 覆盖上面第 2、3、4 项，给出 K9 的建议。
- [ ] 新增依赖（@pierre/diffs 及其依赖）的许可证在 ADR-11 的允许清单里。不在的话，先停下来报给 `main`。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：600；硬停止线：700

<!-- 原型代码也计入。执行者开工前在 `notes.md` 里按模块写行数估计，测量脚本、workflow 单独列一项，估完报给 `main`，等回复再写代码。每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.7.5（审查文档、8 点、后备方案）、5.7.8（CSP、Trusted Types、RP-11）、5.9.6（预算）、5.9.7（场景 9）、13 的 Q29、ADR-03。
- 卷宗：T074（协议与 CSP）、T143（Shiki 的 JavaScript 正则引擎）、T130（审查面板留给 T131 的 diff 视图接口，在它的 notes 里）。
- 纪要：`docs/meetings/2026-11-03-sp6-runner/`。

## 注意

- 原型代码只放在 `spikes/T104-review-iframe/`，产品代码不依赖它；提交前去掉个人信息（本机路径、用户名、主机名等）。
- diff 夹具用生成的合成内容，不用任何真实仓库的代码。
- 本机只跑单份、短时的检查；测量放到 runner 上（守则第 10 条，用户不在本机跑性能测量）。
- 不运行真实的 Claude Code、Codex CLI。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。`report-q29.md`、`notes.md` 和测量数据由执行者在任务分支上提交。汇报里不要写本机的绝对路径。
