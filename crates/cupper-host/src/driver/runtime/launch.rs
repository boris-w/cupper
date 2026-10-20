//! Starting an adapter instance (design 5.6.1, 5.6.6, ADR-08): the program
//! and its environment, the check before every start, and the pieces of a
//! running instance: its process group, its ACP connection through the tap,
//! its stderr in the adapter log and its `adapters` row.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::Arc;

use agent_client_protocol::schema::v1::InitializeResponse;
use cupper_protocol::capabilities::{CliCompat, Versions};
use cupper_protocol::ids::AdapterInstanceId;
use cupper_sys::{Command, Env, ProcessGroup, Signal, Stdio};
use sha2::{Digest, Sha256};
use tokio::net::unix::pipe;
use tokio::sync::mpsc;

use crate::clock::Clock;
use crate::diag::stderr::{AdapterLog, Tail};
use crate::driver::acp::{AcpConnection, FromAgent};
use crate::driver::api::{AgentError, AgentErrorClass, AgentErrorForm};
use crate::driver::locate::env_rules::{self, adapter_env, check_config_dir};
use crate::driver::locate::trust::{Boundaries, system_temp_dirs};
use crate::driver::locate::{CliLocator, resolve};
use crate::driver::probe::login_shell::{self, Failure, LoginEnv};
use crate::driver::probe::version::{self, VerifiedRange, VersionClass};
use crate::driver::tap::Tap;
use crate::driver::transport::StdioTransport;
use crate::durations::Durations;
use crate::manifest::{self, FileHash};
use crate::store::{Durability, RawSink, Receipt, Writer};

/// What the adapter runs.
#[derive(Debug, Clone)]
pub enum Program {
    /// `test_config.agents`: without the runtime or CLI discovery (design
    /// 5.9.9).
    Injected { program: PathBuf, args: Vec<String> },
    /// The runtime installer's node running the adapter's bundle (T056),
    /// both checked against the embedded manifest before each start.
    Runtime { node: PathBuf, bundle: PathBuf },
}

/// How the agent's adapters start.
#[derive(Debug, Clone)]
pub struct Launch {
    pub program: Program,
    pub env: Env,
    pub versions: Versions,
    /// The native CLI and its version, for the `adapters` row.
    pub cli: Option<(PathBuf, String)>,
    /// Why no adapter can start: no CLI was found.
    pub unavailable: Option<AgentErrorClass>,
}

/// A runtime file that does not hash to what the manifest says.
#[derive(Debug, thiserror::Error)]
#[error("{0} does not match the manifest")]
pub struct Mismatch(PathBuf);

/// Checks `path` against `expected`: its sha-256 and, when given, its cdhash.
pub fn verify(path: &Path, expected: &FileHash) -> Result<(), Mismatch> {
    let mismatch = || Mismatch(path.to_owned());
    let content = fs::read(path).map_err(|_| mismatch())?;
    let sha256: String = Sha256::digest(content)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let cdhash = || cupper_sys::file_cdhash(path).ok().map(|c| c.to_string());
    let signed = expected
        .cdhash
        .as_ref()
        .is_none_or(|want| cdhash().as_ref() == Some(want));
    (sha256 == expected.sha256 && signed)
        .then_some(())
        .ok_or_else(mismatch)
}

/// The login shell's environment (design 5.6.6): `test_config.login_env`
/// when given, else what `$SHELL -l -i` prints, started with the host's own
/// basic variables.
pub async fn login_env(
    clock: &dyn Clock,
    durations: &Durations,
    host: &BTreeMap<String, String>,
    injected: Option<BTreeMap<String, String>>,
) -> LoginEnv {
    if let Some(vars) = injected {
        let failure = None;
        return LoginEnv { vars, failure };
    }
    let Some(shell) = host.get("SHELL").filter(|shell| !shell.is_empty()) else {
        return LoginEnv::fallback(Failure::ShellMissing);
    };
    let base = [
        "HOME", "USER", "LOGNAME", "SHELL", "TMPDIR", "LANG", "LC_*", "PATH",
    ];
    let allow = &manifest::embedded().env.allow;
    let wanted: Vec<&str> = allow.iter().map(String::as_str).collect();
    let env = Env::from_allowed(host, &base);
    login_shell::read(clock, durations, Path::new(shell), env, &wanted).await
}

/// The launch of an injected `program`, with the environment rules all the
/// same (design 5.6.6); the config dir comes unchecked from the test.
#[cfg(any(test, feature = "test-harness"))]
pub fn injected(
    program: PathBuf,
    args: Vec<String>,
    host: &BTreeMap<String, String>,
    login: &LoginEnv,
) -> Launch {
    let config_dir = login.vars.get("CLAUDE_CONFIG_DIR").cloned();
    let config_dir = config_dir.map(env_rules::ConfigDir::from_test_config);
    let claude = env_rules::CLAUDE;
    let (env, _) = adapter_env(
        claude,
        host,
        &login.vars,
        &program,
        config_dir.as_ref(),
        &[],
    );
    let versions = versions(String::new(), CliCompat::Unsupported);
    let (program, cli, unavailable) = (Program::Injected { program, args }, None, None);
    Launch {
        program,
        env,
        versions,
        cli,
        unavailable,
    }
}

