# T076 汇报

- 执行者：子 agent（工程师）
- 分支：`feat/T076-desktop-window-bridge`，在原 T052 的 worktree 里从 main 开出
- 完成日期：2026-10-20

## 做了什么

A、B 两部分都做完了，按先 A 后 B 的顺序分步提交。在干净的 worktree 里跑 `cargo xtask ci pr --record` 通过，摘要已提交。代码共 +1,385 −18 行，在约 1,500 行的上限以内；这个数不含生成物 `methods.gen.ts`、notes 和 CI 摘要。

**A 部分**

- **方法目录**：`tools/gen-protocol-ts` 从 `v1.json` 的 `x-methods` 生成 `packages/protocol/src/gen/methods.gen.ts`，内容是 `METHODS`、`CLIENT_NOTIFICATIONS` 两张表（方法名 → scope、幂等），由 `pnpm gen` 产出（T071 交接）。
- **windowBridge**（`apps/desktop/src/main/bridge/window-bridge.ts`）：
  - 入方向不解析，按 16 ms 批量，每批一个独立的 ArrayBuffer 交给窗口。
  - 出方向：解析、按白名单检查、重新序列化后再发，从不转发原始字节。
    - 白名单由方法目录生成，只放行 `read`、`write`、`approve`、`ui_local`；
    - 被拒的请求回 `forbidden`，`initialize` 回 `forbidden/already_initialized`；
    - 没有 `id` 的被拒消息、批量数组、非对象、坏 JSON 直接丢弃；
    - 超出 ±(2^53−1) 的整数拒绝；带重复键的消息转发重新序列化的结果（按你认可的第 1 条）。
  - `sendBlob`：超过握手给出的 `max_pending_upload_bytes` 就不发，对应的 `blob/put` 发出后释放额度。
  - 8 MB 流控：窗口未确认的字节超过 8 MB 时暂停读这条连接，确认后恢复（T075 交接）。
  - 重连顺序：先送完旧连接的数据，再推 `reconnecting`；等 control 连接就绪后再开新连接；新连接 `ready`（带握手）之后才转发它的数据。状态和数据走同一个端口。
  - 按窗口断连：`disconnect()`。
- **接线与文档绑定**（`bridges.ts`）：
  - `host:open` 只接受窗口主框架上的 `app://cupper` 文档，用 T075 的同一个 launcher 以 `window` 角色开连接，端口先于握手结果发出；
  - 主框架跨文档导航、`render-process-gone`、`destroyed` 都会关闭这条连接；同一窗口再次 `open` 时先关掉旧的；
  - control 连接加了 `ready()`；
  - e2e 构建在主进程上有 `globalThis.__cupperE2E.disconnectWindow(webContentsId)`。
- **preload 的 `ShellAPI.host`**（`preload/host.ts`）：经端口收发；preload 把 ShellAPI 暴露为 `window.shell`，名字定义在 `@cupper/shell-api` 的 `SHELL_GLOBAL` 里。
- **修了 T074 的 vite 配置**：preload 原来会把 npm 上那个"查找 Electron 二进制"的 `electron` 包打进产物（T074 时 preload 是空模块，没暴露出来）。现在把 `electron` 设为外部依赖，产物里只有一处 `require("electron")`。

**B 部分**

- **其余 ShellAPI 的主进程接线**（`main/shell.ts`）：
  - `init`：返回 `ShellInfo`，含 prefs 快照，`flavor` 照实给出；
  - prefs：`set` 落盘到 userData 下的 `prefs.json`，`onChange` 广播到所有窗口；
  - 外链：主进程再查一次只放行 http/https；
  - 选目录：调用时才取 `dialog.showOpenDialog`，e2e 可以替换；
  - 主题、窗口焦点：推送到窗口；
  - 上面这些都只回答窗口主框架上的主文档。
- **preload 的其余成员**：菜单命令仍走 `shell:command` 通道（T074 交接）。
- **`ShellInfo.flavor`**：以附加方式加上 `local`。
- **占位**：`requestPrivileged` 返回 `{status: "failed", error: "unsupported"}`，`onNavigate`、`updates` 只注册监听。
- **预测量**：任务目录下的 `premeasure.test.ts` 加一次性的 vitest 配置，数字写进 notes，见"验证方式"。

## 关键取舍

- **端口一侧的代码不依赖 Electron**：主进程和 preload 两端都抽成不依赖 Electron 的接口，所以合同测试和桥的测试能在 Node 里用 `worker_threads` 的 `MessageChannel` 跑，与渲染进程走同一段生产代码。
- **渲染进程拿不到任何 Electron 对象**：只有 `window.shell`。它通往宿主的唯一路径是 windowBridge 的端口，端口上只认 `send`、`blob`、`ack`、`close` 四种消息，其余一律丢弃。
- **重连时 control 失败或关闭**：窗口收到 `failed`，原因沿用 control 的原因。
- **规模偏差**：A 部分比估计多约 190 行（测试辅助、文档绑定的用例），B 部分比估计少约 150 行（接线没有单独的编号用例）。

