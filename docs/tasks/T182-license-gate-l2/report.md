# T182 汇报

- 执行者：子 agent（前端与 e2e、CI）
- 分支：`ci/T182-license-gate-l2`
- 完成日期：2026-11-16

## 做了什么

许可证门禁第二层和 THIRD_PARTY_NOTICES.txt 都已做完。PR #120 的 8 个检查全部通过，分支上的 nightly `release-config` 也通过了，扫描结果是没有问题。

- **指纹清单**
  - 新命令 `license-gate fingerprints`：按 claude-agent-acp 锁文件钉住的 resolved 和 integrity，把 SDK 的 tarball 下载到临时目录，核对后对每个 ≥ 512 字节的文件算 SRI，然后删除临时目录，只把哈希写进 `tools/license-gate/fingerprints.json`。这个命令只在 runner 上跑过。
  - 特征串、签名 identifier 和 Team ID 写在 `fingerprints.ts` 里，每一项都注明了理由：
    - 特征串三个：`@anthropic-ai/claude-agent-sdk`、`Anthropic PBC. All rights reserved`、`Claude Code process exited with code`。
    - identifier `com.anthropic.claude-code`、Team ID `Q6L2SF6YDW`：用 `codesign -dv` 从本机已安装的 Claude Code 2.1.296 读出，只读签名元数据，没有下载任何东西。
  - 下载和校验的代码从 `codex-notices` 抽成共用的 `fetchPackage`。
  - `check` 新增一项：清单里的版本和 integrity 必须与锁文件一致。SDK 升级后，PR 上就会提醒重新生成清单。
- **扫描**（`license-gate scan`，`scan.ts`）
  - 对象是 `.app`，或含 `.app` 的 zip（用 unzip 解开）。
  - 逐个文件检查：
    - SDK 的文件哈希，以及锁文件里所有 `@anthropic-ai/*` 包的 tarball integrity；
    - 特征串，按内容搜索，app.asar 也整体搜；
    - Mach-O（单架构和 universal 都支持），解析 `LC_CODE_SIGNATURE` 里的 CodeDirectory，取 identifier 和 team 比对；签名读不出时也报出。
  - 宿主带的锁文件放行有两个条件：位于 `Helpers/CupperHost.app/Contents/Resources/runtime/locks/` 下，且 sha-256 与运行时清单 `files` 里记的一致。
  - 每个 `.app` 的 `Contents/Resources/THIRD_PARTY_NOTICES.txt` 必须存在，并与按当前代码重新生成的结果逐字节相同。
- **声明生成**（`license-gate notices`，`third-party.ts`）
  - 分节内容：
    - Node、codex-acp（含内联依赖的声明）、esbuild-wasm：取自 `runtime/licenses/`；
    - Electron：它的 LICENSE，并注明 Chromium 的声明见同目录的 `LICENSES.chromium.html`；
    - 前端 npm 包：各 JS 入口打包进去的包取并集，用包内的许可证文件；
    - Rust 依赖：用 cargo-about 0.9.3 对 cupper-host、cupper-cli、cupper-desktop-native 各生成一次 JSON，再按许可证文本合并。
  - cargo-about 的配置在运行时从 `deny.toml` 的允许清单生成，目标只有 `aarch64-apple-darwin`，不含 build、dev 依赖和我们自己的私有 crate。运行方式是先 `cargo fetch --locked`，再用 `--frozen` 离线跑，保证结果确定。
  - 有 2 个 npm 包没有许可证文件，按 ADR-11 从上游仓库补全文本，放在 `tools/license-gate/licenses/`：`react-remove-scroll-bar`、`lru_map`。既没有文件也没有补全文本的包会让生成失败。
- **接入**
  - 发布构建：`build:release` 在 `vite build` 之后生成声明，`electron-builder.release.yml` 用 `extraResources` 把它放进 `Contents/Resources`。
  - `release-config`：新增 `license-gate-l2` 步骤，扫描构建出的 `.app`。
  - PR 上的 `license`：新增 `notices` 步骤。
  - `action.yml`：给这两个作业装 cargo-about，命令是 `cargo install --locked --features cli cargo-about@0.9.3`（不加 `cli` 不会装出命令）。
  - `.prettierignore`：加入 `fingerprints.json`，它由命令写出。

## 关键取舍

- **指纹清单的来源**：用 TS 命令、在 runner 上生成（你批的方案）。备选是放进 runtime-pack 的隔离目录，多 50 行 Rust。没有选它。
- **特征串**：
  - 环境变量名不能用，宿主内嵌的运行时清单里列着 SDK 读的环境变量；`@anthropic-ai/claude-code` 也不能用，宿主在用户机器上找 Claude Code 时用到它。用了会把我们自己的宿主报出来。
  - 生成命令要求每个串都在 SDK 文件里出现，且至少一个在它的 JS 代码里。打包脚本不压缩代码，所以 SDK 被打进我们的 JS 也能发现。
  - 三个候选在 spike-run 43809511708 里都命中了，而且都出现在 sdk.mjs 或 core.mjs 里。
