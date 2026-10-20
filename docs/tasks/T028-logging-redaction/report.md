# T028 汇报

- 执行者：子 agent（Rust 工程师，宿主诊断）
- 分支：`feat/T028-logging-redaction`（在原 T004 worktree 里从 main 新开）
- 完成日期：2026-10-20

## 做了什么

T028 已完成，`cargo xtask ci pr` 全部通过。规模：新增 993 行（含测试约 330 行），删除 31 行，没有超过约 1,200 行的汇报线。这次每完成一个模块统计一次行数，逐模块明细在 `notes.md`。

新增 `cupper-host` 的 `diag` 模块，共 6 项：

1. **凭据规则只留一份**：
   - `driver/tap/redact.rs` 原样移到 `diag/redact.rs`，即结构化脱敏：账号字段、已知格式的凭据。
   - 原始帧分接改为调用它。
   - 原有用例随文件移动、名字不变、照常通过（UNIT-tap 在摘要里是 pass）。
2. **完整脱敏 `diag::full::Redactor`**：
   - 处理顺序：凭据 → 家目录写成 `~` → `/var/folders/…`（含 `/private` 前缀）写成 `<tmpdir>` → 用户名写成 `<user>` → 主机名写成 `<host>`（带域名和不带域名两种）→ 40 字符以上、字母数字混合的 base64 或十六进制串写成 `<redacted>`。
   - 字段名是 `id`，或以 `_id`、`hash`、`digest`、`blob` 结尾的，不受长串规则约束（其余规则照常）。
   - 主机名经新增的 `cupper_sys::host_name()` 读取（`kern.hostname`，与 `boot_session_uuid` 共用 sysctl 读取代码）。
3. **`host.log`（`diag::log`）**：
   - 自己实现的 tracing `Subscriber`，只处理事件，不记录 span。
   - 每个事件写一行 JSON：时间（取自 `Clock`，RFC 3339 UTC）、级别、模块、消息、字段；每个字段值按字段名做完整脱敏。
   - 按 10 MB × 5 轮转（`diag::rotate`，目录 0700、文件 0600）。
   - cupper 自己的模块从 `info` 记起，其他 crate（ACP SDK）从 `warn` 记起。
   - `set_verbose(bool)` 是给 T036 的开关接口：打开后降到 `debug`，`Content` 字段输出原文；关闭时 `Content` 只输出 `<种类, N bytes>`。
   - 开发构建把每行同时回显到启动时的 stderr。
   - panic hook 记录 panic 的消息和位置。
4. **适配器 stderr（`diag::stderr`）**：
   - `AdapterLog::open(目录, agent, clock, redactor)`，再对每个实例调用 `read(instance_id, 管道)`。
   - 每行加时间和实例 ID，完整脱敏后写进 `adapter-<agent>.log`（5 MB × 3）；超过 16 KB 的行切成多段。
   - 每个实例在内存里保留最后 64 KB，只保留整行、存脱敏后的文本，T055 可以直接放进 `error` 条目。
5. **启动顺序（`main`）**：
   - 读完启动载荷后初始化日志（第 2 步），然后**所有构建**才把 stdio 指向 `/dev/null`。发布构建原来一解析完参数就重定向；它本来就不读载荷，推迟到这里没有行为差别。
   - 之后的启动失败写 `host.log`；启动和退出各记一条。
   - 发布特性下的启动用例增加了断言：日志里有启动和退出这两条。
6. **`eprintln!` 的处理**：
   - 已改写：stdio 重定向之后的启动失败原因；会话 actor 的视图拒收（测试构建仍然 panic）。
   - 故障点触发：增加一条 `host.log` 记录，原来的 `eprintln!` 保留（原因见末尾第 6 条）。

## 关键取舍

- **日志目录按数据目录推出**（不新增注入点）：
  - 数据目录是 `<家目录>/Library/Application Support/<名字>` 时，日志写在 `<家目录>/Library/Logs/<名字>/`，即 `cupper`、`cupper-local`、`cupper-dev`，与 5.7.9 的数据目录同名区分。
  - 其他任何数据目录写在 `<数据目录>/logs/`。测试和 e2e 的数据目录本来就是每个用例的临时目录，所以不需要经 `test_config` 另传日志目录。
  - 用例结构上写不到真实的 `~/Library/Logs`。跑完全部用例和 CI 后我核对过，那里没有新建任何东西。
- **不引入 `tracing-subscriber` 和轮转的 crate**：按字段名放行 ID 需要拿到字段，时间也要取自 `Clock`。自己实现的订阅者约 60 行核心代码，只依赖 `tracing` =0.1.44（ACP SDK 本来就用这个版本），锁文件只多一行。
- **`info` 不记内容**：用 `Content` 类型实现。以后记录 prompt、正文、命令输出、文件内容，写成 `tracing::info!(prompt = %Content("prompt", text), …)`。verbose 开关是进程级全局值，测试里改它的用例共用一把锁。
- **开发构建判断**：按特性 `dev-auth` 且不带 `test-harness`。自用构建与开发构建的宿主特性相同，所以自用构建也会回显到它启动时的 stderr，无害。
- **长串规则不误伤路径**：`/` 多（超过长度的 1/16）时按路径看待，不替换，避免把 `/opt/homebrew/Cellar/node/24.1.0/...` 当成密钥。
- **panic hook 不用 `payload_as_str`**：它在 1.91 才稳定，而 MSRV 是 1.89，改用 `downcast_ref`。

