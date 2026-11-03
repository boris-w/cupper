//! IT-files-search and IT-workspace-open: `files/search` and
//! `workspace/open` (design 5.2.11) through the gateway, on the session
//! fixture. A plain directory is walked and a git work tree listed in the
//! snapshot tier; a list holds 5 s, unless a turn ends; `open` is a stand-in
//! that records what it was asked.

use std::fs;
use std::path::Path;
use std::time::Duration;

use cupper_protocol::ids::{ProjectId, SessionId};
use cupper_protocol::session::StopReason;
use serde_json::{Value, json};

use super::tests::{Fixture, done};
use super::trust_tests::git_project;
use crate::driver::api::PromptTag;
use crate::driver::probe::testkit::script;

fn write(root: &Path, path: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "").unwrap();
}

/// The paths found for `query`, and the first match's ranges.
async fn search(f: &Fixture, mut on: Value, query: &str) -> (Vec<String>, Value) {
    on["query"] = json!(query);
    let found = f.ok("files/search", on).await;
    let results = found["results"].as_array().unwrap();
    let paths = results
        .iter()
        .map(|r| r["path"].as_str().unwrap().to_owned());
    let first = results.first().map(|r| r["ranges_utf16"].clone());
    (paths.collect(), first.unwrap_or_default())
}

#[tokio::test]
async fn it_files_search_a_walked_directory_and_a_git_work_tree() {
    // Not a repository: walked, `.gitignore` honoured, hidden files kept.
    let f = Fixture::new("files-walk").await;
    let app = f.project();
    for path in [
        ".gitignore",
        "build/out.rs",
        "src/main.rs",
        ".github/ci.yml",
        "docs/😀 é.md",
    ] {
        write(&app, path);
    }
    fs::write(app.join(".gitignore"), "build/\n").unwrap();
    let project = json!({"project_id": ProjectId::from_uuid(1)});
    let all = [
        ".gitignore",
        "src/main.rs",
        ".github/ci.yml",
        "docs/😀 é.md",
    ];
    assert_eq!(
        search(&f, project.clone(), "").await.0,
        all,
        "shorter first"
    );
    let main = (vec!["src/main.rs".to_owned()], json!([[4, 8]]));
    assert_eq!(search(&f, project.clone(), "main").await, main);
    // In UTF-16 code units: the emoji takes two.
    assert_eq!(search(&f, project.clone(), "é").await.1, json!([[8, 9]]));
    let both =
        json!({"project_id": ProjectId::from_uuid(1), "session_id": SessionId::from_uuid(1)});
    assert_eq!(f.err("files/search", both).await, " ");

    // A git work tree: tracked and untracked, ignored ones left out.
    let (mut f, _home) = git_project("files-git").await;
    let app = f.project();
    for path in ["a.rs", "x.log", ".gitignore"] {
        write(&app, path);
    }
    fs::write(app.join(".gitignore"), "*.log\n").unwrap();
    let session = f.started().await;
    let on = json!({"session_id": session});
    assert_eq!(search(&f, on.clone(), "").await.0, ["a.rs", ".gitignore"]);
    // The list holds until the turn ends, and then for 5 s.
    write(&app, "b.rs");
    assert_eq!(search(&f, on.clone(), "rs").await.0, ["a.rs"]);
    f.emit(done(PromptTag(1), StopReason::EndTurn));
    f.events(&session, "turn.completed", 1).await;
    assert_eq!(search(&f, on.clone(), "rs").await.0, ["a.rs", "b.rs"]);
    write(&app, "c.rs");
    assert_eq!(search(&f, on.clone(), "rs").await.0, ["a.rs", "b.rs"]);
    f.clock.advance(Duration::from_secs(5));
    assert_eq!(search(&f, on, "rs").await.0, ["a.rs", "b.rs", "c.rs"]);
}

#[tokio::test]
async fn it_workspace_open_hands_the_workspace_to_the_app_of_the_setting() {
    let mut f = Fixture::new("open").await;
    let session = f.started().await;
    let (app, dir) = (f.project(), f.project().parent().unwrap().to_owned());
    // TextEdit, the default editor, is not on this machine.
    let body = r#"printf '%s|' "$@" >> "$0.log"; echo >> "$0.log"; [ "$2" != TextEdit ]"#;
    script(&dir, "open", body);
    let open = |target: &str| json!({"session_id": session, "target": target});
    for target in ["terminal", "finder"] {
        assert_eq!(f.ok("workspace/open", open(target)).await, json!({}));
    }
    let not_there = f.err("workspace/open", open("editor")).await;
    assert_eq!(not_there, "unavailable app_not_found");
    let asked = fs::read_to_string(dir.join("open.log")).unwrap();
    let ws = app.display();
    let expected = format!("-a|Terminal|{ws}|\n-a|Finder|{ws}|\n-a|TextEdit|{ws}|\n");
    assert_eq!(asked, expected);

    let other = json!({"session_id": SessionId::from_uuid(9), "target": "finder"});
    assert_eq!(f.err("workspace/open", other).await, "not_found ");
    fs::remove_dir_all(&app).unwrap();
    assert_eq!(f.err("workspace/open", open("finder")).await, "not_found ");
}