/// The fake agent of this build: `target/<profile>/cupper-fake-agent`, beside
/// the `deps/` of the test binary that asks. Tests do not build it; `cargo
/// xtask ci` does before it runs them.
#[cfg(any(test, feature = "test-harness"))]
pub fn fake_agent() -> PathBuf {
    let exe = std::env::current_exe().expect("the test binary");
    let profile = exe.parent().and_then(Path::parent).unwrap();
    let agent = profile.join("cupper-fake-agent");
    let hint = "run `cargo build -p cupper-fake-agent` first";
    assert!(agent.exists(), "{} is missing: {hint}", agent.display());
    agent
}

/// The launch of Claude's adapter from the runtime in `data_dir`: the CLI
/// found where agents cannot write (`roots`: every project), its version
/// classified against the manifest's range (design 5.6.6).
pub async fn production(
    clock: &dyn Clock,
    durations: &Durations,
    data_dir: &Path,
    roots: Vec<PathBuf>,
    host: &BTreeMap<String, String>,
    login: &LoginEnv,
) -> Launch {
    let runtime = data_dir.join("runtime");
    let node = runtime.join("node");
    let bundle = runtime.join("bundle-claude-acp.mjs");
    let mut launch = Launch {
        program: Program::Runtime { node, bundle },
        env: Env::new(),
        versions: versions(String::new(), CliCompat::Unsupported),
        cli: None,
        unavailable: Some(AgentErrorClass::CliMissing),
    };
    let tmp = host.get("TMPDIR").map(String::as_str);
    let boundaries = Boundaries::new(roots, system_temp_dirs(tmp));
    let path = login.vars.get("PATH").map_or("", String::as_str);
    let home = PathBuf::from(host.get("HOME").cloned().unwrap_or_default());
    let locator = CliLocator::new(boundaries.clone(), path, home);
    let Ok(found) = locator.locate(&resolve::CLAUDE, None) else {
        return launch;
    };
    let config_dir = login.vars.get("CLAUDE_CONFIG_DIR");
    let uid = cupper_sys::current_uid();
    let config_dir = config_dir.and_then(|dir| check_config_dir(dir, uid, &boundaries).ok());
    let claude = env_rules::CLAUDE;
    let binary = &found.binary;
    (launch.env, _) = adapter_env(claude, host, &login.vars, binary, config_dir.as_ref(), &[]);
    let found_version = version::probe(clock, durations, binary, launch.env.clone()).await;
    let range = &manifest::embedded().cli.claude;
    let range = VerifiedRange {
        min: &range.verified.min,
        max: &range.verified.max,
        unsupported_below: range.unsupported_below.as_deref(),
    };
    let class = found_version.and_then(|v| version::classify(v, &range).ok());
    let compat = match class {
        Some(VersionClass::Verified) => CliCompat::Verified,
        Some(VersionClass::NewerUntested) => CliCompat::NewerUntested,
        Some(VersionClass::OlderUntested) => CliCompat::OlderUntested,
        _ => CliCompat::Unsupported,
    };
    let cli_version = found_version.map(|v| format!("{}.{}.{}", v.major, v.minor, v.patch));
    let cli_version = cli_version.unwrap_or_default();
    launch.versions = versions(cli_version.clone(), compat);
    (launch.cli, launch.unavailable) = (Some((found.binary, cli_version)), None);
    launch
}

fn versions(cli: String, cli_compat: CliCompat) -> Versions {
    let adapter = manifest::embedded().claude_agent_acp.version.clone();
    Versions {
        adapter,
        cli,
        cli_compat,
    }
}

/// What an instance starts with besides its launch.
#[derive(Clone)]
pub struct Context {
    pub clock: Arc<dyn Clock>,
    pub writer: Writer,
    pub raw: RawSink,
    pub log: Option<Arc<AdapterLog>>,
}

/// A running adapter instance.
#[derive(Debug)]
pub struct Instance {
    pub id: AdapterInstanceId,
    /// Dropping it closes the adapter's stdin.
    pub conn: AcpConnection,
    pub init: InitializeResponse,
    pub group: ProcessGroup,
    /// Its latest stderr, for the details of an `error` item.
    pub tail: Option<Tail>,
}

fn failed(class: AgentErrorClass, message: impl ToString) -> AgentError {
    let (form, message) = (AgentErrorForm::Text, message.to_string());
    AgentError {
        class,
        form,
        message,
    }
}

