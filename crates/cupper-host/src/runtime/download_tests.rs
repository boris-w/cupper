//! IT-runtime-download against a registry served here, with tarballs made
//! here: no network, and nothing of a real npm package.

use std::collections::HashMap;
use std::fs;
use std::pin::pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use flate2::Compression;
use flate2::write::GzEncoder;
use sha2::{Digest as _, Sha256, Sha512};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

use super::npm::{self, Package};
use super::plan::{self, ADAPTER, SCRIPT};
use super::tests::{BUNDLE_CHANGED, ESBUILD, Fixture, Histories, INSTALLED};
use super::{BundleDir, Component, Reason, Status};
use crate::clock::{Clock, ManualClock};
use crate::store::testutil::TempDir;

#[derive(Debug, Clone)]
pub(super) enum Reply {
    Body(Vec<u8>),
    /// 503 so many times, then the body.
    FailThen(u32, Vec<u8>),
    Status(u16),
    /// Never answers.
    Hang,
}

/// A minimal registry on 127.0.0.1: one request per connection, answered by
/// its route; it records each request's path and User-Agent.
pub(super) struct Registry {
    pub url: String,
    routes: Arc<Mutex<HashMap<String, Reply>>>,
    requests: Arc<Mutex<Vec<(String, String)>>>,
}

impl Registry {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let routes = Arc::new(Mutex::new(HashMap::new()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let (r, q) = (Arc::clone(&routes), Arc::clone(&requests));
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(serve(stream, Arc::clone(&r), Arc::clone(&q)));
            }
        });
        Self {
            url,
            routes,
            requests,
        }
    }

    pub fn route(&self, path: &str, reply: Reply) {
        self.routes.lock().unwrap().insert(path.to_owned(), reply);
    }

    /// The paths asked for, sorted, and every User-Agent seen.
    pub fn requests(&self) -> (Vec<String>, Vec<String>) {
        let requests = self.requests.lock().unwrap().clone();
        let (mut paths, agents): (Vec<_>, Vec<_>) = requests.into_iter().unzip();
        paths.sort();
        (paths, agents)
    }
}

async fn serve(
    mut stream: TcpStream,
    routes: Arc<Mutex<HashMap<String, Reply>>>,
    requests: Arc<Mutex<Vec<(String, String)>>>,
) {
    let mut head = Vec::new();
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        let mut chunk = [0; 1024];
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => head.extend_from_slice(&chunk[..n]),
        }
    }
    let head = String::from_utf8_lossy(&head).into_owned();
    let path = head
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_owned();
    let agent = head.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("user-agent")
            .then(|| value.trim().to_owned())
    });
    requests
        .lock()
        .unwrap()
        .push((path.clone(), agent.unwrap_or_default()));
    let reply = routes
        .lock()
        .unwrap()
        .get_mut(&path)
        .map(|reply| match reply {
            Reply::FailThen(0, body) => Reply::Body(body.clone()),
            Reply::FailThen(failures, _) => {
                *failures -= 1;
                Reply::Status(503)
            }
            reply => reply.clone(),
        });
    let (status, body) = match reply.unwrap_or(Reply::Status(404)) {
        Reply::Body(body) | Reply::FailThen(_, body) => (200, body),
        Reply::Status(status) => (status, Vec::new()),
        Reply::Hang => return std::future::pending().await,
    };
    let head = format!(
        "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(&[head.as_bytes(), &body].concat()).await;
}

/// A .tgz of regular files; names are written raw, so unsafe ones can be too.
pub(super) fn tarball(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
    for (name, contents) in files {
        let mut header = tar::Header::new_gnu();
        header.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name.as_bytes());
        header.set_entry_type(tar::EntryType::Regular);
        header.set_mode(0o644);
        header.set_size(contents.len() as u64);
        header.set_cksum();
        builder.append(&header, *contents).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}

pub(super) fn sri(bytes: &[u8]) -> String {
    let digest = base64::engine::general_purpose::STANDARD.encode(Sha512::digest(bytes));
    format!("sha512-{digest}")
}

