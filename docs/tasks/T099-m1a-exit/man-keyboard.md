# MAN-keyboard：实体键盘确认（T099）

清单来自 T077 的 notes（"MAN-keyboard（待作者确认）"）和 T083 的 notes（"需要作者用实体键盘确认的几项"）。Playwright 和 `sendInputEvent` 都不经过系统的按键事件，所以这几项要作者在键盘上亲手按。做完填写本文末尾的"结果"一节；本文件就是 `tests/ids.toml` 里这条证据指向的记录。

## 用哪个构建

- 用 **e2e 构建**，假 agent 驱动，和 T083 的清单一致。
  - 开发构建的宿主不带 `test-harness`，不能换成假 agent，会去找真实的 Claude Code。按计划 1.10，这里不用真实 agent。
  - e2e 构建和开发构建的输入路径相同：主进程的 `before-input-event`、`inputGuard`、窗口桥写入的输入证据，以及宿主对 `interaction/respond` 的真实输入核对。
- **e2e 构建的限制**：主进程的原生对话框都换成了测试替身（`installDialogStandIns`）。没有 Playwright 时，替身既不显示，也不返回。
  - 所以"特权选项的编号弹出原生确认框"这件事看不到。第 3 项只核对"按了不会直接提交"，确认框本身由 E2E-SEC-panel-numbering 自动覆盖。这一项放在最后做。
  - 不要在界面上"添加项目"（会用到打开文件的对话框）。项目和会话都由 CLI 建。
- 在主工作区里做，提交是 `18b3640`，`git status` 为空。

### 构建（大约 3–10 分钟：缓存是热的时候 3 分钟左右，从头编译约 10 分钟）

```sh
pnpm install --frozen-lockfile
cargo xtask runtime-pack            # 运行时包已就绪时会很快结束
cargo build --locked -p cupper-fake-agent
pnpm build:e2e
```

构建产物都在 git 忽略的目录里（`apps/desktop/dist/`、`target/`），工作区仍然是干净的，不影响之后跑 `xtask ci nightly`。

## 准备与启动（终端 1，在仓库根目录）

临时根目录放在 `$TMPDIR/cupper-e2e/` 之外：e2e 套件启动时会清理那个目录下的残留。

```sh
REPO_ROOT=$(pwd)
APP="$REPO_ROOT/apps/desktop/dist/pack/e2e/mac-arm64/Cupper E2E.app/Contents/MacOS/Cupper E2E"
CLI="$REPO_ROOT/target/host-bundle/e2e/cupper"
FAKE="$REPO_ROOT/target/debug/cupper-fake-agent"
R=$(mktemp -d "${TMPDIR:-/tmp}/cupper-man-XXXXXX")
openssl rand -hex 32 > "$R/harness_key"; chmod 600 "$R/harness_key"
cat > "$R/test_config.json" <<EOF
{"agents": {"claude": {"program": "$FAKE"}},
 "login_env": {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin"},
 "cli_root": "$R",
 "runtime": {"registry_url": "http://127.0.0.1:9/"},
 "notifier": "record"}
EOF
mkdir "$R/repo" && (cd "$R/repo" && git init -q --initial-branch=main && echo e2e > README.md && git add README.md \
  && git -c commit.gpgsign=false -c user.name=cupper-e2e -c user.email=e2e@cupper.invalid commit -q -m init)
# CLI 以 harness 身份连宿主，密钥从 fd 3 读
c() { "$CLI" --data-dir "$R/data" --json --harness-key-fd 3 "$@" 3<"$R/harness_key"; }
CUPPER_E2E_DIR="$R" "$APP" >/dev/null 2>&1 &
APP_PID=$!
```

等窗口出现，然后建会话：

```sh
S=$(c new --agent claude --project "$R/repo" "say ready" | python3 -c 'import json,sys; print(json.loads(sys.stdin.readline())["session_id"])')
echo "$S"
```

在窗口左侧边栏点开标题为 "say ready" 的会话，消息流里会出现 "fake reply"。之后每一步都由终端 1 用 `c send "$S" "<剧本>"` 让假 agent 发请求。

终端 2（可选，用来看宿主记下的回答）：先把终端 1 里定义的 `R`、`CLI` 和 `c` 也定义一遍，再运行 `c watch "$S"`。每次回答后，会出现一行 `interaction.resolved`，里面的 `outcome` 就是宿主收到的回答。

**怎样算通过**：宿主只收窗口带真实输入证据的回答（设计 5.7.8，`policy::respond`）。所以只要请求以预期的选项结束，就说明这次按键被主进程认作了真实输入。看三处：

- 消息流里假 agent 回显 `permission:<选项>`，或者 `answer:<文字>`；
- 面板收起，记录行显示"已批准：…"或"已拒绝"；
- 终端 2 的 `outcome`。

假 agent 的 Claude 选项：

| 编号 | 名称 | optionId | 说明 |
|---|---|---|---|
| 1 | Yes | `allow-once` | |
| 2 | Yes, and always allow fake commands | `allow-with-updates` | 特权选项，标"需要确认" |
| 3 | No | `reject` | |

## 六项

每一项开始前都先做两件事：输入框获得焦点并且是空的，2 秒内没有打字。然后在终端 1 发出请求，再把窗口切回前台（点一下窗口标题栏，不要点到面板）。切回前台之后先等 1 秒以上再按键，第 6 项除外。

