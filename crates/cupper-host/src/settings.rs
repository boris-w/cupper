//! The host's settings (design 5.2.11): one `settings` row per key that was
//! changed, holding its JSON value, over the defaults. The host's modules
//! read them from [`SettingsHandle::current`]; `settings/get` and
//! `settings/update` serve them ([`SettingsPort`]). The gateway's policy has
//! checked an update already: a security-related key takes `privileged`
//! (design 5.2.5). A change of a CLI path or of `agents.env_passthrough`
//! finds the agents again, and `agents.updated` shows it; M1a pushes no
//! other notice of a change (T036 ruling).

use std::fmt;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cupper_protocol::annotations::WirePath;
use cupper_protocol::domain::AgentKind;
use cupper_protocol::error::{ErrorCode, INVALID_PARAMS, RpcError};
use cupper_protocol::methods::admin::{
    Settings, SettingsGet, SettingsPatch, SettingsResult, SettingsUpdate, SettingsUpdateParams,
};
use cupper_protocol::rpc::Method;
use futures::FutureExt;
use futures::future::BoxFuture;
use rusqlite::Connection;
use serde_json::{Map, Value, json};

use crate::clock::Clock;
use crate::driver::locate::env_rules::is_variable_name;
use crate::driver::runtime::agents::Agents;
use crate::gateway::{Call, Port, Reply};
use crate::ledger::Ticket;
use crate::store::{Durability, WriteError, Writer};

/// The settings nobody changed: CLIs discovered, no more variables for the
/// adapters, worktrees under `~/.cupper/worktrees`, every notification on,
/// Claude for new sessions, the idle exit after 10 minutes, no detailed
/// logs. The apps are macOS's own until the built-in list exists (T013).
pub fn defaults(home: &Path) -> Settings {
    Settings {
        claude_cli_path: None,
        codex_cli_path: None,
        env_passthrough: Vec::new(),
        worktrees_root: WirePath(home.join(".cupper/worktrees").display().to_string()),
        notify_needs_action: true,
        notify_turn_completed: true,
        notify_error: true,
        terminal_app: "Terminal".into(),
        editor_app: "TextEdit".into(),
        default_agent: AgentKind::Claude,
        idle_exit_minutes: 10,
        verbose_diagnostics: false,
    }
}

/// `settings` over `defaults`; a row of a key the host does not know, or
/// whose value does not fit, is left out.
fn merged(defaults: &Settings, rows: Vec<(String, String)>) -> Settings {
    let mut settings = defaults.clone();
    for (key, value) in rows {
        let Ok(value) = serde_json::from_str::<Value>(&value) else {
            continue;
        };
        let mut object = object(&settings);
        if object.contains_key(&key) {
            object.insert(key, value);
            if let Ok(fits) = serde_json::from_value(Value::Object(object)) {
                settings = fits;
            }
        }
    }
    settings
}

fn object(settings: &Settings) -> Map<String, Value> {
    match json!(settings) {
        Value::Object(object) => object,
        _ => unreachable!("settings are an object"),
    }
}

/// `settings` with `patch` applied and checked; the keys it sets, with
/// their values.
pub fn apply(
    settings: &Settings,
    patch: &SettingsPatch,
) -> Result<(Settings, Map<String, Value>), String> {
    let Value::Object(set) = json!(patch) else {
        unreachable!("a patch is an object")
    };
    let mut object = object(settings);
    object.extend(set.clone());
    let next: Settings =
        serde_json::from_value(Value::Object(object)).map_err(|e| e.to_string())?;
    let cli_paths = [&next.claude_cli_path, &next.codex_cli_path];
    if let Some(path) = cli_paths
        .into_iter()
        .flatten()
        .find(|p| !Path::new(&p.0).is_absolute())
    {
        return Err(format!("{} is not an absolute path", path.0));
    }
    if !Path::new(&next.worktrees_root.0).is_absolute() {
        return Err("worktrees.root must be an absolute path".into());
    }
    if let Some(name) = next.env_passthrough.iter().find(|n| !is_variable_name(n)) {
        return Err(format!("{name} is not an environment variable name"));
    }
    if next.idle_exit_minutes == 0 {
        return Err("host.idle_exit_minutes must be at least 1".into());
    }
    if [&next.terminal_app, &next.editor_app]
        .iter()
        .any(|app| app.is_empty())
    {
        return Err("an app must be named".into());
    }
    Ok((next, set))
}

/// The settings in memory, for every module; clones share them.
#[derive(Debug, Clone)]
pub struct SettingsHandle {
    shared: Arc<Shared>,
}

