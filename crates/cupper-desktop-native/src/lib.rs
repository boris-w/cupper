//! `cupper_native.node`: the napi-rs extension through which the desktop's
//! main process starts the host, connects to it and checks that it is the
//! host of its build (design 5.1.3, 5.7.2, 5.8.1). `ts/index.ts` declares
//! the module for TypeScript.
//!
//! The build kind is a compile-time feature (design 5.7.9). A release build
//! (no features) accepts a host that satisfies the code requirement. A
//! `dev-auth` build (development, self-use, e2e) starts the host with a
//! one-time key, and the two sides prove the key to each other in the
//! handshake; the main process's `handshake(port, options)` (host-client)
//! gets `auth` and `verifyHostProof` from [`HostConnection`].
//!
//! Errors thrown to JavaScript carry a semantic `code`.

mod check;
#[cfg(feature = "dev-auth")]
mod key;
mod launch;
mod relay;

use std::collections::HashMap;
use std::fmt::Display;
use std::os::fd::{IntoRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::ExitStatusExt;
use std::sync::Arc;

use cupper_sys::{Child, Stdio};
use napi::bindgen_prelude::{Env, Error, Object};
use napi_derive::napi;

use crate::check::{BUILD_CHECK, HostCheck};
use crate::relay::{CloseReason, Relay};

/// The `code` of an error thrown to JavaScript.
type Code = &'static str;

fn fail(code: Code, err: impl Display) -> Error<Code> {
    Error::new(code, err.to_string())
}

/// The features of this build, in a fixed order; none in a release build.
const FEATURES: &[&str] = &[
    #[cfg(feature = "dev-auth")]
    "dev-auth",
];

#[napi(object)]
pub struct BuildInfo {
    pub features: Vec<String>,
    /// `signature` or `key_proof` (design 5.8.1).
    pub host_check: String,
}

/// What this build is, for the main process to check against its own flavor
/// and for the release pipeline (design 5.10.7).
#[napi]
pub fn build_info() -> BuildInfo {
    let host_check = match BUILD_CHECK {
        HostCheck::Signature(_) => "signature",
        HostCheck::KeyProof => "key_proof",
    };
    BuildInfo {
        features: FEATURES.iter().map(|name| name.to_string()).collect(),
        host_check: host_check.into(),
    }
}

/// Where the host of `dataDir` listens, by the rule it binds with (design
/// 5.2.13 step 8).
#[napi]
pub fn socket_path(data_dir: String) -> napi::Result<String, Code> {
    let temp = cupper_sys::user_temp_dir().map_err(|err| fail("unexpected", err))?;
    let path = cupper_protocol::transport::socket_path(data_dir.as_ref(), &temp);
    Ok(path.to_string_lossy().into_owned())
}

/// The executable a process runs and its cdhash: what tells the host of this
/// build from anything else at a pid (restarting the host, design 5.8.1).
#[napi(object)]
pub struct ProcessImage {
    pub path: String,
    pub cdhash: String,
}

/// `None` when `pid` names no process of this user.
#[napi]
pub fn process_image(pid: i32) -> Option<ProcessImage> {
    let path = cupper_sys::pid_path(pid).ok()?;
    let cdhash = cupper_sys::pid_cdhash(pid).ok()?;
    Some(ProcessImage {
        path: path.to_string_lossy().into_owned(),
        cdhash: cdhash.to_string(),
    })
}

/// The cdhash of the signed code at `path`, a bundle or an executable.
#[napi]
pub fn file_cdhash(path: String) -> napi::Result<String, Code> {
    let cdhash =
        cupper_sys::file_cdhash(path.as_ref()).map_err(|err| fail("invalid_argument", err))?;
    Ok(cdhash.to_string())
}

/// How to start the host (design 5.7.2, 5.8.5).
#[napi(object)]
pub struct HostCommand {
    /// An absolute path; there is no `PATH` search.
    pub program: String,
    pub args: Vec<String>,
    /// The host's whole environment; `CUPPER_*` names are refused.
    pub env: HashMap<String, String>,
    /// A pipe instead of `/dev/null`; the caller owns our end's fd.
    pub pipe_stdout: Option<bool>,
    pub pipe_stderr: Option<bool>,
}

impl HostCommand {
    fn command(&self) -> napi::Result<cupper_sys::Command, Code> {
        let mut env = cupper_sys::Env::new();
        for (name, value) in &self.env {
            env.set(name, value)
                .map_err(|err| fail("invalid_argument", err))?;
        }
        let stdio = |pipe: Option<bool>| match pipe {
            Some(true) => Stdio::Pipe,
            _ => Stdio::Null,
        };
        let mut cmd = cupper_sys::Command::new(&self.program);
        cmd.args(&self.args)
            .env(env)
            .stdout(stdio(self.pipe_stdout))
            .stderr(stdio(self.pipe_stderr));
        Ok(cmd)
    }
}

/// A started host. `exit` resolves to `{code, signal}` once it has exited.
#[napi(object, object_from_js = false)]
pub struct HostProcess<'env> {
    pub pid: i32,
    /// Fds of our ends of the pipes asked for; the caller owns them.
    pub stdout: Option<i32>,
    pub stderr: Option<i32>,
    pub exit: Object<'env>,
}

