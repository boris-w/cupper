# SP-1 适配器行为实测

一次性的原型实测代码，只用于验证，产品代码不依赖它。任务说明见 `docs/tasks/SP-1-adapter-behavior/brief.md`，结论和工作笔记见同目录的 `notes.md`。

## 文件

| 文件 | 作用 |
|---|---|
| `acp-client.ts` | 最小 ACP 客户端：stdio 上的换行分隔 JSON-RPC，录下每一帧；自动应答审批和 elicitation；进程树和内存采样 |
| `scenarios.ts` | brief 的 12 个场景，另有几个补充场景：`s02b` 明确要求使用计划工具，`s02c`/`s02d` 是 Claude 的任务工具和思考流补充实验，`s04b` 拒绝命令审批，`s07b` CLI 进程在空闲时被杀后的恢复。每个场景对每个适配器写一个 JSONL |
| `run.ts` | 入口：准备临时测试仓库、组装环境变量、按顺序跑场景 |
| `sanitize.ts` | 脱敏：把原始录制转换成可以提交的 `recordings/` |
| `inspect.ts` | 把一个录制文件打印成紧凑的时间线，方便人工查看 |
| `summarize.ts` | 从脱敏后的录制里提取延迟、`stopReason`、update 类型计数和内存采样 |
| `recordings/<adapter>/<scenario>.jsonl` | 脱敏后的录制 |

## 客户端为什么手写 JSON-RPC

brief 允许用 TS SDK（`@agentclientprotocol/sdk`）或 Rust SDK（`agent-client-protocol`）。这里用 TypeScript，但没有用 SDK 的 `ClientSideConnection`，而是直接读写换行分隔的 JSON-RPC：

- 目的是录下线上的原始帧，包括 `_auth/status_update`、`_session/steering`、各种 `_meta` 这类扩展内容。SDK 会做解析、校验和路由，录到的就不一定是线上的原样。
- 需要故意制造并发和时序：回合进行中再发一个 `session/prompt`，在工具调用刚出现时插话，挂起审批再取消，直接杀掉 CLI 子进程。手写传输层更容易控制这些细节。
- 录制文件与语言无关。宿主按 ADR-02 用 Rust，以后的假 agent 和回归测试直接读这些 JSONL 就行，不需要复用这份 TS 代码。
- Node 24 能直接运行 `.ts`（类型擦除），不需要安装依赖或编译。

客户端在 `initialize` 中声明的能力见 `acp-client.ts` 里的 `CLIENT_CAPABILITIES`：表单和 URL 两种 elicitation、unstable 的 `plan`（`plan_update`）、boolean 配置项、`notices`、`compaction`、terminal 认证，以及 Zed 约定的 `_meta.terminal_output`。`fs` 和 `terminal` 声明为不支持，两个适配器在实测中都没有调用它们。

## 录制格式

每行是一个 JSON 对象：

```json
{"seq": 12, "t": 1450, "dir": "recv", "msg": { "jsonrpc": "2.0", "method": "session/update", "params": { } }}
```

- `t`：距离该场景开始的毫秒数。一个场景如果先后启动多个适配器进程（例如 `s09` 的崩溃和恢复），所有进程共用同一个时间起点，`seq` 在每个进程内从头计数。
- `dir`：`send` 是客户端发给适配器的帧，`recv` 是适配器发来的帧，`recv-nonjson` 是 stdout 上无法解析成 JSON 的行（这次实测没有出现），`note` 是客户端自己的标注（场景步骤、延迟、进程采样、安全检查等）。
- 文件第一行是 `header` 标注，写明适配器、版本、日期和客户端能力。

## 运行

```sh
BASE=$(mktemp -d)                       # 放在任何已有仓库之外
mkdir -p $BASE/adapters/claude $BASE/adapters/codex
(cd $BASE/adapters/claude && npm init -y && npm install @agentclientprotocol/claude-agent-acp@0.91.0)
(cd $BASE/adapters/codex  && npm init -y && npm install @agentclientprotocol/codex-acp@2.2.2)

node run.ts --base $BASE --adapter claude        --scenarios s01,s02,s02b,s02c,s02d,s03,s04,s04b,s05,s06,s07,s07b,s09,s10,s11,s12,s08
node run.ts --base $BASE --adapter codex-bundled --scenarios ...   # 适配器自带的 codex
node run.ts --base $BASE --adapter codex-local   --scenarios ...   # CODEX_PATH 指向本机 codex

node sanitize.ts --in $BASE/raw --out recordings --base $BASE --terms $BASE/terms.txt
node summarize.ts recordings > ../../docs/tasks/SP-1-adapter-behavior/data/summary.json
```

