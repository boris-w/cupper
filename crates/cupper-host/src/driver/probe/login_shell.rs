//! The login shell's environment (design 5.6.6, Q18). A GUI process's own
//! PATH lacks Homebrew, nvm and the like (SP-2), so the host asks the user's
//! login shell, at start and on `agents/refresh`. rc files may print anything,
//! so the environment comes between two random markers, NUL-separated
//! (`env -0`). The values stay in memory: nothing here logs them, and
//! [`LoginEnv`]'s `Debug` shows names only, so they cannot reach `host.log`
//! or a diagnostics bundle by accident.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use cupper_sys::{Command, Env};

use super::run::{RunError, run};
use crate::clock::Clock;
use crate::durations::{Durations, LOGIN_SHELL};

/// The PATH to use when the login shell gives none.
pub const FALLBACK_PATH: &str = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin";

/// Kept whatever the caller asks for: PATH, the proxies, the SSH agent and the
/// agents' configuration directories (design 5.6.6).
const KEPT: &[&str] = &[
    "PATH",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "no_proxy",
    "SSH_AUTH_SOCK",
    "CLAUDE_CONFIG_DIR",
    "CODEX_HOME",
];

/// What the login shell gave. A `test-harness` host that has
/// `test_config.login_env` builds it from that map, with no failure, instead
/// of running the shell.
#[derive(Clone, PartialEq, Eq)]
pub struct LoginEnv {
    pub vars: BTreeMap<String, String>,
    /// Why `vars` holds only [`FALLBACK_PATH`]; `None` when the shell answered.
    pub failure: Option<Failure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// There is no such shell, or it cannot be started.
    ShellMissing,
    Timeout,
    /// The output has no environment between the markers.
    NoEnvironment,
    /// Anything else: too much output, a failed read.
    Failed,
}

impl Failure {
    /// The semantic code for the agents status.
    pub fn code(self) -> &'static str {
        match self {
            Self::ShellMissing => "login_shell_missing",
            Self::Timeout => "login_shell_timeout",
            Self::NoEnvironment => "login_shell_no_environment",
            Self::Failed => "login_shell_failed",
        }
    }
}

impl LoginEnv {
    pub fn fallback(failure: Failure) -> Self {
        let vars = BTreeMap::from([("PATH".to_owned(), FALLBACK_PATH.to_owned())]);
        Self {
            vars,
            failure: Some(failure),
        }
    }
}

impl fmt::Debug for LoginEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoginEnv")
            .field("names", &self.vars.keys().collect::<Vec<_>>())
            .field("failure", &self.failure)
            .finish()
    }
}

/// Runs `shell -l -i -c <script>` with `base` as its environment and keeps
/// `PATH`, the proxies and the like, and the names matching `wanted` (a
/// name, or a prefix ending in `*` such as `LC_*`).
pub async fn read(
    clock: &dyn Clock,
    durations: &Durations,
    shell: &Path,
    base: Env,
    wanted: &[&str],
) -> LoginEnv {
    let marker = format!("cupper-env-{:016x}{:016x}", random(), random());
    let (begin, end) = (format!("{marker}-begin"), format!("{marker}-end"));
    let script = format!("printf '%s' {begin}; /usr/bin/env -0; printf '%s' {end}");
    let mut command = Command::new(shell);
    command.args(["-l", "-i", "-c", &script]).env(base);
    let output = match run(clock, command, durations.get(LOGIN_SHELL)).await {
        Ok(output) => output.stdout,
        Err(RunError::Spawn(_)) => return LoginEnv::fallback(Failure::ShellMissing),
        Err(RunError::Timeout) => return LoginEnv::fallback(Failure::Timeout),
        Err(_) => return LoginEnv::fallback(Failure::Failed),
    };
    let Some(entries) = between(&output, begin.as_bytes(), end.as_bytes()) else {
        return LoginEnv::fallback(Failure::NoEnvironment);
    };
    let keep = |name: &str| {
        KEPT.iter()
            .chain(wanted)
            .any(|pattern| match pattern.strip_suffix('*') {
                Some(prefix) => name.starts_with(prefix),
                None => name == *pattern,
            })
    };
    let vars = entries
        .split(|byte| *byte == 0)
        .filter_map(|entry| std::str::from_utf8(entry).ok()?.split_once('='))
        .filter(|(name, _)| keep(name))
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect();
    LoginEnv {
        vars,
        failure: None,
    }
}