/// A package-lock whose packages are `(key, tarball path, integrity, flag)`.
pub(super) fn lockfile(packages: &[(&str, &str, &str, Option<&str>)]) -> Vec<u8> {
    let mut entries = serde_json::Map::new();
    entries.insert(String::new(), serde_json::json!({"name": "root"}));
    for (key, path, integrity, flag) in packages {
        let resolved = format!("{}{path}", npm::REGISTRY);
        let mut entry = serde_json::json!({"resolved": resolved, "integrity": integrity});
        if let Some(flag) = flag {
            entry[*flag] = true.into();
        }
        entries.insert((*key).to_owned(), entry);
    }
    serde_json::to_vec(&serde_json::json!({"lockfileVersion": 3, "packages": entries})).unwrap()
}

const LIMIT: Duration = Duration::from_secs(60);

/// Downloads `lock` from `registry` into a new directory.
async fn fetch(
    registry: &Registry,
    lock: &[u8],
    clock: &ManualClock,
) -> (TempDir, Result<(), Reason>) {
    let dir = TempDir::new("download");
    let packages = npm::packages(lock, &registry.url).unwrap();
    let client = npm::client(None).unwrap();
    let fetched = npm::fetch(&client, clock, LIMIT, &packages, &dir.path().join("tree")).await;
    (dir, fetched)
}

#[tokio::test]
async fn it_runtime_download_fetches_the_locked_packages_only() {
    let registry = Registry::start().await;
    let a = tarball(&[("package/index.js", b"a"), ("package/lib/x.js", b"x")]);
    let b = tarball(&[("package/index.js", b"b")]);
    let sdk = tarball(&[("package/claude", b"binary")]);
    for (path, body) in [
        ("/a/-/a-1.0.0.tgz", &a),
        ("/b/-/b-2.0.0.tgz", &b),
        ("/sdk.tgz", &sdk),
    ] {
        registry.route(path, Reply::Body(body.clone()));
    }
    let lock = lockfile(&[
        ("node_modules/a", "a/-/a-1.0.0.tgz", &sri(&a), None),
        (
            "node_modules/a/node_modules/b",
            "b/-/b-2.0.0.tgz",
            &sri(&b),
            None,
        ),
        (
            "node_modules/sdk-darwin-arm64",
            "sdk.tgz",
            &sri(&sdk),
            Some("optional"),
        ),
        ("node_modules/tool", "sdk.tgz", &sri(&sdk), Some("dev")),
    ]);
    let (dir, fetched) = fetch(&registry, &lock, &ManualClock::new()).await;
    fetched.unwrap();
    let tree = dir.path().join("tree/node_modules");
    let read = |path: &str| std::fs::read(tree.join(path)).unwrap();
    assert_eq!(read("a/lib/x.js"), b"x");
    assert_eq!(read("a/node_modules/b/index.js"), b"b");
    let (paths, agents) = registry.requests();
    assert_eq!(paths, ["/a/-/a-1.0.0.tgz", "/b/-/b-2.0.0.tgz"]);
    let agent = concat!("cupper-runtime-installer/", env!("CARGO_PKG_VERSION"));
    assert!(agents.iter().all(|a| a == agent), "{agents:?}");
}

#[tokio::test]
async fn it_runtime_download_sri_or_tarball_paths_not_accepted_are_hash_mismatch() {
    let registry = Registry::start().await;
    let good = tarball(&[("package/index.js", b"a")]);
    let escape = tarball(&[("package/ok.js", b"ok"), ("package/../../escape.js", b"x")]);
    registry.route("/good.tgz", Reply::Body(good.clone()));
    registry.route("/escape.tgz", Reply::Body(escape.clone()));
    for lock in [
        lockfile(&[("node_modules/a", "good.tgz", &sri(b"other"), None)]),
        lockfile(&[("node_modules/a", "escape.tgz", &sri(&escape), None)]),
    ] {
        let (dir, fetched) = fetch(&registry, &lock, &ManualClock::new()).await;
        assert_eq!(fetched, Err(Reason::HashMismatch));
        assert!(!dir.path().join("escape.js").exists());
        assert!(!dir.path().join("tree/escape.js").exists());
    }
}

