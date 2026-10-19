# T045 cupper-cli I：客户端库与 Mirror

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，CLI）
- 分支：`feat/T045-cli-client-mirror`
- 依赖：T012、T014、T020（均已合并）。M1a 第 4 波，松弛为 1。T033、T039、T046 依赖它。

## 背景

`cupper-cli` 是给用户和 agent 用的命令行客户端（设计 5.2.14），也是宿主集成测试的客户端。本任务做它的库部分：
- 连上宿主、握手、请求关联；
- 订阅并维护本地的 Mirror：用 T014 的视图和同步规则，客户端同步状态机归本任务；
- 按构建种类校验宿主（5.8.1）。

子命令归 T046。`harness` 认证的证明由 T039 补上（计划 v1.3.5），本任务的握手只做 `peer`。

## 范围

**做**（计划 T045 一行；设计 5.2.14、5.3.3、5.8.1）：
1. **迁移 SP-4a 的 client 与 mirror**（`spikes/SP-4a-host-skeleton/crates/cupper-cli/src/`），放进 `crates/cupper-cli` 的库部分。
2. **连接与握手**：
   - 套接字发现（5.2.14）；
   - 读 `host/hello`，发 `initialize`，`auth: peer`；
   - 版本协商用 `VersionRange::negotiate`；
   - 读套接字一律经 `cupper_sys::recv_closing_fds`，收到 fd 就断开；
   - 给 T039 留好接入点，让它能加上 `harness` 认证：`--harness-key-fd` 和证明。
3. **请求关联**：
   - 按 `id` 配对请求和响应，通知单独分流；
   - 命令方法的 `command_id` 是 UUIDv7，在客户端生成（见交接事项）。
4. **Mirror 与客户端同步状态机**（交接自 T014）：
   - 订阅时带上游标（`StreamSubscribeParams::resume()`）；
   - 收到 `snapshot`，不论原因，包括不认识的 mode，都丢弃这条流的视图和游标；核对 `at_seq`、`epoch` 和页序号后组装；
   - 之后的事件经 `StreamCursor` 做连续性检查，再 `apply`；
   - `apply` 报 `ViewError` 时，不带游标重新订阅；
   - 收到 `stream/reset` 时保留游标；
   - epoch 改变时整体重置。
5. **按构建种类校验宿主**（5.8.1 的表）：
   - **发布**：签名要求；
   - **自用**：编译期写入的宿主 cdhash，经审计令牌取对端运行中的 cdhash 比对，之后每次读取都比对令牌；cdhash 不一致就拒绝连接。cdhash 的写入归 T005，本任务用一个编译期常量的接口，测试里可以注入；
   - **开发**：按同目录宿主文件的 cdhash 弱校验，输出里标"宿主未经签名校验"；
   - **e2e**：`harness` 证明，归 T039，本任务只留分支；
   - 发布构建遇到 `weak_auth: true` 的宿主时，拒绝连接。
6. **`--build-info`**：输出构建种类、版本和宿主校验方式。

**不做**：
- 子命令（T046）；
- `harness` 认证与证明（T039）；
- cdhash 的写入（T005）。

## 验收标准

- [ ] 新增 UNIT-mirror：
  - 快照分页组装；
  - 快照不论原因都丢弃旧状态；
  - 连续性缺口触发重新订阅；
  - `ViewError` 时不带游标重新订阅；
  - `stream/reset` 保留游标；
  - epoch 改变时整体重置。
  - 用 T014 的视图和样例事件驱动，不需要真实宿主。
- [ ] 新增 UNIT-cli-host-verify：
  - 四种构建的校验分支；
  - cdhash 不符时拒绝连接；
  - 发布构建拒绝 `weak_auth`。
- [ ] 测试要覆盖拒绝的路径。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T045-cli-client-mirror/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- **客户端同步状态机归本任务**（T014）：
  - 遇到 `snapshot`，不论原因，包括未知的 mode，都丢弃视图和游标；
  - 核对 `at_seq`、`epoch` 和页序号后组装；
  - 之后的事件经 `StreamCursor` 再 `apply`；
  - 遇到 `ViewError` 时不带游标重新订阅；
  - `stream/reset` 保留游标。
- **可以直接用的工具**（T012）：
  - `methods::handshake::PROTOCOL`；
  - `VersionRange::negotiate`，取双方交集中的最大版本；
  - `StreamSubscribeParams::resume()`，转成 T014 的 `Resume`。
- **`command_id`**（T022）：CLI 在客户端生成 UUIDv7 的 `command_id`，而工作区禁用了 `now_v7`。两种做法任选：
  - 加一个带 reason 的豁免，理由写"CLI 是独立进程，按真实时间给命令编号"；
  - 或者用自己的生成器。
- **读套接字**（T021）：
  - 一律用 `cupper_sys::recv_closing_fds`；
  - 判断对端已关闭，以读到 EOF 为准，不要关闭后立刻查 `getsockopt`。
- **启动载荷的密钥格式**（T023）：
  - 密钥是 64 位十六进制；
  - CLI 的 `--harness-key-fd` 用同样的格式，由 T039 接入。
- **惯例**：
  - 错误类型用 thiserror；
  - 拉子进程经 `cupper_sys::spawn`；
  - `unsafe` 只能在 `cupper-sys`。

## 并行注意

T030 正在做宿主的网关（`cupper-host/src/gateway/`），本任务只改 `crates/cupper-cli`。需要的协议类型都在 `cupper-protocol` 里。不要依赖 `cupper-host`：CLI 不能依赖宿主 crate。

## 规模

计划估为 M（400–900 行）。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项**；动手后每完成一个模块统计一次（守则第 8 条）。**预计超出约 1,500 行时，必须先停下来汇报，不要继续写**。

## 参考资料

- 设计：
  - `02-host.md` 5.2.14（cupper-cli）；
  - `03-protocol-v1.md` 5.3.2、5.3.3、5.3.11 末尾 T014、T030 的裁定；
  - `08-security.md` 5.8.1（各构建的宿主校验）。
- SP-4a 的 client 与 mirror 及其报告。
- T014 的 `view/`、`sync.rs`；T012 的 `methods/`；T021 的 `cupper-sys`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料（`notes.md` 等）由执行者在任务分支上提交。
