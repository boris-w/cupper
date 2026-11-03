//! IT-set-config-tier: `session/set_config` (design 5.2.6 "配置项") through
//! the gateway. The permission mode follows the tier table of 5.8.2 cell by
//! cell, and records who set it, with full access's confirmation; other
//! options take `write`. An attached session's agent sets the option and
//! the reply carries what it says; a detached session takes the option at
//! its next attach.

use cupper_protocol::capabilities::{ConfigOption, SessionCapabilities};
use cupper_protocol::ids::SessionId;
use cupper_protocol::methods::handshake::Principal;
use serde_json::{Value, json};

use super::tests::{Fixture, caps, code, create, harness, send};
use crate::driver::api::{AgentError, AgentErrorClass, AgentErrorForm, ConfigResult};
use crate::policy::testkit::column;

const COLUMNS: [&str; 6] = [
    "UI control",
    "UI window",
    "cli + write",
    "cli + write + approve",
    "test_harness",
    "remote_device",
];

/// `options` with `option` at `value`.
fn taken(mut options: Vec<ConfigOption>, option: &str, value: &str) -> ConfigResult {
    let found = options.iter_mut().find(|o| o.id.as_str() == option);
    found.expect("an option of the session").current = value.into();
    Ok(options)
}

/// Claude's capabilities with a model option too.
fn with_model() -> SessionCapabilities {
    let mut caps = caps();
    let model = json!({"id": "model", "agent_option_id": "model", "current": "opus",
        "values": [{"value": "opus", "name": "Opus", "risk": "normal"},
                   {"value": "sonnet", "name": "Sonnet", "risk": "normal"}]});
    caps.config.push(serde_json::from_value(model).unwrap());
    caps
}

struct Session {
    f: Fixture,
    id: SessionId,
    commands: u32,
}

impl Session {
    async fn new(name: &str, caps: SessionCapabilities) -> Self {
        let mut f = Fixture::new(name).await;
        let created = f.ok("session/create", create("c-0")).await;
        f.attach_with(caps).await;
        f.prompt().await;
        let id = serde_json::from_value(created["session_id"].clone()).unwrap();
        Self { f, id, commands: 0 }
    }

    /// Sets `option` to `value` as `by`; the agent takes it.
    async fn set(&mut self, by: Principal, option: &str, value: &str) -> Result<Value, String> {
        self.commands += 1;
        let params = json!({"command_id": format!("set-{}", self.commands),
            "session_id": self.id, "option": option, "value": value});
        let answer = |option: &str, value: &str| taken(with_model().config, option, value);
        let set = self.f.set_config(by, params, answer).await;
        set.map(|r| r["config"].clone()).map_err(code)
    }

    fn set_by(&self) -> Vec<String> {
        let sql =
            "SELECT mode_set_by || ' ' || (full_access_confirmed_at IS NOT NULL) FROM sessions";
        self.f.rows(sql)
    }
}

#[tokio::test]
async fn it_set_config_tier_each_cell_of_the_tier_table() {
    let mut s = Session::new("set-config-tier", caps()).await;
    // Allowed (`Y`), or refused for lack of `approve` (`A`) or `privileged`
    // (`P`), or by SEC-9 (`S`). Each allowed change goes back to `default`.
    let loosen = [("auto", "YYAYYS"), ("bypass", "YPPPYS")];
    for (mode, cells) in loosen {
        for (name, cell) in COLUMNS.into_iter().zip(cells.chars()) {
            let by = column(name);
            let set = s.set(by.clone(), "permission_mode", mode).await;
            let refused = match cell {
                'A' => "forbidden scope",
                'P' => "forbidden scope",
                'S' => "forbidden role",
                _ => {
                    assert_eq!(set.unwrap()["permission_mode"], mode, "{name}");
                    let full = mode == "bypass";
                    let row = s.set_by();
                    let confirmed = row[0].ends_with(if full { "1" } else { "0" });
                    assert!(
                        row[0].starts_with(by.id.as_str()) && confirmed,
                        "{name}: {row:?}"
                    );
                    assert!(s.set(harness(), "permission_mode", "default").await.is_ok());
                    continue;
                }
            };
            assert_eq!(set.unwrap_err(), refused, "{mode}: {name}");
        }
    }
    // Tightening, or keeping the tier, takes `write` only.
    assert!(s.set(harness(), "permission_mode", "auto").await.is_ok());
    for name in COLUMNS {
        let tighten = s.set(column(name), "permission_mode", "default").await;
        assert_eq!(tighten.unwrap()["permission_mode"], "default", "{name}");
        assert!(s.set(harness(), "permission_mode", "auto").await.is_ok());
    }
    assert_eq!(
        s.set(column("cli"), "permission_mode", "default")
            .await
            .unwrap_err(),
        "forbidden scope"
    );
}

#[tokio::test]
async fn it_set_config_tier_other_options_refusals_and_a_detached_session() {
    let mut s = Session::new("set-config-other", with_model()).await;
    let set = s
        .set(column("cli + write"), "model", "sonnet")
        .await
        .unwrap();
    assert_eq!(
        set,
        json!({"permission_mode": "default", "model": "sonnet"})
    );
    assert_eq!(
        s.set(column("cli"), "model", "opus").await.unwrap_err(),
        "forbidden scope"
    );
    assert_eq!(
        s.set(harness(), "colour", "red").await.unwrap_err(),
        "unsupported "
    );
    assert_eq!(s.set(harness(), "model", "gpt").await.unwrap_err(), " ");
    // The agent refuses: its code and words come back.
    let params = json!({"command_id": "refused", "session_id": s.id, "option": "model",
        "value": "opus"});
    let refusal = |_: &str, _: &str| -> ConfigResult {
        let form = AgentErrorForm::Rpc { agent_code: -32602 };
        let message = "no such model".into();
        Err(AgentError {
            class: AgentErrorClass::Other,
            form,
            message,
        })
    };
    let refused =
        s.f.set_config(harness(), params, refusal)
            .await
            .unwrap_err();
    let data = &refused["data"];
    let shown = [&data["code"], &data["agent_code"], &data["agent_message"]];
    assert_eq!(shown, ["agent_error", "-32602", "no such model"]);

    // Detached after a restart: the option waits for the next attach.
    s.f.restart().await;
    let set = s.set(column("cli + write"), "model", "opus").await.unwrap();
    assert_eq!(set["model"], "opus");
    assert!(s.f.quiet(), "no agent to ask");
    s.f.ok("session/send", send(&s.id, "s-1", "next")).await;
    let config = s.f.attach_with(with_model()).await.config;
    assert_eq!(config["model"], "opus");
}
