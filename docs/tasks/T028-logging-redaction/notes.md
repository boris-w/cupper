# T028 工作笔记

- 执行者：子 agent（Rust 工程师，宿主诊断）

## 开工前的规模估计

行数指 rustfmt 展开后的行，含文档注释和空行，不含 `Cargo.lock`。新模块是 `cupper-host` 的 `diag`（设计 5.2 模块表里的名字）。

**实现**

| 模块 | 内容 | 草估 |
|---|---|---|
| `diag/redact.rs` | T050 的结构化脱敏（账号字段、凭据）原样从 `driver/tap/redact.rs` 移来，分接改为调用它 | 10（只算改动） |
| `diag/full.rs` | 完整脱敏：凭据、家目录、用户名、`/var/folders/…`、主机名、40 字符以上的长串；按字段名放行宿主自己的哈希和 ID | 110 |
| `diag/rotate.rs` | 按大小轮转的文件（`host.log` 10 MB × 5，适配器日志 5 MB × 3），目录 0700、文件 0600 | 60 |
| `diag/log.rs` | 自己实现的 tracing `Subscriber`：JSON Lines（时间取自 `Clock`）、级别与 `diagnostics.verbose` 开关、第三方 crate 只记 `warn` 以上、`Content` 字段在 `info` 只记长度和类型、开发构建同时输出到终端、日志目录规则、panic hook | 170 |
| `diag/stderr.rs` | 适配器 stderr 读取器：逐行加实例 ID 和时间、完整脱敏、写进 `adapter-<agent>.log`，每实例保留最后 64 KB | 80 |
| `cupper-sys` 的 `host_name` | 主机名脱敏要用；`unsafe` 只在 sys | 15 |
| `main.rs`、会话 actor、故障点、分接、`Cargo.toml` | 启动顺序里接上日志（第 2 步）；把 `eprintln!` 改写进 `host.log` | 40 |
| **实现小计** | | **485**，按计划 1.1 的经验乘 1.7 约 **825**（按我前几个任务的实际偏差 1.1–1.4 倍计约 630） |

**测试**

| 用例编号 | 内容 | 草估 |
|---|---|---|
| UNIT-redaction | 5.11.2 表里每类规则一个样例，结构化、完整两档分别断言；长串不误伤按字段名放行的哈希和 ID；结构化脱敏不改写 base64 图片和哈希 | 90 |
| UNIT-redact-auth-status | `_auth/status_update` 只保留 `kind` 和套餐名（含多种登录方式、多余字段） | 30 |
| 日志其余 | 轮转（文件数、大小上限）；64 KB 尾部；`info` 不记内容、打开 verbose 后记录；panic hook；JSON 行的字段；日志目录规则 | 185 |
| **测试小计** | | **305** |

**合计约 1,130 行**（实现按 1.7 倍计）或约 935 行（按我的实际偏差计），在约 1,200 的汇报线以内，但贴近，所以这次每完成一个模块就用 `git diff --numstat` 统计一次，推算过线就停下汇报。拆分点（如需要）：日志与轮转、panic hook / 脱敏与 stderr 读取。

## 依赖的取舍（开工前）

- **不引入 `tracing-subscriber`**：完整脱敏要按字段名放行哈希和 ID，用它的 JSON 格式化器就得把写出的行再解析一遍；时间也要取自 `Clock`。自己实现一个只处理事件的 `Subscriber`（不记录 span）约 60 行，只依赖 `tracing` 本身。`tracing` 0.1.44 已经在锁文件里（ACP SDK 依赖它），锁精确版本。
- **不引入轮转的 crate**：按大小轮转约 60 行。

## 实际行数（逐模块，每完成一个统计一次）

`git diff -M --numstat`，不含 `Cargo.lock` 和本目录。

| 完成的模块（提交） | 新增 / 删除 | 其中测试 | 估计（实现 + 测试） | 累计新增 |
|---|---|---|---|---|
| 凭据规则挪进 `diag::redact`（`refactor(host)`） | 15 / 10 | — | 10 | 15 |
| 完整脱敏、主机名（`feat(host)`） | 307 / 6 | 116（含 UNIT-redact-auth-status 27） | 125 + 120 | 322 |
| 按大小轮转（`feat(host)`） | 108 | 35 | 60 + 35 | 430 |
| `host.log` 订阅者、`Content`、panic hook、日志目录（`feat(host)`） | 332 | 99 | 170 + 70 | 762 |
| 适配器 stderr 读取器（`feat(host)`） | 196 | 72 | 80 + 50 | 958 |
| `main` 接线、`eprintln!` 改写、启动用例的断言（`feat(host)`） | 34 / 15 | 6 | 40 | 993 |

**合计新增 993 行、删除 31 行**，其中测试约 330 行。按我自己的偏差推算的估计是约 935，实际多 6%；按计划 1.1 的 1.7 倍推算的估计是约 1,130，实际在它之下。每一步推算出的总数都没有超过约 1,200。

## 实现时的取舍

