//! The environment an adapter starts with (design 5.6.6, security #9,
//! minutes D14), layered as follows; the lists are those of the embedded
//! manifest, of which the host keeps no copy:
//!
//! 1. the fixed allowlist `env.allow`: the login shell's value, else the
//!    host's own; `PATH` only from the login shell;
//! 2. `agents.env_passthrough`: names matching `^[A-Z][A-Z0-9_]*$`, neither
//!    denied (`env.deny`) nor on the allowlist, valued by the login shell;
//! 3. set by the host: the native binary and the checked config dir.
//!
//! `CUPPER_*` never passes: it is denied, and `cupper_sys::Env` refuses it.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use cupper_sys::Env;

use super::trust::Boundaries;
use crate::manifest::{self, env_pattern_matches};

/// The variables the host sets for one agent's adapter.
#[derive(Debug, Clone, Copy)]
pub struct AgentVars {
    pub executable: &'static str,
    pub config_dir: &'static str,
}

pub const CLAUDE: AgentVars = AgentVars {
    executable: "CLAUDE_CODE_EXECUTABLE",
    config_dir: "CLAUDE_CONFIG_DIR",
};

pub const CODEX: AgentVars = AgentVars {
    executable: "CODEX_PATH",
    config_dir: "CODEX_HOME",
};

/// A config dir that passed [`check_config_dir`], as the login shell gave
/// it: the CLI sees the same path as in the user's terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigDir(String);

impl ConfigDir {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// A config dir from `test_config`, unchecked (design 5.6.6).
    #[cfg(any(test, feature = "test-harness"))]
    pub fn from_test_config(value: String) -> Self {
        Self(value)
    }
}

/// Why a config dir is not passed on; [`ConfigDirRejected::code`] is the
/// semantic code for the agent status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigDirRejected {
    NotAbsolute,
    /// Not an existing directory.
    Missing,
    NotOwned,
    InsideProject,
}

impl ConfigDirRejected {
    pub fn code(self) -> &'static str {
        match self {
            Self::NotAbsolute => "config_dir_not_absolute",
            Self::Missing => "config_dir_missing",
            Self::NotOwned => "config_dir_not_owned",
            Self::InsideProject => "config_dir_inside_project",
        }
    }
}

/// Checks `CLAUDE_CONFIG_DIR` or `CODEX_HOME` from the login shell: an
/// absolute path to a directory of user `uid`, outside every project and
/// workspace both by realpath and where the path itself is (a symlink in a
/// project could be retargeted later). Temp dirs are allowed.
pub fn check_config_dir(
    value: &str,
    uid: u32,
    boundaries: &Boundaries,
) -> Result<ConfigDir, ConfigDirRejected> {
    let path = Path::new(value);
    if !path.is_absolute() {
        return Err(ConfigDirRejected::NotAbsolute);
    }
    let meta = fs::metadata(path).map_err(|_| ConfigDirRejected::Missing)?;
    let real = fs::canonicalize(path).map_err(|_| ConfigDirRejected::Missing)?;
    if !meta.is_dir() {
        return Err(ConfigDirRejected::Missing);
    }
    if meta.uid() != uid {
        return Err(ConfigDirRejected::NotOwned);
    }
    let parent = path
        .parent()
        .and_then(|parent| fs::canonicalize(parent).ok());
    let link = parent
        .zip(path.file_name())
        .map(|(dir, name)| dir.join(name));
    if !boundaries.outside_roots(&real) || link.is_some_and(|l| !boundaries.outside_roots(&l)) {
        return Err(ConfigDirRejected::InsideProject);
    }
    Ok(ConfigDir(value.to_owned()))
}

/// A passthrough name that was not passed on; [`EnvNote::code`] is the
/// semantic code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvNote {
    InvalidName(String),
    Denied(String),
    /// On the fixed allowlist, which a passthrough cannot override.
    Fixed(String),
    /// The login shell has no value for it.
    Unset(String),
}

impl EnvNote {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidName(_) => "passthrough_invalid_name",
            Self::Denied(_) => "passthrough_denied",
            Self::Fixed(_) => "passthrough_fixed",
            Self::Unset(_) => "passthrough_unset",
        }
    }

    pub fn name(&self) -> &str {
        let (Self::InvalidName(name) | Self::Denied(name) | Self::Fixed(name) | Self::Unset(name)) =
            self;
        name
    }
}

/// The environment of an adapter of `agent` that runs `binary`. `host` is
/// the host's own environment, `login` what the login shell gave.
pub fn adapter_env(
    agent: AgentVars,
    host: &BTreeMap<String, String>,
    login: &BTreeMap<String, String>,
    binary: &Path,
    config_dir: Option<&ConfigDir>,
    passthrough: &[String],
) -> (Env, Vec<EnvNote>) {
    let classes = &manifest::embedded().env;
    let fixed = host.iter().filter(|(name, _)| *name != "PATH").chain(login);
    let mut env = Env::from_allowed(fixed, &classes.allow);
    let listed = |list: &[String], name: &str| list.iter().any(|p| env_pattern_matches(p, name));
    let mut notes = Vec::new();
    for name in passthrough {
        let note = if !is_variable_name(name) {
            EnvNote::InvalidName
        } else if listed(&classes.deny, name) {
            EnvNote::Denied
        } else if listed(&classes.allow, name) {
            EnvNote::Fixed
        } else if let Some(value) = login.get(name)
            && env.set(name, value).is_ok()
        {
            continue;
        } else {
            EnvNote::Unset
        };
        notes.push(note(name.clone()));
    }
    let set = "the host's names are valid and paths have no NUL";
    env.set(agent.executable, binary).expect(set);
    if let Some(dir) = config_dir {
        env.set(agent.config_dir, dir.as_str()).expect(set);
    }
    (env, notes)
}