#[tokio::test]
async fn it_runtime_download_unreachable_failing_or_slow_registry_is_network() {
    let registry = Registry::start().await;
    registry.route("/failing.tgz", Reply::Status(503));
    registry.route("/slow.tgz", Reply::Hang);
    let lock = |path: &str| lockfile(&[("node_modules/a", path, &sri(b""), None)]);
    let clock = ManualClock::new();
    let (_dir, fetched) = fetch(&registry, &lock("failing.tgz"), &clock).await;
    assert_eq!(fetched, Err(Reason::Network));

    let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gone = Registry {
        url: format!("http://{}/", closed.local_addr().unwrap()),
        ..Registry::start().await
    };
    drop(closed);
    let (_dir, fetched) = fetch(&gone, &lock("a.tgz"), &clock).await;
    assert_eq!(fetched, Err(Reason::Network));

    let slow = lock("slow.tgz");
    let slow = fetch(&registry, &slow, &clock);
    let advance = async {
        while clock.next_deadline().is_none() {
            tokio::task::yield_now().await;
        }
        clock.advance(LIMIT);
    };
    let ((_dir, fetched), ()) = tokio::join!(slow, advance);
    assert_eq!(fetched, Err(Reason::Network));
}

#[test]
fn lockfile_packages_come_from_the_registry_given() {
    let lock = lockfile(&[
        ("node_modules/a", "a/-/a-1.0.0.tgz", "sha512-a", None),
        ("node_modules/x", "x.tgz", "sha512-x", Some("devOptional")),
        ("node_modules/l", "l.tgz", "sha512-l", Some("link")),
    ]);
    let packages = npm::packages(&lock, "http://127.0.0.1:1/").unwrap();
    let a = Package {
        key: "node_modules/a".into(),
        url: "http://127.0.0.1:1/a/-/a-1.0.0.tgz".into(),
        integrity: "sha512-a".into(),
    };
    assert_eq!(packages, [a]);
    let elsewhere = br#"{"packages": {"node_modules/a": {"resolved": "https://example.com/a.tgz", "integrity": "sha512-a"}}}"#;
    let escape = lockfile(&[("node_modules/../a", "a.tgz", "sha512-a", None)]);
    let outside = lockfile(&[("lib/a", "a.tgz", "sha512-a", None)]);
    for lock in [&elsewhere[..], &escape, &outside, b"{}"] {
        assert_eq!(
            npm::packages(lock, npm::REGISTRY),
            Err(Reason::HashMismatch)
        );
    }
}

/// Stands in for node running the bundling script, `<script> <esbuild-wasm
/// main> <root> <entry> <out>`: it fails unless its environment is `PATH`,
/// its own directory, alone (besides what `sh` adds) and esbuild-wasm is
/// installed, then copies the entry to the bundle.
const NODE: &str = r#"#!/bin/sh
env=$(/usr/bin/env | /usr/bin/grep -v -e '^PWD=' -e '^SHLVL=' -e '^_=')
[ "$env" = "PATH=${0%/node}" ] && [ -f "$2" ] || exit 9
/bin/cat "$3/$4" > "$3/$5"
"#;
const ENTRY: &[u8] = b"export const adapter = 1;";
const LOCK: &str = "Resources/runtime/locks/claude.json";

/// A bundle with [`NODE`] and a lockfile of one package, whose `index.js`
/// is the entry; the registry serves the package as `reply` makes it.
async fn claude(name: &str, reply: impl FnOnce(Vec<u8>) -> Reply) -> (Fixture, Registry) {
    let (mut fixture, registry) = (Fixture::new(name), Registry::start().await);
    let a = tarball(&[("package/index.js", ENTRY)]);
    let lock = lockfile(&[("node_modules/a", "a/-/a-1.0.0.tgz", &sri(&a), None)]);
    registry.route("/a/-/a-1.0.0.tgz", reply(a));
    fixture.add("Helpers/node", NODE.as_bytes(), 0o755);
    fixture.add(SCRIPT, b"// bundles", 0o644);
    fixture.add(LOCK, &lock, 0o644);
    let bundle = format!("{:x}", Sha256::digest(ENTRY));
    let entry = "node_modules/a/index.js";
    fixture.claude = Some(plan::npm("0.9.0", &fixture.files, LOCK, entry, &bundle));
    fixture.registry = registry.url.clone();
    (fixture, registry)
}

