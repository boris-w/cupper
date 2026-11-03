//! The file service (design 5.2.11): `files/search` offers a workspace's
//! files for `@` references ([`search`]); `workspace/open` hands a session's
//! workspace to the terminal or the editor its settings name, from the
//! built-in lists, or to Finder (UI-0), through `open -a`, its arguments
//! passed as they are.

pub mod search;

use std::path::PathBuf;
use std::sync::Arc;

use cupper_protocol::error::{ErrorCode, ErrorReason, INVALID_PARAMS, RpcError};
use cupper_protocol::methods::git::{
    FilesSearch, FilesSearchParams, FilesSearchResult, OpenTarget, WorkspaceOpenParams,
};
use cupper_protocol::rpc::Method;
use futures::FutureExt;
use rusqlite::OptionalExtension;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::clock::Clock;
use crate::durations::{APP_OPEN, Durations};
use crate::gateway::{Call, Port, Reply, Routes};
use crate::git::GitTool;
use crate::git::run::run;
use crate::settings::{EDITORS, SettingsHandle, TERMINALS};
use crate::store::Readers;
pub use search::FileLists;

/// A query gets this many matches unless it asks for fewer.
const LIMIT: u32 = 50;
/// And at most this many.
const MAX_LIMIT: u32 = 100;

#[derive(Debug, Clone)]
pub struct FilesDeps {
    /// `None` when no trusted `PATH` entry has a git: every workspace is walked.
    pub git: Option<GitTool>,
    pub readers: Readers,
    pub clock: Arc<dyn Clock>,
    pub durations: Durations,
    pub settings: SettingsHandle,
    /// The sessions drop a workspace's list when a turn ends.
    pub lists: FileLists,
    /// `/usr/bin/open`; a stand-in in tests.
    pub opener: PathBuf,
}

#[derive(Debug, Clone)]
pub struct FilesService(Arc<FilesDeps>);

impl FilesService {
    pub fn new(deps: FilesDeps) -> Self {
        Self(Arc::new(deps))
    }

    pub fn route(self, routes: &mut Routes) {
        routes.files(Arc::new(self));
    }

    async fn search(&self, params: FilesSearchParams) -> Result<FilesSearchResult, RpcError> {
        let sql = match (&params.session_id, &params.project_id) {
            (Some(_), None) => "SELECT workspace_path FROM sessions WHERE session_id = ?1",
            (None, Some(_)) => {
                "SELECT path FROM projects WHERE project_id = ?1 AND removed_at IS NULL"
            }
            _ => {
                return Err(RpcError::new(
                    INVALID_PARAMS,
                    "one of session_id and project_id",
                ));
            }
        };
        let id = params.session_id.map(|s| s.to_string());
        let id = id
            .or(params.project_id.map(|p| p.to_string()))
            .unwrap_or_default();
        let workspace = self.path(sql, id).await?;
        let (git, clock, durations) = (self.0.git.as_ref(), &*self.0.clock, &self.0.durations);
        let files = self.0.lists.of(&workspace, git, clock, durations).await;
        let limit = params.limit.unwrap_or(LIMIT).min(MAX_LIMIT);
        let results = search::best(&files, &params.query, limit as usize);
        Ok(FilesSearchResult { results })
    }

    /// `open -a <app> <workspace>`; `open` fails when the app is not there.
    async fn open(&self, params: WorkspaceOpenParams) -> Result<Value, RpcError> {
        let sql = "SELECT workspace_path FROM sessions WHERE session_id = ?1";
        let workspace = self.path(sql, params.session_id.to_string()).await?;
        let settings = self.0.settings.current();
        let (app, apps) = match params.target {
            OpenTarget::Terminal => (settings.terminal_app, TERMINALS),
            OpenTarget::Editor => (settings.editor_app, EDITORS),
            OpenTarget::Finder => ("Finder".into(), &["Finder"][..]),
            OpenTarget::Unknown(target) => return Err(RpcError::new(INVALID_PARAMS, target)),
        };
        let not_found = || {
            let error = RpcError::app(ErrorCode::Unavailable, format!("{app} is not there"));
            error.with_reason(ErrorReason::AppNotFound)
        };
        if !apps.contains(&app.as_str()) {
            return Err(not_found());
        }
        if !workspace.is_dir() {
            return Err(RpcError::app(ErrorCode::NotFound, "the workspace is gone"));
        }
        let mut command = cupper_sys::Command::new(&self.0.opener);
        command.args(["-a".as_ref(), app.as_ref(), workspace.as_os_str()]);
        let limit = self.0.durations.get(APP_OPEN);
        let output = run(&*self.0.clock, command, None, limit).await;
        match output.map_err(internal)?.status.success() {
            true => Ok(json!({})),
            false => Err(not_found()),
        }
    }

    /// The path `sql` finds for `id`.
    async fn path(&self, sql: &'static str, id: String) -> Result<PathBuf, RpcError> {
        let read = move |c: &rusqlite::Connection| {
            c.query_row(sql, [id], |r| r.get::<_, String>(0)).optional()
        };
        let path = self.0.readers.read(read).await.map_err(internal)?;
        let missing = || RpcError::app(ErrorCode::NotFound, "no such session or project");
        path.map(PathBuf::from).ok_or_else(missing)
    }
}

impl Port for FilesService {
    fn deliver(&self, call: Call) -> Reply {
        let service = self.clone();
        async move {
            match call.method {
                FilesSearch::NAME => Ok(json!(service.search(parse(&call.params)?).await?)),
                _ => service.open(parse(&call.params)?).await,
            }
        }
        .boxed()
    }
}

fn parse<T: DeserializeOwned>(params: &Value) -> Result<T, RpcError> {
    T::deserialize(params).map_err(|err| RpcError::new(INVALID_PARAMS, err.to_string()))
}

fn internal(err: impl std::fmt::Display) -> RpcError {
    RpcError::app(ErrorCode::Internal, err.to_string())
}
