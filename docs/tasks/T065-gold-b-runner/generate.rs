//! One-off generator of T065's first GOLD-B recordings (not compiled in the
//! repository; T064's `cargo xtask compat --fake` takes over). A host process
//! with the claude-shaped fake agent runs each scenario on the system clock,
//! `diag/recording_export` exports the session, and the header and paths are
//! rewritten for `testdata/`.
//!
//! Run from the repository, with a `[[test]]` entry `gen_gold_b` that
//! requires `test-harness`:
//! `cargo build -p cupper-fake-agent && cargo test -p cupper-host --features test-harness --test gen_gold_b -- --ignored`

mod common;

use std::path::Path;

use cupper_cli::client;
use cupper_host::gateway::testkit::TempDir;
use cupper_protocol::methods::admin::DiagRecordingExport;
use cupper_protocol::methods::session::SessionInterrupt;
use cupper_protocol::recording::{Recording, Usage};
use serde_json::json;

use common::*;

/// Where the recordings go, and what stands for the project's path.
const OUT: &str = "testdata/recordings/cupper-fake-agent/0.0.0/fake";
const WORK: &str = "/work/project";

#[derive(Clone, Copy)]
enum Act {
    None,
    Allow,
    Interrupt,
}

const SCENARIOS: &[(&str, &str, Act)] = &[
    (
        "normal-turn",
        "say hello; think pondering; tool t1 read Read notes; tool_done t1 20; say done",
        Act::None,
    ),
    (
        "approval",
        "tool t1 execute Run tests; permission t1; say finished",
        Act::Allow,
    ),
    ("cancel", "say working; wait_cancel", Act::Interrupt),
];

#[tokio::test(flavor = "multi_thread")]
#[ignore = "writes testdata; run by hand"]
async fn generate_gold_b_fake_recordings() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (scenario, script, act) in SCENARIOS {
        let dir = TempDir::new("gen-gold-b");
        let (data_dir, host, client) = fake_agent_host(&dir, json!({})).await;
        let mut host = Running(host);
        let session = create(&client, script).await;
        match act {
            Act::None => {}
            Act::Allow => {
                let asked = events(&data_dir, &session, "interaction.requested", 1).await;
                let allow = json!({"type": "selected", "option_id": "allow-once"});
                respond(&client, &session, &asked[0], allow).await.unwrap();
            }
            Act::Interrupt => {
                events(&data_dir, &session, "item.delta", 1).await;
                let params = json!({"command_id": client::command_id(), "session_id": session});
                let params = serde_json::from_value(params).unwrap();
                client.request::<SessionInterrupt>(params).await.unwrap();
            }
        }
        events(&data_dir, &session, "turn.completed", 1).await;
        let params = serde_json::from_value(json!({"session_id": session})).unwrap();
        let blob = client.request::<DiagRecordingExport>(params).await.unwrap().blob;
        let hash = blob.hash.to_string();
        let path = data_dir.join("blobs/sha256").join(&hash[..2]).join(&hash);
        let mut text = std::fs::read_to_string(path).unwrap();
        let real = std::fs::canonicalize(dir.path()).unwrap();
        for project in [real.to_str().unwrap(), dir.path().to_str().unwrap()] {
            text = text.replace(project, WORK);
        }
        let mut recording = Recording::parse(&text).unwrap();
        recording.header.scenario = (*scenario).to_owned();
        recording.header.usage = vec![Usage::GoldB];
        recording.header.adapter = "cupper-fake-agent".into();
        recording.header.adapter_version = "0.0.0".into();
        recording.header.cli_version = "fake".into();
        let out = repo.join(OUT);
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join(format!("{scenario}.jsonl")), recording.to_jsonl()).unwrap();
        terminate(&host.0);
        assert_eq!(exit_code(&mut host.0).await, 0);
    }
}
