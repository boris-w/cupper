//! The `cupper-fake-agent` binary; see the library documentation.

use cupper_fake_agent::config::Config;
use cupper_fake_agent::{agent, script};
use tokio::signal::unix::{SignalKind, signal};

/// `anyhow` only here, to print the cause chain of a failed start.
fn main() -> anyhow::Result<()> {
    let config = Config::parse(std::env::args().skip(1))?;
    if config.fault("exit_on_start") {
        std::process::exit(1);
    }
    script::record_inherited_fds();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let linger = config.fault("linger");
    runtime.block_on(async {
        // Handling SIGTERM replaces its default action: the process stays.
        let term = linger
            .then(|| signal(SignalKind::terminate()))
            .transpose()?;
        agent::serve(config, agent::stdio()).await?;
        if let Some(_term) = term {
            std::future::pending::<()>().await;
        }
        anyhow::Ok(())
    })
}
