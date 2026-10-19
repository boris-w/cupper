# T003 技术验证：Q8（Claude）、Q9、SP-4b 遗留第 7 条

一次性的原型实测代码，只用于验证，产品代码不依赖它。任务说明见 `docs/tasks/T003-tech-validation/brief.md`，过程记录见同目录的 `notes.md`。

## 文件

| 文件 | 作用 |
|---|---|
| `common.ts` | 共用部分：按宿主的方式拉起适配器（自成会话和进程组）、安全闸门（`default` 模式加 haiku 才发 prompt）、带进程组和会话 ID 的进程树采样 |
| `q8.ts` | Q8：同一个适配器进程上两个会话同时各发 3 个 prompt，重复多轮；`--hold` 让一个会话的审批挂起 15 秒，看另一个会话是否受影响；进程树采样回答 SP-4b 遗留第 7 条；`--killgroup` 杀掉适配器的进程组，看哪些进程活下来 |
| `q9-probe/` | Q9：Rust 探针，用 `agent-client-protocol = "=3.3.0"` 作为 ACP 客户端连接真实的 claude-agent-acp，中间夹一层按行转发的线路分接，逐项确认扩展消息的收发。`tests/unhandled.rs` 用内存管道上的假 agent 演示 SDK 对"没有处理器的请求"的默认行为 |
| `config-hash.ts` | 运行前后对 `~/.claude/settings.json` 和 `~/.claude.json` 中与权限有关的字段做哈希对比，只输出字段名和"是否变化" |
| `load-rates.ts` | 从录制里算每条会话流的峰值速率，按宿主 40 ms 写入合并折算成线路帧，复核测量闸门的负载数字 |
| `sanitize-t003.ts` | 第二遍脱敏，接在 SP-1 的 `sanitize.ts` 之后：会话 ID 换成 `session-NN`；从 claude.ai 同步的命令和插件命令按用户 skill 处理；进程标签一律换成角色名；把结果文件还原成 JSON |
| `recordings/claude/` | Q8 的录制（脱敏后），SP-1 的录制格式 |
| `recordings/q9/` | Q9 探针在线路分接处记下的帧（脱敏后），同样的格式 |
| `recordings/results/` | 每次运行的结果：Q8 的逐轮数据、hold 和 killgroup 的数据、进程树采样；Q9 的逐项结论 |

ACP 客户端是 SP-1 的 `acp-client.ts`，原样复用。

## 运行

```sh
BASE=$(mktemp -d "${TMPDIR%/}/cupper-t003.XXXXXX")       # 每个用户自己的临时目录
for v in 0.91.0 0.93.0; do
  mkdir -p $BASE/adapters/claude-$v
  (cd $BASE/adapters/claude-$v && npm init -y && npm install --ignore-scripts @agentclientprotocol/claude-agent-acp@$v)
done

node config-hash.ts snapshot $BASE/private/config-before.json

node q8.ts --base $BASE --adapter 0.91.0 --rounds 3 --hold yes
node q8.ts --base $BASE --adapter 0.91.0 --rounds 0 --hold yes --killgroup yes --tag hold
node q8.ts --base $BASE --adapter 0.93.0 --rounds 1 --hold yes

(cd q9-probe && CARGO_TARGET_DIR=$BASE/target cargo build && CARGO_TARGET_DIR=$BASE/target cargo test)
# 测试仓库 $BASE/repos/q9 先用 git init 建好；两个版本各跑一次
for v in 0.91.0 0.93.0; do
  CLAUDE_CODE_EXECUTABLE=$(which claude) $BASE/target/debug/q9-probe \
    $BASE/adapters/claude-$v/node_modules/@agentclientprotocol/claude-agent-acp/dist/index.js \
    $BASE/repos/q9 $BASE/raw/q9/q9-probe-claude-$v.jsonl $BASE/raw/q9/q9-probe-claude-$v.findings.json
done

node config-hash.ts snapshot $BASE/private/config-after.json
node config-hash.ts compare $BASE/private/config-before.json $BASE/private/config-after.json --base $BASE
```

脱敏分两遍。结果文件先包成一行 JSONL（`{"seq":1,"t":0,"dir":"note","note":"results","data":<结果>}`）放进 `stage0/results/`，录制放进 `stage0/claude/`、`stage0/q9/`：

```sh
node ../SP-1-adapter-behavior/sanitize.ts --in $BASE/stage0 --out $BASE/stage1 --base $BASE --terms $BASE/private/terms.txt
node sanitize-t003.ts --in $BASE/stage1 --out recordings --terms $BASE/private/terms.txt
node load-rates.ts ../SP-1-adapter-behavior/recordings/{claude,codex-bundled,codex-local} recordings/claude
```

`terms.txt` 是本地词表（账号名、组织名、邮箱等），不入库。两遍脚本写完都会重新扫描输出，发现家目录、用户名、主机名、词表里的词、邮箱或会话 ID 就以非零状态退出。

## 安全约束在代码里的落实

- 会话的 `cwd` 一律是 `$BASE/repos/` 下新建的一次性 git 仓库，提交身份是占位的 `cupper-t003 <t003@example.invalid>`。
- 每个会话建好后都显式把权限模式设为 `default`，选 haiku 和最低推理强度。发 prompt 之前检查模式和模型，不满足就抛错，不发送。
- 审批只会选 `allow_once`（SP-1 客户端里不存在选择持久选项的路径；Q9 探针同样只找 `allow_once`）。
- 剧本固定，只让模型回显令牌、运行 `sleep`、写一个文件、数数，不让模型联网取内容。
- 传给适配器的环境变量只保留一个很小的白名单，外加 `CLAUDE_CODE_EXECUTABLE`。
- 不运行 Codex。

## 适配器的拉起方式

宿主用 `POSIX_SPAWN_SETSID` 拉起适配器（设计 5.6.1）。SP-1 客户端用的是 Node 的 `spawn`，没有这个选项，所以 `common.ts` 用一行 Perl 先调用 setsid(2) 再 exec node，进程号不变。实测适配器的 pid、pgid、sid 三者相同。
