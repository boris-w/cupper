//! What the host finds out about a process that asks for elevation (design
//! 5.8.4): whether it could be an adapter's descendant ([`lineage`]), and
//! otherwise what the native dialog shows of it, checked by the host.

pub mod lineage;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cupper_protocol::annotations::WirePath;
use cupper_protocol::methods::admin::{ChainProcess, VerifiedRequester};
use cupper_sys::{AuditToken, ProcessInfo, SigningIdentity, StartTime};
use futures::FutureExt;
use futures::future::BoxFuture;

use self::lineage::LiveAdapter;
use crate::elevation::{Examined, Requesters};
use crate::store::Readers;

/// Clear, with what the dialog shows; or an adapter's possible descendant,
/// with the signals for the audit record: `same_session`, `same_group`,
/// `ancestor`, or `unchecked` when the process could not be examined.
type Finding = Result<VerifiedRequester, Vec<String>>;

/// The CLI's signing identifier (design 5.2.14).
const CLI_IDENTIFIER: &str = "dev.cupper.cli";

/// Examines requesters against the `adapters` table and this app's CLI.
#[derive(Debug, Clone)]
pub struct ProcessRequesters {
    readers: Readers,
    bundled: Option<Arc<Bundled>>,
}

/// What tells the app's own CLI, without reading the app bundle (design
/// 5.8.4): where it is, and the host's own signature.
#[derive(Debug)]
struct Bundled {
    /// `Contents/Helpers/cupper` beside the host's bundle.
    path: PathBuf,
    /// The host's signing certificate.
    certificate: Vec<u8>,
}

impl ProcessRequesters {
    /// `host_exe`: the host's own executable; `host`: its own signature
    /// ([`cupper_sys::own_signing_identity`]). Outside the app, or signed ad
    /// hoc, no requester is the app's CLI.
    pub fn new(readers: Readers, host_exe: &Path, host: Option<SigningIdentity>) -> Self {
        let certificate = host.and_then(|host| host.leaf_certificate);
        let bundled = bundled_cli(host_exe).zip(certificate);
        let bundled = bundled.map(|(path, certificate)| Arc::new(Bundled { path, certificate }));
        Self { readers, bundled }
    }
}

impl Requesters for ProcessRequesters {
    fn examine(&self, token: AuditToken) -> BoxFuture<'static, Examined> {
        let (readers, bundled) = (self.readers.clone(), self.bundled.clone());
        async move {
            let found = match readers.read(live_adapters).await {
                Ok(adapters) => {
                    let examined = move || examine(&token, &adapters, bundled.as_deref());
                    let examined = tokio::task::spawn_blocking(examined).await;
                    examined.unwrap_or_else(|_| Err(unchecked()))
                }
                Err(_) => Err(unchecked()),
            };
            match found {
                Ok(verified) => Examined::Clear(verified),
                Err(signals) => Examined::Descendant { signals },
            }
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
fn examine(token: &AuditToken, adapters: &[LiveAdapter], bundled: Option<&Bundled>) -> Finding {
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
    let (Ok(executable), Ok(_), false) =
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
    let identity = token.signing_identity().ok().flatten();
    let bundled_cli =
        bundled.is_some_and(|bundled| is_bundled(&executable, identity.as_ref(), bundled));
    let signing = identity.map(|id| match id.team_id {
        Some(team) => format!("{} ({team})", id.identifier),
        None => id.identifier,
    });
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

/// The app's own CLI: at the path beside the host's bundle (compared as
/// text), running code with the CLI's identifier and the host's own signing
/// certificate. Ad-hoc code has no certificate, so it never is.
fn is_bundled(executable: &Path, identity: Option<&SigningIdentity>, bundled: &Bundled) -> bool {
    identity.is_some_and(|identity| {
        let certificate = identity.leaf_certificate.as_ref();
        executable == bundled.path
            && identity.identifier == CLI_IDENTIFIER
            && certificate == Some(&bundled.certificate)
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
        // No adapter runs; this ad-hoc signed process is no app's CLI, even
        // where the app's CLI would be.
        let certificate = vec![1];
        let bundled = Bundled {
            path: exe.clone(),
            certificate,
        };
        let verified = examine(&token, &[], Some(&bundled)).unwrap();
        assert_eq!(verified.pid, std::process::id());
        assert_eq!(verified.executable.0, exe.display().to_string());
        assert!(!verified.bundled_cli && verified.signing.is_some());
        let parent = std::os::unix::process::parent_id();
        assert_eq!(verified.parent_chain[0].pid, parent);
        // The test's parent as an adapter: this process descends from it.
        let parent = cupper_sys::process_info(parent as i32).unwrap();
        let (pid, pgid, start) = (parent.pid, parent.pgid, parent.start);
        let adapter = LiveAdapter { pid, pgid, start };
        let signals = examine(&token, &[adapter], None).unwrap_err();
        assert!(signals.contains(&"ancestor".to_owned()), "{signals:?}");
        drop(theirs);
    }

    #[test]
    fn bundled_cli_needs_the_path_identifier_and_host_certificate() {
        let path = PathBuf::from("/A/Cupper.app/Contents/Helpers/cupper");
        let bundled = Bundled {
            path: path.clone(),
            certificate: vec![7],
        };
        let identity = |identifier: &str, certificate: Option<u8>| SigningIdentity {
            identifier: identifier.to_owned(),
            team_id: None,
            leaf_certificate: certificate.map(|byte| vec![byte]),
        };
        let other = Path::new("/usr/local/bin/cupper");
        let cases = [
            (
                path.as_path(),
                Some(identity(CLI_IDENTIFIER, Some(7))),
                true,
            ),
            (other, Some(identity(CLI_IDENTIFIER, Some(7))), false),
            (&path, Some(identity("dev.cupper.app", Some(7))), false),
            (&path, Some(identity(CLI_IDENTIFIER, Some(8))), false),
            // Ad hoc: no certificate.
            (&path, Some(identity(CLI_IDENTIFIER, None)), false),
            (&path, None, false),
        ];
        for (n, (executable, identity, expected)) in cases.into_iter().enumerate() {
            assert_eq!(
                is_bundled(executable, identity.as_ref(), &bundled),
                expected,
                "{n}"
            );
        }
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