- **签名判定**：按 identifier 或 Team ID 任一命中就报。用 Team ID，凡是 Anthropic 签名的二进制都能认出，包括 SDK 平台包里的 Claude Code；那个平台包不下载，所以没有核对它的 identifier。签名解析用 TS 自己写，因为单元测试跑在 ubuntu 上，那里没有 codesign。
- **声明核对用逐字节比对**：扫描时按当前代码重新生成，与产物里的比。"齐全"由生成器保证，单元测试覆盖。这样能同时发现产物里缺声明和声明过期。
- **范围**：按你的决定，release-config 不改，只扫 `.app`；zip 由扫描支持、由单元测试覆盖；DMG 留给 T181。

## 验证方式

- **验收标准**
  - **两个用例通过，负面样例全部报出**：UNIT-license-gate-l2 共 6 条，UNIT-third-party-notices 共 2 条，本机和 PR 的 `ts` 作业都通过。
    - L2 的负面样例：
      - 含特征串的文件，app.asar 也在内；
      - 哈希命中 SDK 文件的文件；
      - 哈希命中 SDK tarball 的文件；
      - 伪造 identifier 的 Mach-O、team 命中的 Mach-O、universal 二进制里的伪造切片、签名读不出的 Mach-O；
      - 锁文件改动一个字节后不再放行，挪到别的位置也不再放行；
      - zip 里的 SDK 文件；
      - 声明缺失或与当前代码不一致。
    - 干净的 app 通过。
    - 声明用例：各节齐全，同一许可证文本只出现一次，输出与输入顺序无关；缺许可证文本时生成失败，补全文本生效。
  - **分支上的 nightly `release-config` 通过**：https://github.com/boris-w/cupper/actions/runs/43810008408 ，结果是 10 passed、0 failed。
    - `license-gate-l2 ok (32.6s)`；原始日志里，对构建出的 `Cupper.app` 的扫描输出 `license-gate: no problem`。这说明产物里有 THIRD_PARTY_NOTICES.txt，且与重新生成的一致；没有命中 SDK 或 Claude Code；宿主带的锁文件按哈希放行了。
  - **新增依赖的许可证**：cargo-about 0.9.3 是 MIT OR Apache-2.0，在 ADR-11 的允许清单里，而且只是构建工具，不进产物。
  - **PR 上 8 个检查全部通过**：PR #120 https://github.com/boris-w/cupper/pull/120 ，运行 https://github.com/boris-w/cupper/actions/runs/43810009677 。`license` 里的 `notices ok (34.0s)`。
- **本机核对**（只做冒烟，没有测量）
  - 签名解析读出的值与 `codesign -dv` 一致：
    - 本机 Claude Code：`com.anthropic.claude-code`、`Q6L2SF6YDW`；
    - `/usr/bin/true` 是 universal 二进制，两个切片都能读出；
    - 用 `codesign -s - -i com.anthropic.claude-code` 重签的副本，读出了伪造的 identifier；
    - e2e 包里的 Electron Framework、Node 也能正常读出。
  - 用本机 e2e 包冒烟扫描，没有误报。
  - eslint、tsc、prettier、xtask clippy、trace-ids 都通过。
- **规模**：代码约 848 行，硬线 850。`cargo xtask size` 显示 +911，减去补全的许可证文本 42 行、指纹清单约 21 行。开工前估约 570 行，严重低估；也没有按守则第 8 条每写完一个模块就统计一次，你已记入评审。以后每个模块写完就跑 `cargo xtask size`，并按模块分提交。

## 提交列表

```
6e3960b3 chore(license-gate): the SDK's fingerprints, written on a runner
fbba12e5 fix(license-gate): fetch the crates before cargo-about reads them offline
716c1ed0 feat(license-gate): second layer scan and third-party notices
32dd0127 docs(task): T182 estimate before the work
```

## 遗留问题 / 范围外发现

- **交给 T181**：
  - 扫描 DMG：挂载后走同一套扫描。
  - 发布流水线对 zip 运行 `license-gate scan`；zip 支持已有，单元测试覆盖。
  - 发布流水线要装 cargo-about，`build:release` 依赖它。
- **SDK 升级时**：claude-agent-acp 升级、锁文件里的 SDK 变了以后，PR 上的 `check` 会失败。维护者要在 runner 上用 spike-run 重新运行 `license-gate fingerprints`，本次用的参数是 `dir=spikes/T103-sp6-render`、command `cd ../.. && node tools/license-gate/src/main.ts fingerprints ...`。命令也会核对特征串是否仍然命中。
- **补全的许可证文本**：取自上游默认分支，不是对应版本的 tag。这两个包以后升级、自带了 LICENSE 时，补全文本就不再被使用，可以删掉。
- **设计文档没有改**：`docs/design/01-repo-build-ci.md` 中 `license` 和 `release-config` 的步骤说明，以及 5.10.7 里第二层与 THIRD_PARTY_NOTICES 已经有了落点，是否要补进文档，由你决定。
- **缓存**：本机为了试 cargo-about，装在了 `/tmp` 下的临时目录，不在仓库里。runner 上 cargo-about 每次重新编译，约 1.5 分钟；如果以后要省这段时间，可以考虑缓存。
