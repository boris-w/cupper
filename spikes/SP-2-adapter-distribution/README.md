# SP-2 适配器分发与版本耦合实测脚本

只用于 SP-2 的验证，不是产品代码，后续开发不要依赖。结果和结论见 `docs/tasks/SP-2-adapter-distribution/`。

所有脚本只用 Node 内置模块，不需要 `npm install`。外部工具（esbuild、esbuild-wasm、postject、bun、官方 Node）都按下面的步骤装到临时目录。下文用 `$TMP` 表示一个 worktree 之外的临时目录。

## 隔离约定

- npm 一律带上 `NPM_CONFIG_USERCONFIG=$TMP/empty-npmrc`、`NPM_CONFIG_GLOBALCONFIG=$TMP/empty-globalrc`、`NPM_CONFIG_CACHE=$TMP/npm-cache`，不读 `~/.npmrc`（里面可能有令牌），也不写 `~/.npm`。安装时加 `--ignore-scripts`，不运行任何包脚本。
- 验证 GPG 签名时用 `gpgv --keyring <下载的公钥环>`，或者把 `GNUPGHOME` 设为临时目录，不碰 `~/.gnupg`。
- bun 构建时设置 `BUN_INSTALL`、`BUN_INSTALL_CACHE_DIR`（指向临时目录）和 `DO_NOT_TRACK=1`（见 `build-bun.sh`）。
- 被测的适配器进程用 `acp-probe.mjs --clean-env 1` 启动，环境变量只保留 HOME、USER、LOGNAME、TMPDIR、LANG 和 `PATH=/usr/bin:/bin:/usr/sbin:/sbin`，模拟由 launchd 拉起的 GUI App。
- 测试旧版 Claude Code 时额外设置 `CLAUDE_CONFIG_DIR=$TMP/...` 和 `DISABLE_AUTOUPDATER=1`，不让旧版本写用户的 `~/.claude`。

## 脚本

| 文件 | 用途 | 对应 brief |
|---|---|---|
| `acp-probe.mjs` | 最小 ACP v1 客户端：initialize → session/new →（可选）设置模型、模式、推理强度 → 一个 prompt；每一帧都写进 JSONL。`--clobber <path>` 在 session/new 之后把适配器的文件挪走或替换，用来模拟升级 | 1、2、3、4 |
| `hashfile.mjs` | 输出文件的大小、sha256 和 sha512 SRI | 2、5 |
| `codex-schema-drift.mjs` | 对多个 codex 二进制运行 `app-server generate-json-schema`（分别带和不带 `--experimental`），只比较 codex-acp 实际引用到的方法，看它们是否存在、是 stable 还是 experimental、参数 schema 有没有变 | 2 |
| `appserver-poke.mjs` | 直接向 `codex app-server` 发原始 JSON-RPC，记录"方法不存在"和"缺少 experimentalApi 能力"时的错误形态 | 2 |
| `bundle.mjs` | 用 esbuild 把适配器打成单个 ESM 或 CJS 文件，并检查产物中是否还有 Node 内置模块以外的 import/require | 4 |
| `sea-entry-claude.mjs` | claude-agent-acp 的 SEA 入口。Node 24 的 SEA 只能运行 CJS，而上游入口用了顶层 await，所以把它改写进 async 函数 | 3 |
| `build-sea.sh` | 构建 Node SEA：生成 blob → postject 注入 → 用 hardened runtime 重签 | 3 |
| `build-bun.sh` | `bun build --compile`，然后用 hardened runtime 重签 | 3 |
| `sign-matrix.sh` | 用 E0（无 entitlement）到 E3 几组 entitlement 加 hardened runtime 重签运行时，然后检查 JIT、`--jitless`、WebAssembly 和适配器能否启动 | 3 |
| `bench-startup.mjs` | 启动耗时：`--version` 从拉起到退出，以及从拉起到收到 ACP `initialize` 响应。不建会话，不调用模型 | 3 |
| `sizes.sh` | 统计原始大小、gzip -9 和 xz -9 后的大小 | 3 |
| `install-runtime.mjs` | 首次运行安装器原型：按清单下载，校验 sha256 或 SRI，按锁文件安装 npm 树（不跑脚本），在本机用 esbuild-wasm 打包并核对固定的产物哈希，写入 `.installed.json` 后原子改名；附 `verify` 子命令和故障注入 | 5 |
| `licenses.mjs` | 清点 node_modules 树的许可证 | 6 |

## 复现步骤（摘要）

```sh
# 1. 适配器（不带可选依赖），在 $TMP/work/<name> 下
npm install --omit=optional --ignore-scripts   # package.json 中固定版本 claude-agent-acp@0.91.0 / codex-acp@2.2.2

# 2. 本机 CLI 实测，一个回合
node acp-probe.mjs --log $TMP/logs/x.jsonl --cwd $TMP/testrepo --clean-env 1 \
  --model haiku --prompt "Reply with exactly one word: pong." \
  --set CLAUDE_CODE_EXECUTABLE=/path/to/claude -- node $TMP/work/claude/node_modules/@agentclientprotocol/claude-agent-acp/dist/index.js
node acp-probe.mjs ... --mode '^read-only$' --effort '^low$' \
  --set CODEX_PATH=/path/to/native/codex -- node .../codex-acp/dist/index.js

# 3. 多个 codex 版本的协议差异
node codex-schema-drift.mjs --adapter .../codex-acp/dist/index.js --out $TMP/schema --baseline v0160 \
  v0159=/path/codex-0.159.2 v0160=/path/codex-0.160.1 v0163=/path/codex-0.163.0

# 4. 单文件、SEA、bun
node bundle.mjs --esbuild $TMP/tools/esbuild/node_modules/.bin/esbuild --entry .../claude-agent-acp/dist/index.js --out $TMP/out/claude.mjs
node bundle.mjs --esbuild ... --format cjs --entry sea-entry-claude.mjs --alias ADAPTER_DIST=.../claude-agent-acp/dist --out $TMP/sea/claude.cjs
bash build-sea.sh $TMP/node-v24.22.0-darwin-arm64/bin/node $TMP/sea/claude.cjs $TMP/sea/claude-sea $TMP/tools/postject/node_modules/.bin/postject E1.plist
bash build-bun.sh $TMP/bun $ENTRY $TMP/bunc/claude-bun E1.plist $TMP/bunc/scratch
bash sign-matrix.sh <runtime binary> $TMP/sign/x [adapter.mjs]      # KIND=exe 用于 SEA 和 bun 产物

# 5. 首次运行安装
node install-runtime.mjs install docs/tasks/SP-2-adapter-distribution/runtime-manifest.json $TMP/runtime \
  --esbuild-wasm $TMP/tools/esbuild-wasm/node_modules/esbuild-wasm/lib/main.js [--fault bad-hash|bad-url|crash-before-commit]
node install-runtime.mjs verify docs/tasks/SP-2-adapter-distribution/runtime-manifest.json $TMP/runtime

# 6. 许可证
node licenses.mjs $TMP/work/claude-acp-full --lock $TMP/work/claude-acp-full/package-lock.json
```

签名一律是 ad-hoc（`-`）。用真实证书时设置 `SIGN_ID=<证书哈希>`。证书名称、哈希和 Team ID 都不要写进仓库。