#[derive(Debug)]
struct Shared {
    current: Mutex<Settings>,
    /// One update at a time, from the check to the commit.
    update: tokio::sync::Mutex<()>,
    writer: Writer,
    clock: Arc<dyn Clock>,
}

impl SettingsHandle {
    /// The settings `host.db` holds, read on `conn` at start, over the
    /// defaults for `home`.
    pub fn load(
        conn: &Connection,
        writer: Writer,
        clock: Arc<dyn Clock>,
        home: &Path,
    ) -> rusqlite::Result<Self> {
        let mut select = conn.prepare("SELECT key, value FROM settings")?;
        let rows = select.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let rows = rows.collect::<rusqlite::Result<Vec<(String, String)>>>()?;
        let current = merged(&defaults(home), rows);
        let shared = Shared {
            current: Mutex::new(current),
            update: tokio::sync::Mutex::new(()),
            writer,
            clock,
        };
        Ok(Self {
            shared: Arc::new(shared),
        })
    }

    pub fn current(&self) -> Settings {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, Settings> {
        self.shared
            .current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// `settings/update`: the keys of `patch` written, with the command's
    /// record, in one transaction; then they are in effect. Returns the
    /// settings now, and whether the agents' discovery reads a key that
    /// changed.
    pub async fn update(
        &self,
        patch: &SettingsPatch,
        ticket: Option<Ticket>,
    ) -> Result<(Settings, bool), RpcError> {
        let _one = self.shared.update.lock().await;
        let before = self.current();
        let (next, set) = match apply(&before, patch) {
            Ok(applied) => applied,
            Err(message) => {
                let err = RpcError::new(INVALID_PARAMS, message);
                if let Some(ticket) = ticket {
                    ticket.reject(err.clone());
                }
                return Err(err);
            }
        };
        let result = json!(SettingsResult {
            settings: next.clone()
        });
        let completion = ticket.map(|ticket| ticket.complete(Ok(result)));
        let record = completion.as_ref().map(|completion| completion.row());
        let at = self.shared.clock.now_wall_ms();
        let write = self.shared.writer.submit(Durability::Normal, move |conn| {
            for (key, value) in &set {
                conn.execute(
                    "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
                     ON CONFLICT (key) DO UPDATE SET value = excluded.value,
                         updated_at = excluded.updated_at",
                    (key, value.to_string(), at),
                )?;
            }
            record.map_or(Ok(()), |record| record(conn))
        });
        let internal = |err: WriteError| RpcError::app(ErrorCode::Internal, err.to_string());
        write.await.map_err(internal)?;
        *self.lock() = next.clone();
        if let Some(completion) = completion {
            completion.committed();
        }
        let agents = (before.claude_cli_path != next.claude_cli_path)
            || (before.codex_cli_path != next.codex_cli_path)
            || (before.env_passthrough != next.env_passthrough);
        Ok((next, agents))
    }
}

/// Finds the agents again: `agents/refresh` of Claude.
pub type Rediscover = Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>;

/// `settings/get` and `settings/update`.
#[derive(Clone)]
pub struct SettingsPort {
    pub settings: SettingsHandle,
    pub rediscover: Rediscover,
}

impl fmt::Debug for SettingsPort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SettingsPort").finish_non_exhaustive()
    }
}

impl SettingsPort {
    /// The port, finding `agents` again when an update asks for it.
    pub fn new(settings: SettingsHandle, agents: Arc<Agents>) -> Self {
        let rediscover: Rediscover = Arc::new(move || {
            let agents = Arc::clone(&agents);
            // A failure shows in `agents.updated`; the settings stand.
            async move { drop(agents.refresh(&AgentKind::Claude).await) }.boxed()
        });
        Self {
            settings,
            rediscover,
        }
    }
}

impl Port for SettingsPort {
    fn deliver(&self, call: Call) -> Reply {
        let port = self.clone();
        async move {
            if call.method == SettingsGet::NAME {
                let settings = port.settings.current();
                return Ok(json!(SettingsResult { settings }));
            }
            debug_assert_eq!(call.method, SettingsUpdate::NAME);
            let params: SettingsUpdateParams = serde_json::from_value(call.params)
                .map_err(|err| RpcError::new(INVALID_PARAMS, err.to_string()))?;
            let (settings, agents) = port.settings.update(&params.patch, call.ticket).await?;
            if agents {
                (port.rediscover)().await;
            }
            Ok(json!(SettingsResult { settings }))
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests;
