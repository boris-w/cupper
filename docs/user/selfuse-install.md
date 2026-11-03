# Cupper Local 的安装与手动更新

- 适用于：Cupper Local，作者在自己的 Mac 上构建、日常使用的"自用构建"（设计 5.7.9）。用之前先读 [selfuse-notice.md](selfuse-notice.md)。
- Cupper Local 没有自动更新：每次更新都是自己构建、自己替换。它有一套自己的身份和目录，和开发构建（cupper Dev）可以同时运行，互不相通：

  | | 位置 |
  |---|---|
  | App | `/Applications/Cupper Local.app`（bundle id `dev.cupper.app.local`，URL scheme `cupper-local://`） |
  | 数据 | `~/Library/Application Support/cupper-local/`（界面的数据在其下的 `ui/`） |
  | 日志 | `~/Library/Logs/cupper-local/` |
  | worktree | `~/.cupper-local/worktrees/`（可以在设置里改） |

## 构建

需要：macOS 15 或更新、Apple 芯片；Xcode 命令行工具（`xcode-select --install`）；rustup（版本按仓库的 `rust-toolchain.toml` 自动安装）；Node 24（版本见 `.node-version`）和 pnpm（版本见根目录 `package.json` 的 `packageManager`）。

在仓库根目录：

```sh
pnpm i
cargo xtask runtime-pack   # 第一次构建前，以及 runtime/manifest.json 变了之后
pnpm build:local
```

- `runtime-pack` 从公开地址下载 Node、codex-acp、esbuild-wasm，按 `runtime/manifest.json` 核对哈希后重签 Node，放进 `target/runtime-pack/`。
- `build:local` 依次构建原生扩展、宿主和命令行（先签宿主，再把宿主的 cdhash 编进命令行）、界面，最后由 electron-builder 打包并做 ad-hoc 签名。
- 产物是 `apps/desktop/dist/pack/local/mac-arm64/Cupper Local.app`。

## 第一次安装

```sh
ditto "apps/desktop/dist/pack/local/mac-arm64/Cupper Local.app" "/Applications/Cupper Local.app"
open "/Applications/Cupper Local.app"
```

第一次打开时：

- **没有"无法验证开发者"的提示。** 本机构建的 App 不带隔离标记，Gatekeeper 不拦。反过来，不要经 AirDrop、网盘或下载把它传到别的 Mac：带上隔离标记后，ad-hoc 签名的 App 会被拦下。
- **没有钥匙串密码框。** Cupper Local 关掉了 cookie 加密，原因见须知第 6 节。
- **工具栏上有"弱认证"标记。** 这是自用构建的正常状态，见须知第 1 节。
- **添加项目时可能问文件夹访问。** 项目在"文稿""桌面""下载"、iCloud 云盘或外接磁盘里时，macOS 会问"Cupper Local 想访问……中的文件"。这是项目所在的文件夹，就允许。
- **Cupper Local 自己不会申请别的权限。** 之后出现的辅助功能、自动化、屏幕录制、摄像头、麦克风提示都来自 agent 运行的东西，一律拒绝，见须知第 2 节。
- 系统通知还没做；以后加上时，会多问一次是否允许通知。

## 命令行（可选）

App 里带着 Cupper Local 自己的命令行，它默认连接 Cupper Local 的宿主。放进 PATH 里的某个目录，例如：

```sh
ln -s "/Applications/Cupper Local.app/Contents/Helpers/cupper" ~/.local/bin/cupper-local
cupper-local status
```

命令行只读；写操作在 `cupper-local repl` 里先 `elevate`，再到 Cupper Local 弹出的确认框里批准。只有宿主在运行（也就是 Cupper Local 开着）时命令行才连得上。

## 手动更新

1. 拉取新代码，按上面的步骤重新构建。
2. **退出 Cupper Local**（⌘Q）。有运行中的回合或待处理的请求时，它会先问你；确认后这些回合被中断。宿主随界面一起退出。
3. **先删掉旧的，再复制新的：**

   ```sh
   rm -rf "/Applications/Cupper Local.app"
   ditto "apps/desktop/dist/pack/local/mac-arm64/Cupper Local.app" "/Applications/Cupper Local.app"
   ```

   不要把新版本直接复制到旧版本上面（对已经存在的 App 运行 `ditto` 或 `cp -R`）：那样会原地改写可执行文件，macOS 会杀掉正在运行它的进程。
4. 打开 Cupper Local。数据库结构有变化时，新版本的宿主先把旧库备份到 `cupper-local/backups/`，再执行迁移。
5. 之前给过的文件夹访问等授权会再问一次：ad-hoc 签名的每个版本在 macOS 看来都是新程序（须知第 5 节）。用 `ln -s` 装的命令行不用重装。

**忘了退出就替换了：** 旧宿主在下一次有客户端连接（例如运行新版本的命令行）或要启动 agent 时，发现磁盘上的程序已经不是自己，进入"待升级"：不再开始新会话和新回合，运行中的回合结束后自己退出。新版本的命令行认得出旧宿主不是自己那一版，会拒绝连接（退出码 4）。这时退出 Cupper Local 再打开就行。不要接着用旧界面：旧宿主退出后，旧界面会拉起磁盘上的新宿主，新旧两版混在一起运行。

**回到旧版本：** 不支持降级。新版本迁移过数据库后，旧版本的宿主会以"数据来自更新的版本"拒绝启动。迁移前的备份在 `backups/` 下，用它回到旧版本会丢掉迁移之后的全部记录；Cupper Local 开着时不要动数据目录。

## 卸载

退出 Cupper Local，删掉 `/Applications/Cupper Local.app`、命令行的链接，以及上表里的数据、日志和 worktree 目录（worktree 里可能有还没合并的改动，先看一眼）。