/// `^[A-Z][A-Z0-9_]*$`.
fn is_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;
    use crate::driver::locate::testkit::Tree;

    /// `NAME=value` words.
    fn vars(words: &str) -> BTreeMap<String, String> {
        let pair = |word: &str| word.split_once('=').map(|(k, v)| (k.into(), v.into()));
        words.split_whitespace().filter_map(pair).collect()
    }

    /// The environment as `NAME=value` words.
    fn words(env: &Env) -> Vec<String> {
        let word = |(k, v): (&OsStr, &OsStr)| format!("{}={}", k.display(), v.display());
        env.iter().map(word).collect()
    }

    #[test]
    fn unit_env_rules_adapter_env_layers() {
        let host =
            vars("HOME=/home/user PATH=/usr/bin LANG=C SECRET=x NODE_OPTIONS=x CUPPER_KEY=x");
        let login = vars(
            "PATH=/opt/homebrew/bin:/usr/bin LANG=en_US.UTF-8 HTTPS_PROXY=http://proxy.invalid
             ANTHROPIC_API_KEY=key CLAUDE_CONFIG_DIR=/unchecked CUPPER_HARNESS=x",
        );
        let passthrough = "ANTHROPIC_API_KEY ANTHROPIC_BASE_URL lower_case 1ST LANG PATH \
             CLAUDE_CONFIG_DIR CUPPER_HARNESS";
        let passthrough: Vec<String> = passthrough.split_whitespace().map(Into::into).collect();
        let dir = ConfigDir("/home/user/.claude-work".into());
        let binary = Path::new("/opt/homebrew/bin/claude");
        let (env, notes) = adapter_env(CLAUDE, &host, &login, binary, Some(&dir), &passthrough);
        let want = "ANTHROPIC_API_KEY=key CLAUDE_CODE_EXECUTABLE=/opt/homebrew/bin/claude
            CLAUDE_CONFIG_DIR=/home/user/.claude-work HOME=/home/user
            HTTPS_PROXY=http://proxy.invalid LANG=en_US.UTF-8 PATH=/opt/homebrew/bin:/usr/bin";
        assert_eq!(words(&env), want.split_whitespace().collect::<Vec<_>>());
        let notes: Vec<String> = notes
            .iter()
            .map(|n| format!("{}:{}", n.code(), n.name()))
            .collect();
        let want = "passthrough_unset:ANTHROPIC_BASE_URL passthrough_invalid_name:lower_case
            passthrough_invalid_name:1ST passthrough_fixed:LANG passthrough_fixed:PATH
            passthrough_denied:CLAUDE_CONFIG_DIR passthrough_denied:CUPPER_HARNESS";
        assert_eq!(notes, want.split_whitespace().collect::<Vec<_>>());
        // Codex gets its own names.
        let (env, _) = adapter_env(CODEX, &host, &login, binary, Some(&dir), &[]);
        let names: Vec<String> = env
            .iter()
            .map(|(name, _)| name.display().to_string())
            .collect();
        let want = "CODEX_HOME CODEX_PATH HOME HTTPS_PROXY LANG PATH";
        assert_eq!(names.join(" "), want);
    }

    #[test]
    fn unit_env_rules_every_denied_name_is_refused() {
        // The embedded manifest's list, not a copy: prefixes get a suffix.
        let deny = &manifest::embedded().env.deny;
        assert!(deny.iter().any(|p| p == "DYLD_*") && deny.iter().any(|p| p == "IS_SANDBOX"));
        let denied: Vec<String> = deny.iter().map(|p| p.replace('*', "X")).collect();
        let login = denied
            .iter()
            .map(|name| (name.clone(), "x".into()))
            .collect();
        let binary = Path::new("/usr/local/bin/claude");
        let (env, notes) = adapter_env(CLAUDE, &BTreeMap::new(), &login, binary, None, &denied);
        let want: Vec<EnvNote> = denied.iter().cloned().map(EnvNote::Denied).collect();
        assert_eq!(notes, want);
        assert_eq!(
            words(&env),
            ["CLAUDE_CODE_EXECUTABLE=/usr/local/bin/claude"]
        );
    }

    #[test]
    fn unit_env_rules_config_dir_checks() {
        let tree = Tree::new(
            "config-dir",
            "
            conf/claude           dir
            conf/file             plain not a dir
            tmp/claude            dir
            home/project/conf     dir
            links/into            -> ../home/project/conf
            links/out             -> ../conf/claude
            home/project/out      -> ../../conf/claude
            ",
        );
        let boundaries = Boundaries::new([tree.path("home/project")], [tree.path("tmp")]);
        let uid = cupper_sys::current_uid();
        // The value passed on is the path as given, not its realpath.
        let check = |relative: &str, uid| {
            let value = tree.path(relative).display().to_string();
            let checked = check_config_dir(&value, uid, &boundaries);
            checked
                .map(|dir| dir.as_str() == value)
                .map_err(ConfigDirRejected::code)
        };
        for ok in ["conf/claude", "tmp/claude", "links/out"] {
            assert_eq!(check(ok, uid), Ok(true), "{ok}");
        }
        let rejected = "conf/missing:config_dir_missing conf/file:config_dir_missing
            home/project/conf:config_dir_inside_project links/into:config_dir_inside_project
            home/project/out:config_dir_inside_project";
        for (relative, code) in rejected
            .split_whitespace()
            .filter_map(|w| w.split_once(':'))
        {
            assert_eq!(check(relative, uid), Err(code), "{relative}");
        }
        assert_eq!(check("conf/claude", uid + 1), Err("config_dir_not_owned"));
        let relative = check_config_dir("conf/claude", uid, &boundaries);
        assert_eq!(relative, Err(ConfigDirRejected::NotAbsolute));
    }
}