## 验证方式

- [x] **UNIT-redaction**：
  - 5.11.2 表里每类规则都有样例，结构化和完整两档分别断言；
  - 结构化脱敏不改写 base64 图片和 64 位十六进制哈希；
  - 按字段名放行哈希和 ID，但其余规则照常；
  - 不误伤 `jdoe-example2` 这类前缀相同的名字；
  - 样例里的家目录、用户名、主机名都是虚构值。
- [x] **UNIT-redact-auth-status**：`_auth/status_update` 只留 `kind` 和套餐名；邮箱、组织、uuid、令牌、`_meta` 都去掉；API key 登录没有 account。
- [x] **轮转、64 KB 尾部、`info` 不记内容、panic hook 都有用例**：
  - 轮转：文件数、每个文件的大小上限、重新打开后接着计数、目录和文件权限；
  - 尾部：保留整行、只留最新的、长行切段；
  - 内容：`info` 只记种类和长度，verbose 打开后输出原文；
  - panic：记录消息和位置，并经过脱敏；
  - 另有 JSON 行格式、第三方 crate 的级别过滤、日志目录规则和日期格式的已知答案（含闰日和 2100 年）。
  - 这些用例算在 UNIT-redaction 名下：计划里 NFR-12（轮转、脱敏、收集适配器错误输出）对应的就是 UNIT-redaction。
- [x] 原有的原始帧脱敏用例（UNIT-tap）原样通过。
- [x] 没有睡眠；时间都经 `Clock`（用例用 `ManualClock`）。
- [x] 测试名以用例编号开头（`unit_redaction_*`、`unit_redact_auth_status_*`）。
- [x] 在干净的 worktree 里运行 `cargo xtask ci pr --record docs/tasks/T028-logging-redaction/ci/`，全部通过：
  - rust 1244 / protocol 185 / ts 314 / license 1 / testdata-scan 2；
  - ui、e2e 尚未实现；
  - 三个编号在摘要里都是 pass；摘要为 `pr-8250a81123e9.json`，已提交。
  - 宿主的四种特性组合下 clippy 都干净：全部特性、不带特性、只带 `dev-auth`、只带 `test-harness`。
- [x] **`LoginEnv.vars`（T094 的提醒）**：核对过，`driver/probe` 里没有任何日志调用，`Debug` 只打印变量名，没有路径会把这些值写进日志。
- [x] grep 过分支的 diff 和提交信息，没有本机路径或用户名。

## 提交列表

```
5cc1a83 docs(task): record the T028 pr CI summary
8250a81 docs(task): record T028 sizes and trade-offs
6465d00 feat(host): start host.log at step 2 and log what went to stderr
5dd3299 feat(host): read adapters' stderr into their own logs
91e33f9 feat(host): write host.log as redacted JSON lines
18d0727 feat(host): add a size-rotated log file
d82c16e feat(host): add the full redaction tier for logs
2adacc7 refactor(host): move the credential rules to diag::redact
df79b64 docs(task): estimate T028 by module before starting
```

## 遗留问题 / 范围外发现

1. **请决定是否写进设计**：
   - "其他数据目录的日志写在 `<数据目录>/logs/`"这条规则（5.11.1）；
   - 测试和 e2e 因此不需要新的 `test_config` 注入点（5.9.9）。

   如果仍想要显式的注入点，加一个字段即可。
2. **审计记录还没有写进 `host.log`**：设计 5.8.6 和 `gateway/audit.rs` 的注释都写着审计记录进 `audit` 表和 `host.log`，但现在审计通道的接收端在 `Host::start` 里直接丢弃。建议随 T059 一起接。
3. **T055 要注意**：
   - 每种 agent 开一个 `AdapterLog`，每个实例调用一次 `read`；
   - 实例 ID 作为行前缀，不受长串规则约束。
4. **以后加日志的任务要注意**：
   - 宿主自己的 ID 和哈希要作为字段记录（字段名带 `id`、`hash` 等），不要拼进消息正文，否则 40 字符以上的会被当成长串替换；
   - 内容一律经 `Content`。
5. **主机名脱敏的覆盖范围**：只覆盖 `kern.hostname`（含 `.local` 和短名）。系统设置里的"电脑名称"（ComputerName，可能带空格和撇号）没有覆盖，它出现在日志里的可能性很小。需要的话可以在 `cupper-sys` 里补读 SystemConfiguration。
6. **保留的 `eprintln!`**：
   - `main` 里日志之前的失败原因：这时 stderr 还连着，启动用例也断言这些文字；
   - `--build-info` 的输出：属于启动契约；
   - `cupper-gateway-fixture` 测试夹具；
   - 故障点触发：进程内的故障点用例断言 stderr 里有这句话，这条 `eprintln!` 和新增的 `host.log` 记录并存。
