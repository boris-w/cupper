use std::collections::HashMap;
use std::fs::{self, File};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use clap::Parser;
use cupper_host::driver::AgentSpec;
use cupper_host::host::{Config, Host};
use cupper_host::store::Store;
use cupper_host::{conn, sys};
use serde::Deserialize;

/// Exit code when another host already holds the data directory lock.
const EXIT_LOCKED: i32 = 3;

#[derive(Parser, Debug)]
#[command(name = "cupper-host", about = "cupper session host (spike)")]
struct Args {
    /// Data directory (created with mode 0700). Use a temporary directory.
    #[arg(long)]
    data_dir: PathBuf,
    /// Agents config (JSON). Defaults to `<data-dir>/agents.json`.
    #[arg(long)]
    agents: Option<PathBuf>,
    /// Events kept per stream behind the latest checkpoint.
    #[arg(long, default_value_t = 10_000)]
    retain_events: u64,
    /// Live event backlog per connection before a stream reset.
    #[arg(long, default_value_t = 4 << 20)]
    max_queue_bytes: usize,
    /// Write-time coalescing window for text fragments (ms).
    #[arg(long, default_value_t = 40)]
    write_coalesce_ms: u64,
    /// Read the `test_harness` key (hex) from stdin until EOF.
    #[cfg(feature = "test-harness")]
    #[arg(long)]
    harness_key_stdin: bool,
}

#[derive(Debug, Deserialize)]
struct AgentsFile {
    agents: HashMap<String, AgentSpec>,
}

fn private_dir(path: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(path).with_context(|| format!("creating {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    let meta = fs::metadata(path)?;
    if meta.uid() != sys::current_uid() {
        bail!("{} is not owned by the current user", path.display());
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_env("CUPPER_LOG").unwrap_or_else(|_| "info".into()))
        .init();
    let args = Args::parse();
    let data_dir = args.data_dir.clone();
    private_dir(&data_dir)?;
    private_dir(&data_dir.join("logs"))?;

    // Single instance: hold an exclusive flock on the lock file for our lifetime,
    // and only then touch the socket path.
    let lock = File::create(data_dir.join("host.lock"))?;
    if let Err(e) = lock.try_lock() {
        eprintln!("another host owns {}: {e}", data_dir.display());
        std::process::exit(EXIT_LOCKED);
    }

    let agents = match &args.agents {
        Some(p) => Some(p.clone()),
        None => Some(data_dir.join("agents.json")).filter(|p| p.exists()),
    };
    let agents = match agents {
        Some(p) => serde_json::from_slice::<AgentsFile>(&fs::read(&p).with_context(|| format!("reading {}", p.display()))?)?.agents,
        None => HashMap::new(),
    };

    #[cfg(feature = "test-harness")]
    let harness_key = if args.harness_key_stdin {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)?;
        Some(s.trim().as_bytes().to_vec())
    } else {
        None
    };
    #[cfg(not(feature = "test-harness"))]
    let harness_key: Option<Vec<u8>> = None;

    let config = Config {
        data_dir: data_dir.clone(),
        agents,
        retain_events: args.retain_events,
        max_queue_bytes: args.max_queue_bytes,
        write_coalesce: Duration::from_millis(args.write_coalesce_ms),
        harness_key,
    };

    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    rt.block_on(async move {
        let store = Arc::new(Store::open(&data_dir.join("host.db"))?);
        let host = Host::new(config, store)?;
        let report = host.recover()?;
        host.commands.prune();
        tracing::info!(?report, "recovery done");

        let socket = cupper_protocol::transport::socket_path(&data_dir, sys::current_uid());
        private_dir(socket.parent().context("socket path has a parent")?)?;
        // We hold the lock, so any socket file left here belongs to a dead host.
        let _ = fs::remove_file(&socket);
        let listener = tokio::net::UnixListener::bind(&socket).with_context(|| format!("binding {}", socket.display()))?;
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
        tracing::info!(host_id = %host.host_id, pid = std::process::id(), "listening");

        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            _ = conn::serve(host.clone(), listener) => {}
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
        tracing::info!("shutting down");
        host.shutdown_sessions();
        let reaped = tokio::task::spawn_blocking({
            let host = host.clone();
            move || host.reap_adapters()
        })
        .await??;
        tracing::info!(reaped, "adapters stopped");
        let _ = fs::remove_file(&socket);
        drop(lock);
        Ok::<_, anyhow::Error>(())
    })
}
