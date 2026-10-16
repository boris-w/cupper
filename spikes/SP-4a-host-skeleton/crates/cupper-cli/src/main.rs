//! `cupper-cli`: a second protocol client, for tests and manual checks.
//!
//! Without `--harness-key-file` it authenticates as the read-only `cli`
//! principal; write commands then fail with `forbidden`.

use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use cupper_cli::{Client, Incoming, Mirror, command_id};
use cupper_protocol::sync::StreamId;
use cupper_protocol::view::InteractionState;
use cupper_protocol::*;

#[derive(Parser, Debug)]
#[command(name = "cupper-cli", about = "cupper host protocol client (spike)")]
struct Args {
    /// Host data directory (the socket path is derived from it).
    #[arg(long)]
    data_dir: PathBuf,
    /// File holding the test-harness key; enables write commands on test builds.
    #[arg(long)]
    harness_key_file: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Subscribe and print events as JSON lines.
    Watch {
        /// `index` or `session/<id>`.
        stream: String,
        #[arg(long)]
        after: Option<u64>,
        /// Stop after this long without new data (ms).
        #[arg(long, default_value_t = 1500)]
        idle_ms: u64,
    },
    /// Print the index (sessions and their status).
    Status,
    Create {
        #[arg(long)]
        agent: String,
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        command_id: Option<String>,
    },
    Send {
        session: String,
        text: String,
        #[arg(long)]
        command_id: Option<String>,
        /// Wait for the turn to finish, printing agent text and pending requests.
        #[arg(long)]
        wait: bool,
    },
    Interrupt {
        session: String,
    },
    /// List pending interaction requests of a session.
    Pending {
        session: String,
    },
    /// Answer a permission request with one of its options.
    Approve {
        session: String,
        /// Option id (e.g. the `allow_once` option). Never pick an "always" option.
        #[arg(long)]
        option: String,
        /// Request id; defaults to the oldest pending permission request.
        #[arg(long)]
        request: Option<String>,
    },
    /// Answer a question.
    Answer {
        session: String,
        text: String,
        #[arg(long)]
        request: Option<String>,
    },
}

async fn connect(args: &Args) -> anyhow::Result<Client> {
    let socket = cupper_protocol::transport::socket_path(&args.data_dir, owner_uid(&args.data_dir)?);
    let auth = match &args.harness_key_file {
        Some(f) => AuthParams::Harness { token: std::fs::read_to_string(f).context("reading key file")?.trim().to_owned() },
        None => AuthParams::Peer,
    };
    Client::connect(&socket, auth, None).await.with_context(|| format!("connecting to {}", socket.display()))
}

/// The host only runs on a data dir owned by its user, so the owner is the uid
/// the socket path is derived from.
fn owner_uid(data_dir: &std::path::Path) -> anyhow::Result<u32> {
    use std::os::unix::fs::MetadataExt;
    Ok(std::fs::metadata(data_dir).with_context(|| format!("reading {}", data_dir.display()))?.uid())
}

/// Subscribe and fold until no data arrives for `idle`.
async fn mirror(c: &mut Client, stream: StreamId, idle: Duration, mut print: impl FnMut(&Incoming)) -> anyhow::Result<Mirror> {
    let mut m = Mirror::new(stream.clone());
    let r = c.request::<Subscribe>(SubscribeParams { stream: stream.to_string(), after_seq: None }).await?;
    m.on_subscribed(&r);
    while let Ok(inc) = c.next_timeout(idle).await {
        print(&inc);
        match &inc {
            Incoming::Snapshot(p) => m.on_snapshot(p)?,
            Incoming::Events(p) => m.on_events(p)?,
            Incoming::Reset(_) => bail!("stream reset by host"),
            _ => {}
        }
    }
    Ok(m)
}

fn pending_of(m: &Mirror) -> Vec<InteractionState> {
    m.session().map(|s| s.pending_interactions().cloned().collect()).unwrap_or_default()
}