/// Runs the installer, moving the clock on to every retry, and the waits
/// before them. Only a retry waits 8 s or less: the download and bundling
/// limits are a minute and more.
async fn run(fixture: &Fixture) -> (Histories, Vec<Duration>) {
    let clock = ManualClock::new();
    let installer = fixture.installer(Arc::new(BundleDir(fixture.contents())), &[]);
    let mut ran = pin!(Fixture::run_on(&installer, &clock));
    let mut waits = Vec::new();
    loop {
        tokio::select! {
            biased;
            ran = &mut ran => return (ran, waits),
            () = tokio::task::yield_now() => {
                let wait = clock.next_deadline().map(|at| at.saturating_duration_since(clock.now_mono()));
                if let Some(wait) = wait.filter(|wait| *wait <= Duration::from_secs(8)) {
                    waits.push(wait);
                    clock.advance(wait);
                }
            }
        }
    }
}

#[tokio::test]
async fn it_runtime_download_installs_claude_agent_acp_by_its_lockfile() {
    let (fixture, registry) = claude("claude", Reply::Body).await;
    let (ran, waits) = run(&fixture).await;
    assert_eq!(ran[&Component::ClaudeAgentAcp], INSTALLED);
    assert!(waits.is_empty());
    let version = fixture.runtime().join("claude-agent-acp/0.9.0");
    assert_eq!(fs::read(version.join(ADAPTER)).unwrap(), ENTRY);
    // The bundle and `.installed.json`: no npm tree, no inputs.
    assert_eq!(fs::read_dir(&version).unwrap().count(), 2);
    assert_eq!(fixture.listing(Component::ClaudeAgentAcp), ["0.9.0"]);
    assert_eq!(registry.requests().0, ["/a/-/a-1.0.0.tgz"]);
}

#[tokio::test]
async fn it_runtime_download_network_failures_are_tried_again_after_2_4_and_8_s() {
    let secs = |s: &[u64]| {
        s.iter()
            .map(|s| Duration::from_secs(*s))
            .collect::<Vec<_>>()
    };
    let (fixture, registry) = claude("retry-fails", |_| Reply::Status(503)).await;
    let (ran, waits) = run(&fixture).await;
    let failed = [
        Status::Missing,
        Status::Installing,
        Status::Failed(Reason::Network),
    ];
    assert_eq!(ran[&Component::ClaudeAgentAcp], failed);
    assert_eq!(waits, secs(&[2, 4, 8]));
    assert_eq!(registry.requests().0.len(), 4);

    let (fixture, registry) = claude("retry-recovers", |a| Reply::FailThen(2, a)).await;
    let (ran, waits) = run(&fixture).await;
    assert_eq!(ran[&Component::ClaudeAgentAcp], INSTALLED);
    assert_eq!(waits, secs(&[2, 4]));
    assert_eq!(registry.requests().0.len(), 3);
}

#[tokio::test]
async fn it_runtime_download_bundle_not_matching_the_manifest_is_hash_mismatch() {
    let (mut fixture, registry) = claude("bundle-hash", Reply::Body).await;
    let plan = fixture.claude.as_mut().unwrap();
    plan.files.get_mut(ADAPTER).unwrap().sha256 = "00".repeat(32);
    let (ran, waits) = run(&fixture).await;
    let failed = [
        Status::Missing,
        Status::Installing,
        Status::Failed(Reason::HashMismatch),
    ];
    assert_eq!(ran[&Component::ClaudeAgentAcp], failed);
    assert!(waits.is_empty() && registry.requests().0.len() == 1);
    // Neither the npm tree nor a staging directory stays.
    assert!(fixture.listing(Component::ClaudeAgentAcp).is_empty());
}

#[tokio::test]
async fn it_runtime_download_waits_for_its_dependencies_and_fails_with_them() {
    let (fixture, registry) = claude("dependency", Reply::Body).await;
    let main = fixture.contents().join(format!("{ESBUILD}/lib/main.js"));
    fs::write(main, b"changed").unwrap();
    let (ran, _) = run(&fixture).await;
    assert_eq!(ran[&Component::EsbuildWasm], BUNDLE_CHANGED);
    assert_eq!(ran[&Component::ClaudeAgentAcp], BUNDLE_CHANGED);
    assert!(registry.requests().0.is_empty());
}
