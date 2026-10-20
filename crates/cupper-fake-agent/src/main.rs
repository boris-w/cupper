//! The `cupper-fake-agent` binary; see the library documentation.

use anyhow::Context;
use cupper_fake_agent::config::Config;
use cupper_fake_agent::{agent, replay, script};
use tokio::io::BufReader;
use tokio::signal::unix::{SignalKind, signal};

/// `anyhow` only here, to print the cause chain of a failed start.
fn main() -> anyhow::Result<()> {
    let config = Config::parse(std::env::args().skip(1))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    if let Some(recording) = &config.replay {
        let text = std::fs::read_to_string(recording)
            .with_context(|| format!("cannot read the recording {}", recording.display()))?;
        let frames = replay::segment(&text, config.segment)?;
        let input = BufReader::new(tokio::io::stdin());
        let play = replay::play(&frames, input, tokio::io::stdout(), tokio::io::stderr());
        std::process::exit(runtime.block_on(play).unwrap_or_else(|e| {
            eprintln!("cupper-fake-agent: {e}");
            replay::EXIT_MISMATCH
        }));
    }
    if config.fault("exit_on_start") {
        std::process::exit(1);
    }
    script::record_inherited_fds();
    let linger = config.fault("linger");
    runtime.block_on(async {
        // Handling SIGTERM replaces its default action: the process stays.
        let term = linger
            .then(|| signal(SignalKind::terminate()))
            .transpose()?;
        let served = agent::serve(config, agent::stdio()).await;
        if let Some(_term) = term {
            std::future::pending::<()>().await;
        }
        anyhow::Ok(served?)
    })
}