async fn respond(c: &mut Client, session: &str, request: Option<String>, outcome: InteractionOutcome) -> anyhow::Result<()> {
    let m = mirror(c, StreamId::Session(session.to_owned()), Duration::from_millis(300), |_| {}).await?;
    let pending = pending_of(&m);
    let target = match request {
        Some(id) => pending.into_iter().find(|p| p.request_id == id),
        None => pending.into_iter().next(),
    }
    .context("no matching pending request")?;
    let r = c
        .request::<InteractionRespond>(InteractionRespondParams {
            meta: CommandMeta::new(command_id()),
            session_id: session.to_owned(),
            request_id: target.request_id.clone(),
            request_digest: target.digest.clone(),
            outcome,
        })
        .await?;
    println!("{}", serde_json::to_string(&r)?);
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut c = connect(&args).await?;
    let out = &mut std::io::stdout();
    match &args.cmd {
        Cmd::Watch { stream, after, idle_ms } => {
            let r = c.request::<Subscribe>(SubscribeParams { stream: stream.clone(), after_seq: *after }).await?;
            writeln!(out, "{}", serde_json::to_string(&r)?)?;
            while let Ok(inc) = c.next_timeout(Duration::from_millis(*idle_ms)).await {
                match inc {
                    Incoming::Events(p) => {
                        for e in p.events {
                            writeln!(out, "{}", serde_json::to_string(&e)?)?;
                        }
                    }
                    Incoming::Snapshot(p) => writeln!(out, "{}", serde_json::to_string(&p)?)?,
                    Incoming::Reset(p) => writeln!(out, "{}", serde_json::to_string(&p)?)?,
                    Incoming::BlobChunk(..) | Incoming::Other { .. } => {}
                }
            }
        }
        Cmd::Status => {
            let m = mirror(&mut c, StreamId::Index, Duration::from_millis(300), |_| {}).await?;
            for s in &m.index().context("index view")?.sessions {
                writeln!(out, "{}", serde_json::to_string(s)?)?;
            }
        }
        Cmd::Create { agent, cwd, command_id: id } => {
            let r = c
                .request::<SessionCreate>(SessionCreateParams {
                    meta: CommandMeta::new(id.clone().unwrap_or_else(command_id)),
                    agent: agent.clone(),
                    cwd: std::fs::canonicalize(cwd)?.display().to_string(),
                })
                .await?;
            writeln!(out, "{}", serde_json::to_string(&r)?)?;
        }
        Cmd::Send { session, text, command_id: id, wait } => {
            let r = c
                .request::<SessionSend>(SessionSendParams {
                    meta: CommandMeta::new(id.clone().unwrap_or_else(command_id)),
                    session_id: session.clone(),
                    text: text.clone(),
                    expected_turn_id: None,
                })
                .await?;
            writeln!(out, "{}", serde_json::to_string(&r)?)?;
            if *wait {
                wait_turn(&mut c, session, &r.turn_id).await?;
            }
        }
        Cmd::Interrupt { session } => {
            let r = c
                .request::<SessionInterrupt>(SessionInterruptParams {
                    meta: CommandMeta::new(command_id()),
                    session_id: session.clone(),
                    expected_turn_id: None,
                })
                .await?;
            writeln!(out, "{}", serde_json::to_string(&r)?)?;
        }
        Cmd::Pending { session } => {
            let m = mirror(&mut c, StreamId::Session(session.clone()), Duration::from_millis(300), |_| {}).await?;
            for p in pending_of(&m) {
                let line = serde_json::json!({ "request_id": p.request_id, "digest": p.digest, "request": p.request });
                writeln!(out, "{line}")?;
            }
        }
        Cmd::Approve { session, option, request } => {
            respond(&mut c, session, request.clone(), InteractionOutcome::Selected { option_id: option.clone() }).await?;
        }
        Cmd::Answer { session, text, request } => {
            respond(&mut c, session, request.clone(), InteractionOutcome::Answer { text: text.clone() }).await?;
        }
    }
    Ok(())
}

/// Follow the session stream until `turn_id` completes, printing what matters.
async fn wait_turn(c: &mut Client, session: &str, turn_id: &str) -> anyhow::Result<()> {
    let stream = StreamId::Session(session.to_owned());
    let mut m = Mirror::new(stream.clone());
    let r = c.request::<Subscribe>(SubscribeParams { stream: stream.to_string(), after_seq: None }).await?;
    m.on_subscribed(&r);
    let started = Instant::now();
    let mut announced = std::collections::HashSet::new();
    loop {
        match c.next_timeout(Duration::from_secs(600)).await? {
            Incoming::Snapshot(p) => m.on_snapshot(&p)?,
            Incoming::Events(p) => m.on_events(&p)?,
            Incoming::Reset(_) => bail!("stream reset by host"),
            _ => {}
        }
        let Some(s) = m.session() else { continue };
        for p in s.pending_interactions() {
            if announced.insert(p.request_id.clone()) {
                eprintln!("pending: {}", serde_json::json!({ "request_id": p.request_id, "request": p.request }));
            }
        }
        if let Some(t) = s.turn(turn_id).filter(|t| t.status != TurnStatus::Running) {
            let text: String = s.items.iter().filter(|i| i.turn_id == turn_id && i.kind == ItemKind::AgentMessage).map(|i| i.text.as_str()).collect();
            let line = serde_json::json!({
                "turn_id": turn_id, "status": t.status, "stop_reason": t.stop_reason,
                "agent_text": text, "elapsed_ms": started.elapsed().as_millis() as u64,
            });
            println!("{line}");
            return Ok(());
        }
    }
}