fn between<'a>(output: &'a [u8], begin: &[u8], end: &[u8]) -> Option<&'a [u8]> {
    let find = |haystack: &[u8], needle: &[u8]| {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    };
    let start = find(output, begin)? + begin.len();
    let len = find(&output[start..], end)?;
    Some(&output[start..start + len])
}

fn random() -> u64 {
    getrandom::u64().expect("the operating system's random source is available")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::clock::ManualClock;
    use crate::driver::probe::testkit::script;
    use crate::store::testutil::TempDir;

    const SECRET: &str = "s3cr3t-token-value";

    /// A fake login shell: `before`, then the `-c` script, then `after`. It
    /// exports a few variables as an rc file would.
    fn shell(dir: &Path, before: &str, after: &str) -> std::path::PathBuf {
        let body = format!(
            "export PATH=/fake/bin:/usr/bin CODEX_HOME=/fake/codex LC_ALL=C \
             API_TOKEN={SECRET} UNWANTED=x\n{before}\n/bin/sh -c \"$4\"\n{after}"
        );
        script(dir, "shell", &body)
    }

    async fn read_with(clock: &ManualClock, shell: &Path) -> LoginEnv {
        let wanted = ["LC_*", "API_TOKEN"];
        read(clock, &Durations::default(), shell, Env::new(), &wanted).await
    }

    #[tokio::test]
    async fn unit_login_shell_keeps_the_wanted_names_whatever_rc_files_print() {
        let dir = TempDir::new("login-shell");
        let clock = ManualClock::new();
        for (before, after) in [("", ""), ("echo welcome; printf 'A=1\\0'", "echo bye")] {
            let env = read_with(&clock, &shell(dir.path(), before, after)).await;
            let names: Vec<&str> = env.vars.keys().map(String::as_str).collect();
            assert_eq!(names, ["API_TOKEN", "CODEX_HOME", "LC_ALL", "PATH"]);
            assert_eq!(env.vars["PATH"], "/fake/bin:/usr/bin");
            assert_eq!(env.vars["API_TOKEN"], SECRET);
            assert_eq!(env.failure, None);
            // The values never reach a log line through `Debug`.
            let shown = format!("{env:?}");
            assert!(
                !shown.contains(SECRET) && !shown.contains("/fake/bin"),
                "{shown}"
            );
        }
    }

    #[tokio::test]
    async fn unit_login_shell_falls_back_without_an_environment() {
        let dir = TempDir::new("login-shell-fail");
        let clock = ManualClock::new();
        let quiet = script(dir.path(), "quiet", "echo no environment here");
        let missing = dir.path().join("no-such-shell");
        for (shell, failure) in [
            (quiet, Failure::NoEnvironment),
            (missing, Failure::ShellMissing),
        ] {
            let env = read_with(&clock, &shell).await;
            assert_eq!(env, LoginEnv::fallback(failure));
            assert_eq!(env.vars["PATH"], FALLBACK_PATH);
        }
        assert_eq!(Failure::NoEnvironment.code(), "login_shell_no_environment");
    }

    #[tokio::test]
    async fn unit_login_shell_a_hanging_shell_is_killed_at_the_timeout() {
        let dir = TempDir::new("login-shell-hang");
        let pid_file = dir.path().join("pid");
        let body = format!("echo $$ > {}\nexec /bin/sleep 600", pid_file.display());
        let shell = script(dir.path(), "hang", &body);
        let clock = Arc::new(ManualClock::new());
        let reading = tokio::spawn({
            let clock = Arc::clone(&clock);
            async move { read_with(&clock, &shell).await }
        });
        let started = || std::fs::read_to_string(&pid_file).is_ok_and(|pid| pid.ends_with('\n'));
        while clock.next_deadline().is_none() || !started() {
            tokio::task::yield_now().await;
        }
        clock.advance(Duration::from_secs(5));
        let env = reading.await.expect("the read ends");
        assert_eq!(env, LoginEnv::fallback(Failure::Timeout));
        let pid = std::fs::read_to_string(&pid_file).expect("pid");
        let pid: i32 = pid.trim().parse().expect("a pid");
        assert!(
            cupper_sys::process_info(pid).is_none(),
            "the shell was killed"
        );
    }
}