#[napi(object)]
pub struct HostExit {
    pub code: Option<i32>,
    pub signal: Option<i32>,
}

/// Starts the host in a session of its own. A release build writes no
/// startup payload: the host's stdin is `/dev/null` (design 5.8.1).
#[cfg(not(feature = "dev-auth"))]
#[napi]
pub fn spawn_host(env: &Env, command: HostCommand) -> napi::Result<HostProcess<'_>, Code> {
    started(env, launch::start(&mut command.command()?, None))
}

/// Starts the host in a session of its own, with the startup payload on
/// stdin: `key` as `dev_key`, and the entries of `payload` (e2e builds:
/// `harness_key`, `test_config`; design 5.8.1).
#[cfg(feature = "dev-auth")]
#[napi]
pub fn spawn_host<'env>(
    env: &'env Env,
    command: HostCommand,
    key: &HostKey,
    payload: Option<serde_json::Map<String, serde_json::Value>>,
) -> napi::Result<HostProcess<'env>, Code> {
    let line = key
        .key
        .payload(payload.unwrap_or_default())
        .map_err(|err| fail("invalid_argument", err))?;
    started(env, launch::start(&mut command.command()?, Some(&line)))
}

fn started(env: &Env, child: cupper_sys::Result<Child>) -> napi::Result<HostProcess<'_>, Code> {
    let mut child = child.map_err(|err| fail("spawn_failed", err))?;
    let (pid, stdout, stderr) = (child.pid(), child.stdout.take(), child.stderr.take());
    let (deferred, exit) = env
        .create_deferred()
        .map_err(|err| fail("unexpected", err))?;
    launch::on_exit(child, move |status| match status {
        Ok(status) => deferred.resolve(move |_| {
            let (code, signal) = (status.code(), status.signal());
            Ok(HostExit { code, signal })
        }),
        Err(err) => deferred.reject(Error::from_reason(err.to_string())),
    })
    .map_err(|err| fail("spawn_failed", err))?;
    let fd = |end: Option<OwnedFd>| end.map(IntoRawFd::into_raw_fd);
    Ok(HostProcess {
        pid,
        stdout: fd(stdout),
        stderr: fd(stderr),
        exit,
    })
}

/// The one-time key of `dev_key` authentication, from the system's random
/// source. JavaScript only holds this handle: the key itself never leaves
/// the extension (design 5.8.1).
#[cfg(feature = "dev-auth")]
#[napi]
pub struct HostKey {
    key: key::DevKey,
}

#[cfg(feature = "dev-auth")]
#[napi]
impl HostKey {
    #[napi(constructor)]
    pub fn new() -> napi::Result<Self, Code> {
        let key = key::DevKey::generate().map_err(|err| fail("unexpected", err))?;
        Ok(Self { key })
    }
}