/// Starts instance `id` of `launch` in a session of its own (design 5.8.5)
/// and connects to it: what the agent sends goes to `mailbox`, and `exited`
/// hears the process's end.
pub async fn start<M: From<FromAgent> + Send + 'static>(
    launch: &Launch,
    cx: &Context,
    id: AdapterInstanceId,
    mailbox: mpsc::UnboundedSender<M>,
    exited: impl FnOnce(ExitStatus) + Send + 'static,
) -> Result<Instance, AgentError> {
    use AgentErrorClass::RuntimeFailed;
    if let Some(class) = launch.unavailable {
        return Err(failed(class, "the agent's CLI was not found"));
    }
    let mut command = match &launch.program {
        Program::Injected { program, args } => {
            let mut command = Command::new(program);
            command.args(args);
            command
        }
        Program::Runtime { node, bundle } => {
            let files = &manifest::embedded().files;
            let check =
                |path, name| verify(path, &files[name]).map_err(|e| failed(RuntimeFailed, e));
            check(node, manifest::NODE)?;
            check(bundle, manifest::CLAUDE_BUNDLE)?;
            let mut command = Command::new(node);
            command.arg(bundle);
            command
        }
    };
    command.env(launch.env.clone()).new_session();
    for stdio in [Command::stdin, Command::stdout, Command::stderr] {
        stdio(&mut command, Stdio::Pipe);
    }
    let mut child = cupper_sys::spawn(&command).map_err(|e| failed(RuntimeFailed, e))?;
    let group = child.group();
    record(cx, &id, &group, launch).await;
    let pipes = (child.stdin.take(), child.stdout.take(), child.stderr.take());
    tokio::task::spawn_blocking(move || {
        if let Ok(status) = child.wait() {
            exited(status);
        }
    });
    let (Some(stdin), Some(stdout), Some(stderr)) = pipes else {
        unreachable!("all three are pipes")
    };
    let tail = match (&cx.log, pipe::Receiver::from_owned_fd(stderr)) {
        (Some(log), Ok(stderr)) => Some(log.read(id.to_string(), stderr).0),
        _ => None,
    };
    let opened = match StdioTransport::new(stdin, stdout) {
        Ok(transport) => {
            let tap = Tap::new(id.to_string(), Arc::clone(&cx.clock), cx.raw.clone());
            AcpConnection::open(transport, tap, mailbox)
                .await
                .map_err(|e| e.to_string())
        }
        Err(err) => Err(err.to_string()),
    };
    match opened {
        Ok((conn, init)) => Ok(Instance {
            id,
            conn,
            init,
            group,
            tail,
        }),
        Err(err) => {
            let _ = group.signal(Signal::Kill);
            Err(failed(RuntimeFailed, err))
        }
    }
}

/// The `adapters` row, written as soon as the process runs, so that a host
/// that dies leaves a record of what to clean up (design 5.2.13).
async fn record(cx: &Context, id: &AdapterInstanceId, group: &ProcessGroup, launch: &Launch) {
    let manifest = manifest::embedded();
    let cli = launch.cli.clone();
    let (cli_path, cli_version) = cli.map(|(p, v)| (p.display().to_string(), v)).unzip();
    let row = (
        id.to_string(),
        group.pgid,
        group.start.as_micros(),
        cx.clock.now_wall_ms(),
    );
    let versions = (
        manifest.claude_agent_acp.version.clone(),
        manifest.node.version.clone(),
    );
    let write = cx.writer.submit(Durability::Normal, move |conn| {
        conn.execute(
            "INSERT INTO adapters (instance_id, agent, pid, pgid, start_us, adapter_version,
                 node_version, cli_path, cli_version, started_at)
             VALUES (?1, 'claude', ?2, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            (
                row.0,
                row.1,
                row.2,
                versions.0,
                versions.1,
                cli_path,
                cli_version,
                row.3,
            ),
        )
        .map(drop)
    });
    // Without the row the host only loses the cleanup of a crash.
    let _ = write.await;
}

/// The instance's end in its `adapters` row.
pub fn ended(writer: &Writer, at: u64, id: &AdapterInstanceId, status: ExitStatus) -> Receipt<()> {
    let (id, status) = (id.to_string(), status.to_string());
    writer.submit(Durability::Normal, move |conn| {
        let sql = "UPDATE adapters SET ended_at = ?2, exit_status = ?3 WHERE instance_id = ?1";
        conn.execute(sql, (id, at, status)).map(drop)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::testutil::TempDir;

    #[test]
    fn verify_checks_a_runtime_file_against_its_manifest_entry() {
        let dir = TempDir::new("runtime-verify");
        let node = dir.path().join("node");
        fs::write(&node, b"node").unwrap();
        // sha-256 of "node".
        let sha256 = "545ea538461003efdc8c81c244531b003f6f26cfccf6c0073b3239fdedf49446";
        let entry = |sha256: &str, cdhash: Option<&str>| FileHash {
            sha256: sha256.to_owned(),
            cdhash: cdhash.map(str::to_owned),
        };
        assert!(verify(&node, &entry(sha256, None)).is_ok());
        assert!(verify(&node, &entry(&sha256.replace('5', "6"), None)).is_err());
        // Unsigned: no cdhash to match.
        assert!(verify(&node, &entry(sha256, Some("00"))).is_err());
        assert!(verify(&dir.path().join("missing"), &entry(sha256, None)).is_err());
        // The embedded manifest names both files the runtime checks.
        let files = &manifest::embedded().files;
        assert!(files.contains_key(manifest::NODE) && files.contains_key(manifest::CLAUDE_BUNDLE));
    }
}