**日志目录**：
- 数据目录是 `<家目录>/Library/Application Support/<名字>` 时，日志写在 `<家目录>/Library/Logs/<名字>/`：发布构建是 `cupper`，自用构建是 `cupper-local`，开发构建是 `cupper-dev`，与 5.7.9 的数据目录同名区分。
- 其他任何数据目录（测试、e2e 的临时目录，或手工指定的目录）写在 `<数据目录>/logs/`。
- 这样不用新增 `test_config` 的注入点，用例也写不到真实的 `~/Library/Logs`：所有用例的数据目录都在临时目录里。全部用例跑完后核对过，真实的 `~/Library/Logs` 下没有新建任何东西。
- 目录 0700、文件 0600。任务书提到"经 `--test-config` 或启动载荷传入"，现在的规则让它不再需要；如果编排者仍想要显式的注入点，加一个字段即可。

**启动顺序**：
- 日志在读完启动载荷之后初始化（第 2 步），然后**所有构建**都把 stdio 指向 `/dev/null`。原来发布构建一解析完参数就重定向；发布构建本来就不读载荷，所以推迟到这里没有行为差别。
- 在这之前的失败（参数、`launcher`、tokio 运行时、载荷）仍然写 stderr：这时 stderr 还连着，启动用例也断言这些文字。之后的失败写 `host.log`。
- 日志初始化失败时只在 stderr 提示一句，不阻止宿主启动。

**自己实现 tracing 的 `Subscriber`**：
- 只处理事件，不记录 span，只依赖 `tracing` 0.1.44（锁文件里本来就有，锁成精确版本）。
- 每行是 `{"time","level","module","message","fields"}`。时间取自注入的 `Clock`，格式化成 RFC 3339 UTC，用 Hinnant 的日期算法，没有引入日期库。
- 每个字段值按字段名做完整脱敏。
- cupper 自己的模块从 `info` 记起；其他 crate（ACP SDK）从 `warn` 记起，免得 SDK 在 `info` 记下消息内容。

**`diagnostics.verbose` 的接口**：
- `diag::log::set_verbose(bool)`，留给 T036 接设置项。
- 打开后两类来源都降到 `debug`，`Content` 字段输出原文；关闭时只输出 `<种类, N bytes>`。
- 写法是 `tracing::info!(prompt = %Content("prompt", text), ...)`。以后记录 prompt、正文、命令输出、文件内容，一律经 `Content`。
- 开关是进程级的全局值。改动开关的用例在测试里共用一把锁，避免并行时互相干扰。

**开发构建同时输出到终端**：
- 宿主按特性判断：`dev-auth` 且不带 `test-harness`。
- 自用构建和开发构建的宿主特性相同，所以自用构建也会回显：写到它启动时的 stderr，那是启动器给的，无害。
- 回显写的是初始化时复制的 stderr（`try_clone_to_owned`，安全 API），所以 stdio 指向 `/dev/null` 之后仍然能写到终端。

**完整脱敏的细节**：
- 处理顺序：先凭据（共用 `diag::redact` 的规则），再家目录、`/var/folders/…`（含 `/private` 前缀，吃掉后面三段）、用户名（取家目录的最后一段，前后要是词边界）、主机名（`kern.hostname`，带域名和不带域名的两种都替换），最后是长串。
- 长串：40 个字符以上、同时有字母和数字、由 base64 或十六进制字符组成。`/` 的个数超过长度的 1/16 时按路径看待，不替换，避免把 `/opt/homebrew/Cellar/...` 这类路径当成密钥。
- 字段名是 `id`，或以 `_id`、`hash`、`digest`、`blob` 结尾的，不受长串规则约束（其余规则照常）。宿主自己的 ID 和哈希因此要作为字段记录，不要拼进消息正文。

**适配器 stderr**：
- 超过 16 KB 的行切成多段。
- 尾部缓冲只保留整行，存的是脱敏后的文本，不带前缀，T055 可以直接放进 `error` 条目。
- 接口：`AdapterLog::open(目录, "claude", clock, redactor)`，然后对每个实例调用 `log.read(instance_id, 管道)`，得到 `Tail` 和读取任务。

**panic hook**：
- 记录 panic 的消息和位置（编译时的相对路径加行号），然后交给原来的 hook。
- MSRV 是 1.89，`PanicHookInfo::payload_as_str` 要 1.91，所以改用 `downcast_ref`。

**`eprintln!` 的去向**：
- 已改写：`main` 里 stdio 重定向之后的失败原因；会话 actor 的视图拒收（测试构建仍然 panic）。
- 故障点触发时增加一条 `host.log` 记录，原来的 `eprintln!` 保留：故障点的进程内用例断言 stderr 里有这句话，那时没有日志。
- 保留的：`main` 里日志之前的失败原因（`fail`）、`--build-info` 的 `println!`（输出契约）、`cupper-gateway-fixture` 测试夹具的 stderr、故障点用例里的 `println!`。

**`LoginEnv.vars`（T094 的提醒）**：核对过，`driver/probe` 里没有任何日志调用，`LoginEnv` 的 `Debug` 只打印变量名，没有路径会把这些值写进日志。

**没做的（范围外）**：连接审计记录写进 `host.log`（设计 5.8.6、`gateway/audit.rs` 的注释）。现在审计通道的接收端在 `Host::start` 里直接丢弃，接入归 T059。
