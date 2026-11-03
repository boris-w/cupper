# T112 工作笔记

## 开工前的估计

停止线 600、硬停止线 750。用例放进 `tests/driver_k.rs`（K 表的进程级用例已在这里，`test-harness` 的 `[[test]]` 也已有），codex 形态的假 agent 用 `--persona codex`。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 用例的辅助 | codex 形态的宿主、以 codex 建会话、读原始帧 | — | 35 |
| IT-K2-reject-ends-turn | 拒绝只有 `cancel`，回合 `cancelled`，选项 `ends_turn`，队列不暂停 | — | 30 |
| IT-K3-late-output | 中断时的命令 `abandoned` 加 `may_still_run`，晚到的输出成 `late_tool_output` | — | 35 |
| IT-K4-cancel-pending-approval | 中断时挂着的审批标 `cancelled`，原始帧里有宿主替用户的 `cancelled` 应答 | — | 25 |
| IT-K7-restartable | `1001`、`restartable`，回合 `failed` 带 `error` 条目（`retry`），下一个 prompt 正常 | — | 20 |
| IT-K12-plan-confirm-idle | 线程已 idle 时计划确认不失效，回合以 prompt 响应为准 | — | 25 |
| IT-K17-text-error | `systemError` 加文字：回合 `failed`、`model_unsupported`，`error` 条目带原文 | — | 20 |
| IT-K18-upstream-retry | `notice: upstream_retry` 带进度 | — | 15 |
| K14 打开时改判（T063 交接） | codex 形态打开时 `-32000`：低版本改判 `cli_too_old`，否则 `not_logged_in` 并改登录状态 | 25 | 20 |
| 跑出来的缺陷 | 按 1.2 的上限就地修，每处不超过 100 行 | 约 150 | 约 60 |
| 小计 | | 约 175 | 约 285 |

合计约 460。

## 跑出来的结果

七个用例里，K2、K3、K4、K7、K17 第一次跑就通过，宿主的行为已经符合 K 表；K12 第一次失败，是用例把请求的字段名写错了（`request.kind`，实际是 `request.type`）。T110、T226 交接的 K17 已由 T226 做到：文字错误经 `PromptFailed` 交给回合机，`Failure::of` 写出唯一一条 `error` 条目，`raw` 是那段文字，`guidance` 是 `change_model`。

缺陷：

1. **K14 在会话打开时没有改判**（T063 的交接）：Codex 在 `session/new` 就返回 `-32000`，运行时只对 prompt 的失败做 K14 改判和登录状态更新，打开时的失败照原样以 `not_logged_in` 返回，CLI 低于已验证范围时也一样，登录状态也不变。就地修：改判和登录状态并成一个 `judged()`，prompt 和打开两处都经过它（约 25 行）。用例 `it_k14_old_cli_auth_codex_fails_on_opening`。
2. **`notice: upstream_retry` 从来不写**（K18）：运行时把 `UpstreamRetry {attempt, max}` 交给会话，会话只拿它给自发回合判定算活动。按设计要"同一回合内 upsert"，需要给 `ItemPatch` 加 `params`、改两边的 session view，超出 1.2 就地修的上限。编排者裁定：另开 T230，IT-K18-upstream-retry 移过去，本分支不留半成品。

## 留给 T230 的 K18 用例草稿

本分支上写过、跑过（失败在 `items(..., "notice", 1)`：一条 notice 都没有）的用例如下，放进 `tests/driver_k.rs` 即可用（依赖本任务的 `host_of`、`codex_session`、`items`）。按设计做好之后，最后一段应改为读这条 notice 的最终 `params`（`item.updated` 合并之后），而不是取 `item.started`/`item.updated` 里最后一条的 JSON 文本：

```rust
/// IT-K18-upstream-retry: Codex retries an overloaded upstream by itself;
/// one `upstream_retry` notice of the turn follows its attempts.
#[tokio::test(flavor = "multi_thread")]
async fn it_k18_upstream_retry_shows_the_attempts() {
    let dir = TempDir::new("k18");
    let (data_dir, _host, client, _) = host_of("codex", &dir, &[], json!({})).await;
    let session = codex_session(&client, "upstream_retry 3; say through").await;
    let ended = events(&data_dir, &session, "turn.completed", 1).await;
    assert_eq!(ended[0]["status"], "completed");
    let notices = items(&data_dir, &session, "notice", 1).await;
    let retries: Vec<&Value> = notices
        .iter()
        .filter(|n| n["code"] == "upstream_retry")
        .collect();
    assert_eq!(retries.len(), 1, "{notices:?}");
    let params = format!(
        "SELECT json_extract(json, '$.data') FROM events WHERE stream = 'session/{session}'
         AND type IN ('item.started', 'item.updated') AND json_extract(json, '$.data.item_id') = '{}'
         ORDER BY seq", retries[0]["item_id"].as_str().unwrap()
    );
    let last = query(&data_dir.join("host.db"), &params).pop().unwrap();
    assert!(
        last.contains("\"attempt\":3") && last.contains("\"max\":5"),
        "{last}"
    );
}
```
