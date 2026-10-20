//! What the host finds out about a process that asks for elevation (design
//! 5.8.4): whether it could be an adapter's descendant ([`lineage`]), and
//! otherwise what the native dialog shows of it, checked by the host.

pub mod lineage;

use std::path::{Path, PathBuf};

use cupper_protocol::annotations::WirePath;
use cupper_protocol::methods::admin::{ChainProcess, VerifiedRequester};
use cupper_sys::{AuditToken, ProcessInfo, StartTime};
use futures::FutureExt;
use futures::future::BoxFuture;

use self::lineage::LiveAdapter;
use crate::store::Readers;

/// Clear, with what the dialog shows; or an adapter's possible descendant,
/// with the signals for the audit record: `same_session`, `same_group`,
/// `ancestor`, or `unchecked` when the process could not be examined.
pub type Finding = Result<VerifiedRequester, Vec<String>>;

/// Examines requesters against the `adapters` table and this app's CLI.
#[derive(Debug, Clone)]
pub struct ProcessRequesters {
    readers: Readers,
    /// `Contents/Helpers/cupper`, when the host runs inside the app.
    bundled: Option<PathBuf>,
}

impl ProcessRequesters {
    /// `host_exe`: the host's own executable, its links resolved.
    pub fn new(readers: Readers, host_exe: &Path) -> Self {
        let bundled = bundled_cli(host_exe);
        Self { readers, bundled }
    }

    /// Examines the process that `token` names.
    pub fn find(&self, token: AuditToken) -> BoxFuture<'static, Finding> {
        let (readers, bundled) = (self.readers.clone(), self.bundled.clone());
        async move {
            let Ok(adapters) = readers.read(live_adapters).await else {
                return Err(unchecked());
            };
            let examined = move || examine(&token, &adapters, bundled.as_deref());
            let examined = tokio::task::spawn_blocking(examined).await;
            examined.unwrap_or_else(|_| Err(unchecked()))
        }
        .boxed()
    }
}

/// The app's CLI beside the host's bundle: the host runs as
/// `<app>/Contents/Helpers/CupperHost.app/Contents/MacOS/cupper-host`.
fn bundled_cli(host_exe: &Path) -> Option<PathBuf> {
    let bundle = host_exe.ancestors().nth(3)?;
    let helpers = bundle
        .parent()
        .filter(|_| bundle.ends_with("CupperHost.app"))?;
    Some(helpers.join("cupper"))
}

fn unchecked() -> Vec<String> {
    vec!["unchecked".to_owned()]
}

fn live_adapters(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<LiveAdapter>> {
    let sql = "SELECT pid, pgid, start_us FROM adapters WHERE ended_at IS NULL";
    let mut select = conn.prepare(sql)?;
    let rows = select.query_map([], |row| {
        let start = StartTime::from_micros(row.get(2)?);
        let (pid, pgid) = (row.get(0)?, row.get(1)?);
        Ok(LiveAdapter { pid, pgid, start })
    })?;
    rows.collect()
}

/// Blocking: the process table and code signatures. What is read by pid
/// counts only if the token still names a running process afterwards.
fn examine(token: &AuditToken, adapters: &[LiveAdapter], bundled: Option<&Path>) -> Finding {
    let pid = token.pid();
    let chain = cupper_sys::parent_chain(pid);
    let now: Vec<ProcessInfo> = adapters
        .iter()
        .filter_map(|adapter| cupper_sys::process_info(adapter.pid))
        .collect();
    let signals = lineage::signals(&chain, adapters, &now);
    if !signals.is_empty() {
        return Err(signals.iter().map(|s| s.as_str().to_owned()).collect());
    }
    let (Ok(executable), Ok(cdhash), false) =
        (cupper_sys::pid_path(pid), token.cdhash(), chain.is_empty())
    else {
        return Err(unchecked());
    };
    let path_of = |p: &ProcessInfo| {
        let path = cupper_sys::pid_path(p.pid).map(|path| path.display().to_string());
        WirePath(path.unwrap_or_else(|_| p.name.clone()))
    };
    let parent_chain = (chain[1..].iter())
        .map(|p| ChainProcess {
            pid: p.pid.unsigned_abs(),
            executable: path_of(p),
        })
        .collect();
    let has_tty = cupper_sys::has_tty(pid).unwrap_or(false);
    let launchd = |p: Option<&ProcessInfo>| p.is_some_and(|p| p.pid == 1);
    // No terminal, launchd as the parent, or a chain cut short.
    let origin_unverifiable = !has_tty || launchd(chain.get(1)) || !launchd(chain.last());
    let signing = token
        .signing_identity()
        .ok()
        .flatten()
        .map(|id| match id.team_id {
            Some(team) => format!("{} ({team})", id.identifier),
            None => id.identifier,
        });
    let ours = |path: &Path| cupper_sys::file_cdhash(path).is_ok_and(|ours| ours == cdhash);
    let bundled_cli = bundled.is_some_and(|path| executable == path && ours(path));
    Ok(VerifiedRequester {
        executable: WirePath(executable.display().to_string()),
        signing,
        pid: pid.unsigned_abs(),
        parent_chain,
        has_tty,
        origin_unverifiable,
        bundled_cli,
    })
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixStream;

    use super::*;

    #[test]
    fn examine_reports_this_process_and_finds_an_ancestor_adapter() {
        let (ours, theirs) = UnixStream::pair().unwrap();
        let token = cupper_sys::peer_token(&ours).unwrap();
        let exe = std::fs::canonicalize(std::env::current_exe().unwrap()).unwrap();
        // This test process as the app's CLI, with no adapter running.
        let verified = examine(&token, &[], Some(&exe)).unwrap();
        assert_eq!(verified.pid, std::process::id());
        assert_eq!(verified.executable.0, exe.display().to_string());
        assert!(verified.bundled_cli && verified.signing.is_some());
        let parent = std::os::unix::process::parent_id();
        assert_eq!(verified.parent_chain[0].pid, parent);
        assert!(!examine(&token, &[], None).unwrap().bundled_cli);
        // The test's parent as an adapter: this process descends from it.
        let parent = cupper_sys::process_info(parent as i32).unwrap();
        let (pid, pgid, start) = (parent.pid, parent.pgid, parent.start);
        let adapter = LiveAdapter { pid, pgid, start };
        let signals = examine(&token, &[adapter], None).unwrap_err();
        assert!(signals.contains(&"ancestor".to_owned()), "{signals:?}");
        drop(theirs);
    }

    #[test]
    fn bundled_cli_sits_beside_the_host_bundle() {
        let host = Path::new("/A/Cupper.app/Contents/Helpers/CupperHost.app/Contents/MacOS/host");
        let cli = Path::new("/A/Cupper.app/Contents/Helpers/cupper");
        assert_eq!(bundled_cli(host).as_deref(), Some(cli));
        assert_eq!(
            bundled_cli(Path::new("/repo/target/debug/cupper-host")),
            None
        );
    }
}