## 验证方式

- 在干净的 worktree 里于 `83592eb` 跑 `cargo xtask ci pr --record docs/tasks/T076-desktop-window-bridge/ci/`，全部通过。摘要是 `pr-83592eb2524c.json`，提交为 `f6d38c1`。

  | 作业 | 结果 |
  |---|---|
  | rust | 1,503 通过 / 0 失败 |
  | protocol | 185 / 0 |
  | ts | 348 / 0 |
  | license | 1 / 0 |
  | testdata-scan | 3 / 0，`sensitive-words` 跳过 |
  | ui、e2e | 还没实现 |

- 对照验收标准：
  - **UNIT-bridge-whitelist**：遍历全部方法，以及畸形消息、超范围整数、未知端口消息、上传限额，都通过。✅
  - **UNIT-bridge-flow**：16 ms 批量、超过 8 MB 暂停、确认后恢复，都通过。✅
  - **UNIT-bridge-lifecycle**：等 control 就绪后重连、状态与数据的顺序、重连失败、断连钩子、`close` 不再重连、文档绑定，都通过。✅
  - **UNIT-hostport-contract-bridge**：与 mock 用同一套合同，8 个用例全部通过。✅
  - **渲染进程不能绕过桥**：只有 `window.shell`，只能经桥的端口发消息，`send` 和 `blob` 在桥里检查、重组后才写到套接字。✅
  - **代码约束**：没有新增 ui-core 代码；渲染进程侧没有用 `innerHTML` 等；主进程在调用时才取 `dialog.showOpenDialog`。✅
  - 测试名以编号开头，编号都在 T076 段里。✅
- **构建核对**：
  - `vite build --mode dev` 的 preload 产物只 `require("electron")`；
  - e2e 构建的主进程里有 `__cupperE2E`，release 和 dev 的产物里没有。
- **预测量**：判定负载为索引流加 4 条会话流、5 MB blob、5,000 事件重同步，共 10 秒，本机连跑 3 次。

  | 指标 | 结果 |
  |---|---|
  | 事件循环延迟 p50 / p99 / max | 1.15 / 1.36–1.37 / 5.9–8.9 ms |
  | 5 MB blob 送达 | 23–28 ms |
  | 5,000 事件重同步送达 | 15–17 ms |
  | 流控暂停 | 没有发生 |

  这组数字的局限：mock 宿主、桥和消费端在同一个 Node 线程里，所以延迟是桥本身的上限；不含原生扩展的 socketpair（8 KB 缓冲区），也不是 Electron 的 `MessagePortMain` 和渲染进程。`setsockopt` 加不加留给 T078 按真实测量定。

## 提交列表

```
f6d38c1 docs(task): record the T076 pr CI summary
83592eb docs(task): note that vite refuses the local build mode
ae7f3e6 docs(task): record the T076 pre-measurement, line counts and choices
cfe2a81 feat(desktop): expose the rest of the ShellAPI from the preload
f51f6c2 feat(desktop): open each window document's data connection
6a6f620 feat(desktop): add the window bridge and the preload's HostPort
f0027e5 feat(protocol): generate the method catalog for TypeScript
8cc99e8 docs(task): record the T076 checks, size estimate and split proposal
```

## 遗留问题 / 范围外发现

1. **local 构建跑不起来（范围外）**：`vite build --mode local` 被 vite 拒绝，因为 "local" 与 `.env.local` 的后缀冲突，T074 留下的 local 构建因此跑不起来。要换模式名，或改由别的方式传入 flavor，建议交给构建相关的任务。
2. **占位等待对应任务**：
   - `requestPrivileged` 由 T077 接上；
   - `onNavigate` 在 `shell:navigate` 通道上收，等深链接和通知的任务来发；
   - `updates.onState` 在 `updates:state` 通道上收，`installNow` 等更新器的任务。
3. **给 T077**：`interaction/respond` 和 `git/revert` 现在照常放行。真实输入检查加在 `window-bridge.ts` 的 `#send` 里，白名单检查之后、转发之前。
4. **给 T078**：
   - 预测量脚本可以直接改成真实环境的测量：`docs/tasks/T076-desktop-window-bridge/premeasure.test.ts`，配置在同一目录；
   - e2e 断连钩子是 `__cupperE2E.disconnectWindow`。
5. worktree 已保留。