//! The `cupper-fake-agent` binary; see the library documentation.

use cupper_fake_agent::config::Config;
use cupper_fake_agent::{agent, script};

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
    runtime.block_on(agent::serve(config, agent::stdio()))?;
    Ok(())
}