1. **Enter 批准**
   - 发请求：`c send "$S" "permission k1"`。
   - 面板出现，等约半秒（待命期 500 ms），按 **Enter**。
   - 通过：回显 `permission:allow-once`，记录行"已批准"。
2. **Esc 拒绝（有特权选项时也不弹确认框）**
   - 发请求：`c send "$S" "permission k2"`，选项里有特权选项 2。
   - 面板就绪后按 **Esc**。
   - 通过：立即回显 `permission:reject`，记录行"已拒绝"。
   - 如果 Esc 走到了原生确认框，e2e 构建里替身不会返回，请求会一直停在"等待处理"，这一项就算不通过。
3. **数字键选择并提交**
   - a. 发请求：`c send "$S" "permission k3"`，面板就绪后按 **3**。通过：回显 `permission:reject`。
   - b. 效果未知的选项：
     - 发请求：`c send "$S" 'permission_options [{"optionId":"allow-once","name":"Yes","kind":"allow_once"},{"optionId":"maybe","name":"Maybe","kind":"allow_maybe"},{"optionId":"reject","name":"No","kind":"reject_once"}]'`
     - 选项 2 标"效果未知"。按 **2**，通过：没有任何反应，请求仍在等待。
     - 再按 **1**，通过：回显 `permission:allow-once`。
   - c. 特权选项的编号放到最后，见第 7 步。
4. **输入法选词不触发**
   - a. 输入框：切到中文输入法（系统自带拼音即可），在输入框里打拼音（例如 `nihao`），按 **Enter** 选词。
     - 通过：只上屏汉字，消息没有发出。
     - 随后清空输入框，切回英文输入法。
   - b. 提问里的"在此输入"：
     - 发请求：`c send "$S" "question Name?"`，面板出现一个文字输入框"在此输入"。
     - 点进输入框，用中文输入法打拼音，按 **Enter** 选词。通过：回答没有提交，面板还在。
     - 等 1 秒以上，再按 **Enter**（如果界面提示别的提交方式，按提示做）。通过：回显 `answer:<刚才的文字>`。
   - c.（T077 的写法）在输入框里用拼音打字时发请求：`c send "$S" "permission k4"`。
     - 在面板出现的同时按 **Enter** 选词，选词后 1 秒内再按一次 **Enter**。
     - 通过：请求没有被回答。
     - 然后按第 5 项的 Tab 处理掉这个请求。
5. **打字时来请求，面板不抢焦点**
   - 在输入框里持续打英文字母（不要停），同时让终端 1 发请求：`c send "$S" "permission k5"`。可以先在终端里输好命令，切回窗口打几个字，再切过去回车。
   - 通过：面板出现，但提示"按 Tab 或点击以处理请求"。继续打字、按 Enter 或数字键，都不会回答请求。
   - 按 **Tab**（或点击面板）激活，等半秒，再按 **Enter**。通过：回显 `permission:allow-once`。
   - 最后清掉输入框里打的字。
6. **刚切回前台的 1 秒内按 Enter 要再按一次**
   - 发请求：`c send "$S" "permission k6"`，面板就绪。
   - 按 **Cmd-Tab** 切到别的应用，再切回来，立即（1 秒内）按 **Enter**。
   - 通过：请求没有被回答，面板提示"请再按一次"。
   - 过 1 秒以上再按 **Enter**。通过：回显 `permission:allow-once`。
7. **（最后做）特权选项的编号**
   - 发请求：`c send "$S" "permission k7"`，面板就绪后按 **2**。
   - 通过：请求没有被直接提交，仍是"等待处理"。在正式构建里，这里会弹出原生确认框；e2e 构建里由替身接管，看不到。
   - 做完直接收尾。

## 收尾

```sh
c interrupt "$S" || true                 # 结束可能还在等的回合
kill "$APP_PID"; sleep 2
pkill -f -- "--data-dir $R/data" || true   # 宿主（它会带走假 agent）
grep -m1 '"electron"' "$R/logs/main.log"   # 记下 Electron 版本
sw_vers -productVersion                  # 记下 macOS 版本
defaults delete dev.cupper.app.e2e 2>/dev/null; rm -f ~/Library/Preferences/dev.cupper.app.e2e.plist
rm -rf "$R"
```

## 结果（做完填写）

- 日期：
- 提交：`18b3640`（e2e 构建）
- Electron：
- macOS：
- 键盘与输入法：
- 执行人：作者

| 项 | 结论（通过 / 不通过） | 看到的现象 |
|---|---|---|
| 1 Enter 批准 | | |
| 2 Esc 拒绝（有特权选项） | | |
| 3a 数字键提交 | | |
| 3b 效果未知的编号没反应 | | |
| 4a 输入框里选词不发送 | | |
| 4b "在此输入"里选词不提交 | | |
| 4c 打拼音时来请求，选词与随后 1 秒内的 Enter 不算 | | |
| 5 打字时面板不抢焦点，Tab 后可提交 | | |
| 6 切回前台 1 秒内提示"请再按一次" | | |
| 7 特权选项的编号不直接提交 | | |

全部通过后，在 `tests/ids.toml` 末尾追加：

```toml
[[evidence]]
id = "MAN-keyboard"
date = "<上面的日期>"
record = "docs/tasks/T099-m1a-exit/man-keyboard.md"
versions = { electron = "<上面的 Electron 版本>" }
```
