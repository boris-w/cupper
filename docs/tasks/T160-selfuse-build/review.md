# T160 评审

- 评审人：编排者
- 日期：2026-11-03
- 结论：**通过，合并**（PR #40）

## 核对

- **规模**：+812 −67，第 3 轮放宽后的停止线 850 以内（原为 800）。
- **CI**：PR #40 https://github.com/boris-w/cupper/pull/40 ，48961e0 上 8 个检查全绿；5a490a3 上也全绿（run 41606577120）。中间一次 rust 作业撞上网关同步的偶发失败（T227），记录在案后只重跑了失败的作业。
- **runner 上的自用构建**（nightly，job=selfuse）：
  - 第 1 轮（41581457853）：构建成功，10 过 3 败。查出 sourcemap 带着 e2e 代码的源码；GPU 进程跑在普通 helper 里；被换包的旧 App 退出卡住。
  - 第 2 轮（41586879127）、第 3 轮（41589556442）：前两处修好；卡住查明是产品缺陷，开 T225 修，范围随后扩大。
  - 第 4 轮（41604114489，main 合进分支，含 T225）：只剩用例自己的等待条件不对。
  - 第 5 轮（41606572821）：三个用例 7 例全过。
- **个人信息**：改动的文件过了敏感词清单，干净；用户文档没有个人信息。本机没有构建、安装或启动过 Cupper Local。

## 逐项意见

1. **配置**：发布配置和自用配置各写完整，不用 `extends`（数组会拼接）；自用构建 ad-hoc 签名并开 hardened runtime，entitlements 只多 `disable-library-validation`，fuses 同发布、只关 cookie 加密，与 5.7.9 一致。IT-selfuse-build 逐个 Mach-O 核对。
2. **runner 上的五轮是这个任务最有价值的部分**：用例第一次在真实的自用构建上跑，查出两处测试问题，以及一个影响所有构建的产品缺陷（T225：宿主不可用时 ⌘Q 退不出）。执行者每次都先给证据再修，产品问题都先报上来，没有自己动手。
3. **防护**：用例和 `selfuse` 作业只在 GitHub 托管的 runner 上运行，本机一律跳过，因为作者机器上的 `cupper-local` 是日常在用的数据。
4. **编排者的合并**：T225 合并后，我把 main 合进了 T160 的分支（7a4f9ad）。锁文件和 `settings.rs`、`supervisor/mod.rs` 是自动合并的，`pnpm install --frozen-lockfile` 和 clippy 都过了。

## 记下的点

- **设计**（本次合并时改）：01 的 5.1.5 加 `pnpm build:local`、`test:selfuse`，CI 表加 `selfuse` 作业；07 的 Fuses 一行改正 `asarUnpack` 的矛盾、写明第 9 根 fuse 保持出厂；5.7.9 补自用构建落地的事实（Vite mode、不带 source map、helper、worktree 根目录、用户文档的位置）。
- **交接事项**（已写入 handoff-notes）：
  - T161、T180：Info.plist 里 Electron 默认的摄像头、麦克风、蓝牙、音频采集用途说明要不要删；
  - T161：复用 `selfuse/build.test.ts` 的 Mach-O 检查；`electron-builder.release.yml` 还没真正构建过；
  - 5.5.7：keyed 构建里，宿主因待升级退出后，旧界面会拉起磁盘上的新宿主，新旧混用；
  - T147：设置页"安装命令行工具"还没做，安装说明暂时用 `ln -s`；
  - 10 的 5.10.2：库不带 entitlements、helper 的 identifier 等事实，T161 核对时一并补。
- **P-4** 的缺口已由 T223 补上（已合并）。用户现在可以按 `docs/user/selfuse-install.md` 安装 Cupper Local。