/// Connects to the host at `socketPath` and checks it by its code signature
/// (design 5.8.1) before anything is sent.
#[cfg(not(feature = "dev-auth"))]
#[napi]
pub fn connect_host(socket_path: String) -> napi::Result<HostConnection, Code> {
    let (relay, fd) = connect(&socket_path)?;
    Ok(HostConnection {
        relay,
        fd: Some(fd),
    })
}

/// Connects to the host at `socketPath`, started with `key`. The check on
/// connect only asks for this user: the host proves the key in `initialize`
/// (`verifyHostProof`), and nothing but `initialize` goes before that.
#[cfg(feature = "dev-auth")]
#[napi]
pub fn connect_host(socket_path: String, key: &HostKey) -> napi::Result<HostConnection, Code> {
    let (relay, fd) = connect(&socket_path)?;
    Ok(HostConnection {
        relay,
        fd: Some(fd),
        key: key.key.clone(),
        channel: None,
    })
}

fn connect(socket_path: &str) -> napi::Result<(Arc<Relay>, OwnedFd), Code> {
    let host = UnixStream::connect(socket_path).map_err(|err| fail("connect_failed", err))?;
    let pinned = BUILD_CHECK
        .verify(&host)
        .map_err(|err| fail(err.code(), err))?;
    Relay::start(host, pinned).map_err(|err| fail("unexpected", err))
}

/// A connection to a host that passed the check on connect (design 5.7.2).
/// The main process takes its end with `takeFd()` and wraps it in
/// `net.Socket`; see [`relay`]. Once that reads end of file, `closeReason`
/// says why.
#[napi]
pub struct HostConnection {
    relay: Arc<Relay>,
    fd: Option<OwnedFd>,
    #[cfg(feature = "dev-auth")]
    key: key::DevKey,
    #[cfg(feature = "dev-auth")]
    channel: Option<cupper_protocol::proof::Channel>,
}

#[napi]
impl HostConnection {
    /// The main process's end, once; the caller then owns the fd.
    #[napi]
    pub fn take_fd(&mut self) -> napi::Result<i32, Code> {
        let fd = self
            .fd
            .take()
            .ok_or_else(|| fail("fd_taken", "taken already"))?;
        Ok(fd.into_raw_fd())
    }

    /// The host's pid, from the pinned token.
    #[napi(getter)]
    pub fn host_pid(&self) -> i32 {
        self.relay.pinned().pid()
    }

    /// `host_closed`, `fds_passed`, `token_changed`, `host_unverified` or
    /// `local`; null while the connection is open.
    #[napi(getter)]
    pub fn close_reason(&self) -> Option<String> {
        self.relay.reason().map(|reason| reason.code().into())
    }

    #[napi]
    pub fn close(&self) {
        self.relay.close(CloseReason::Local);
    }
}

#[cfg(feature = "dev-auth")]
#[napi]
impl HostConnection {
    /// `initialize`'s `auth` for the `nonce` of `host/hello`: a fresh
    /// `c_nonce` and the proof of the key by this process. Once per
    /// connection, as the host takes `initialize` once.
    #[napi]
    pub fn auth(&mut self, nonce: String) -> napi::Result<key::DevKeyAuth, Code> {
        if self.channel.is_some() {
            return Err(fail("already_authenticated", "auth was given already"));
        }
        let c_nonce = key::random().map_err(|err| fail("unexpected", err))?;
        let pid = std::process::id() as i32;
        let (auth, channel) = self
            .key
            .auth(&nonce, c_nonce, pid)
            .map_err(|err| fail("protocol_error", err))?;
        self.channel = Some(channel);
        Ok(auth)
    }

    /// Whether `hostProof`, from `initialize`'s result, proves the key for
    /// this connection's `auth`, from the host of the pinned token. When it
    /// does not, or is missing, the connection is closed (`host_unverified`).
    #[napi]
    pub fn verify_host_proof(&self, host_proof: Option<String>) -> bool {
        let pid = self.relay.pinned().pid();
        let holds = self
            .channel
            .as_ref()
            .is_some_and(|channel| self.key.holds(channel, pid, host_proof.as_deref()));
        if !holds {
            self.relay.close(CloseReason::HostUnverified);
        }
        holds
    }
}