- `s09` 依赖 `s02` 留下的会话，`s08` 放在最后跑，这样能看到前面场景建立的会话。
- 会真实调用模型、消耗少量额度，也会在 CLI 的历史里留下会话记录。

## 安全约束在代码里的落实

- 会话的 `cwd` 一律是 `$BASE/repos/<adapter>`：一个新建的一次性 git 仓库，提交身份是占位的 `cupper-sp1 <sp1@example.invalid>`。
- 发送任何 prompt 之前都会调用 `assertSafe`：只允许 Claude 的 `default`、`plan` 模式，以及 Codex 的 `read-only`、`workspace-write` 模式（比默认的 Auto review 更严格），并且必须已经选中最便宜的模型（Claude 是 `haiku`，Codex 是 `gpt-5.6-luna`），推理强度选最低档。不满足条件时直接抛错，不发送 prompt。
- 回答审批时只会选 `allow_once` 或 `reject_once`，代码里不存在选择 `allow_always` 或 `reject_always` 的路径。
- 传给适配器的环境变量只保留一个很小的白名单（`HOME`、`PATH`、`SHELL`、`TMPDIR`、`LANG`、`USER` 等），调用方会话的变量和密钥都不会传过去。
- `s10` 用空的 `CLAUDE_CONFIG_DIR` 或 `CODEX_HOME` 模拟未登录，从不调用 `authenticate` 或 `logout`。
- `s08` 在写入磁盘之前，就把不属于本次实测的会话替换成只包含字段结构的形状，不保存标题和路径。

## 脱敏

`sanitize.ts` 的规则全部从运行环境中读取（`HOME`、`USER`、`TMPDIR`、主机名），另外可以用 `--terms` 指定一个本地词表（例如组织名），所以脚本本身不包含任何个人信息。规则如下：

- 路径：临时目录写成 `<tmp>`，系统临时目录写成 `<tmpdir>/`，家目录写成 `$HOME`；用户名写成 `<user>`，主机名写成 `<host>`，词表中的词写成 `<redacted>`。
- 邮箱、`sk-` 开头的密钥、JWT、Bearer token，以及 64 个字符以上、字母和数字混合的 base64 或十六进制串（签名、哈希）都替换掉；`signature` 字段一律替换。测试图片的 base64 不替换。
- `_auth/status_update` 中的账号字段只保留 `plan`。
- 用户自己安装的 skill 和命令（Claude 中描述以 `(user)`、`(project)`、`(claude.ai sync)` 结尾的命令，名字形如 `<来源>:<名字>` 的插件和 MCP 命令，Codex 中以 `$` 开头的命令）替换成 `user-skill-N`，只保留数量。
- 会话 ID 换成 `session-NN`：先读完全部输入再编号，同一个会话在所有录制里用同一个编号。
- 用户自己的 MCP 服务器和插件的名字：`mcp__<服务器>__<工具>`、`plugin:<插件>:<服务器>`、Codex 配置警告里的 `mcp_servers.<名字>`，以及 `*_mcp_servers` 列表里的名字，都换成占位。
- 模型列表只保留公开的 `gpt-*` 和 Claude 模型名。本机的 Codex 接的是一个私有网关，网关目录里的其他条目会被删掉。
- 进程信息只保留角色名（`adapter (node)`、`claude CLI`、`codex app-server` 等），用户自己配置的 MCP 服务器、hooks、插件进程统一写成 `user-configured tool or MCP server`。注记里任何带 `pid` 和 `label` 的对象都按这条处理（包括 `procsAtExit`）。脚本对自己的输出再跑一遍，结果不变。

写完之后，脚本会把所有输出文件重新扫描一遍。只要还能找到家目录、`/Users/`、`/var/folders`、用户名、主机名、词表中的词或者邮箱，就以非零状态退出。
