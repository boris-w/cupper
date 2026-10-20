//! `host.log` (design 5.11.1): a tracing subscriber that writes each event as
//! one JSON line (time from the host's [`Clock`], level, module, message,
//! fields), redacted in full ([`Redactor`]), to a file rotated at 10 MB × 5.
//! Spans are not recorded. cupper's own modules log from `info` up and other
//! crates (the ACP SDK) from `warn`; `diagnostics.verbose` ([`set_verbose`])
//! lowers both to `debug` and lets [`Content`] fields show what they hold.

use std::fmt;
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::subscriber::Interest;
use tracing::{Event, Level, Metadata, Subscriber};

use super::full::Redactor;
use super::rotate::Rotating;
use crate::clock::Clock;

const MAX_BYTES: u64 = 10 * 1024 * 1024;
const FILES: usize = 5;

static VERBOSE: AtomicBool = AtomicBool::new(false);

/// `diagnostics.verbose` (design 5.11.1): `debug` events, and the text of
/// [`Content`] fields. The setting and its confirmation are T036's.
pub fn set_verbose(on: bool) {
    VERBOSE.store(on, Ordering::Relaxed);
}

fn verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

/// A prompt, message body, command output or file content as a log field:
/// only its kind and length, unless `diagnostics.verbose` is on (design
/// 5.11.2). `tracing::info!(prompt = %Content("prompt", text), ...)`.
pub struct Content<'a>(pub &'static str, pub &'a str);

impl fmt::Display for Content<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match verbose() {
            true => f.write_str(self.1),
            false => write!(f, "<{}, {} bytes>", self.0, self.1.len()),
        }
    }
}

/// Where a host on `data_dir` logs: `~/Library/Logs/<name>` for a data
/// directory `~/Library/Application Support/<name>` (`cupper`,
/// `cupper-local`, `cupper-dev`: one per build kind, design 5.7.9), and
/// `<data_dir>/logs` for any other, such as the temporary ones of tests.
pub fn log_dir(data_dir: &Path) -> PathBuf {
    match (data_dir.parent(), data_dir.file_name()) {
        (Some(parent), Some(name)) if parent.ends_with("Library/Application Support") => {
            parent.with_file_name("Logs").join(name)
        }
        _ => data_dir.join("logs"),
    }
}

/// Makes `dir/host.log` the global log and installs the panic hook. `echo`
/// (development builds) copies each line to the stderr the host started
/// with, kept before stdio goes to `/dev/null`.
pub fn init(dir: &Path, clock: Arc<dyn Clock>, echo: bool) -> io::Result<()> {
    let logger = Logger::new(dir, clock, Redactor::for_this_machine(), echo)?;
    tracing::subscriber::set_global_default(logger).map_err(io::Error::other)?;
    install_panic_hook();
    Ok(())
}

/// Panics go to the log, with where they happened, then to the previous hook.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info.payload();
        let message = (payload.downcast_ref::<&str>().copied())
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("(no message)");
        let location = info
            .location()
            .map(|at| format!("{}:{}", at.file(), at.line()));
        tracing::error!(target: "cupper_host::panic", location, "panic: {message}");
        previous(info);
    }));
}

pub struct Logger {
    file: Mutex<Rotating>,
    echo: Option<Mutex<File>>,
    clock: Arc<dyn Clock>,
    redactor: Redactor,
}

impl Logger {
    pub fn new(
        dir: &Path,
        clock: Arc<dyn Clock>,
        redactor: Redactor,
        echo: bool,
    ) -> io::Result<Self> {
        let file = Mutex::new(Rotating::open(dir, "host.log", MAX_BYTES, FILES)?);
        let echo = match echo {
            true => Some(Mutex::new(File::from(
                io::stderr().as_fd().try_clone_to_owned()?,
            ))),
            false => None,
        };
        Ok(Self {
            file,
            echo,
            clock,
            redactor,
        })
    }
}

impl Subscriber for Logger {
    fn register_callsite(&self, _: &'static Metadata<'static>) -> Interest {
        // `diagnostics.verbose` can change at any time.
        Interest::sometimes()
    }

    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        let ours = metadata.target().starts_with("cupper");
        let most = match (verbose(), ours) {
            (true, _) => Level::DEBUG,
            (false, true) => Level::INFO,
            (false, false) => Level::WARN,
        };
        *metadata.level() <= most
    }

    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _: &Id, _: &Record<'_>) {}

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut fields = Fields {
            redactor: &self.redactor,
            message: String::new(),
            fields: Map::new(),
        };
        event.record(&mut fields);
        let metadata = event.metadata();
        let line = json!({
            "time": rfc3339(self.clock.now_wall_ms()),
            "level": metadata.level().as_str(),
            "module": metadata.target(),
            "message": fields.message,
            "fields": fields.fields,
        })
        .to_string();
        // A log that cannot be written has nowhere to say so.
        let _ = self.file.lock().map(|mut file| file.write_line(&line));
        if let Some(echo) = &self.echo {
            let _ = echo.lock().map(|mut echo| writeln!(echo, "{line}"));
        }
    }

    fn enter(&self, _: &Id) {}

    fn exit(&self, _: &Id) {}
}

/// An event's fields, each redacted by its name.
struct Fields<'a> {
    redactor: &'a Redactor,
    message: String,
    fields: Map<String, Value>,
}

impl Visit for Fields<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        let value = self.redactor.field(field.name(), value);
        match field.name() {
            "message" => self.message = value,
            name => {
                self.fields.insert(name.to_owned(), value.into());
            }
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.record_str(field, &format!("{value:?}"));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.insert(field.name().to_owned(), value.into());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.insert(field.name().to_owned(), value.into());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields.insert(field.name().to_owned(), value.into());
    }
}

/// Milliseconds since the Unix epoch as `YYYY-MM-DDTHH:MM:SS.mmmZ` (days to
/// civil date after H. Hinnant).
pub(crate) fn rfc3339(ms: u64) -> String {
    let (secs, days) = (ms / 1000 % 86_400, ms / 86_400_000);
    let z = days + 719_468;
    let (era, doe) = (z / 146_097, z % 146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    let (h, m, s) = (secs / 3600, secs % 3600 / 60, secs % 60);
    format!(
        "{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}.{:03}Z",
        ms % 1000
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use std::fs;

    use super::*;
    use crate::clock::ManualClock;
    use crate::store::testutil::TempDir;

    /// Tests that log take it: `diagnostics.verbose` is one switch per process.
    pub(crate) static VERBOSE_SWITCH: Mutex<()> = Mutex::new(());

    /// Runs `f` with a logger on a made-up machine as this thread's
    /// subscriber; returns the lines of `host.log` as JSON.
    fn logged(dir: &TempDir, f: impl FnOnce()) -> Vec<Value> {
        let redactor = Redactor::new(Some("/Users/jdoe-example"), Some("example-mbp.local"));
        let clock = Arc::new(ManualClock::new());
        let logger = Logger::new(dir.path(), clock, redactor, false).expect("logger");
        tracing::subscriber::with_default(logger, f);
        let text = fs::read_to_string(dir.path().join("host.log")).expect("host.log");
        text.lines()
            .map(|line| serde_json::from_str(line).expect("a JSON line"))
            .collect()
    }

    #[test]
    fn unit_redaction_host_log_lines_are_redacted_json() {
        let _switch = VERBOSE_SWITCH.lock().unwrap_or_else(|e| e.into_inner());
        let dir = TempDir::new("host-log");
        let hash = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
        let lines = logged(&dir, || {
            tracing::info!(
                blob_hash = hash,
                path = "/Users/jdoe-example/x",
                "opened {hash}"
            );
            tracing::debug!("not at info");
            tracing::info!(target: "agent_client_protocol", "a crate's info");
            tracing::warn!(target: "agent_client_protocol", attempt = 2u64, "a crate's warning");
        });
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(
            lines[0],
            json!({"time": "2026-01-01T00:00:00.000Z", "level": "INFO",
                "module": "cupper_host::diag::log::tests", "message": "opened <redacted>",
                "fields": {"blob_hash": hash, "path": "~/x"}})
        );
        assert_eq!(lines[1]["module"], "agent_client_protocol");
        assert_eq!(lines[1]["fields"], json!({"attempt": 2}));
    }

    #[test]
    fn unit_redaction_info_logs_only_the_kind_and_length_of_content() {
        let _switch = VERBOSE_SWITCH.lock().unwrap_or_else(|e| e.into_inner());
        let dir = TempDir::new("host-log-content");
        let lines = logged(&dir, || {
            tracing::info!(prompt = %Content("prompt", "fix the bug"), "prompt sent");
            set_verbose(true);
            tracing::debug!(output = %Content("command output", "ok"), "command ended");
            set_verbose(false);
        });
        assert_eq!(lines[0]["fields"]["prompt"], "<prompt, 11 bytes>");
        assert_eq!(lines[1]["level"], "DEBUG");
        assert_eq!(lines[1]["fields"]["output"], "ok");
    }

    #[test]
    fn unit_redaction_panics_go_to_the_log_with_their_location() {
        let _switch = VERBOSE_SWITCH.lock().unwrap_or_else(|e| e.into_inner());
        let dir = TempDir::new("host-log-panic");
        let lines = logged(&dir, || {
            install_panic_hook();
            let _ = std::panic::catch_unwind(|| panic!("boom in /Users/jdoe-example/proj"));
        });
        assert_eq!(lines[0]["level"], "ERROR");
        assert_eq!(lines[0]["module"], "cupper_host::panic");
        assert_eq!(lines[0]["message"], "panic: boom in ~/proj");
        let location = lines[0]["fields"]["location"].as_str().expect("a location");
        assert!(
            location.starts_with("crates/cupper-host/src/diag/log.rs:"),
            "{location}"
        );
    }

    #[test]
    fn log_directories_and_times() {
        let support = Path::new("/Users/x/Library/Application Support");
        assert_eq!(
            log_dir(&support.join("cupper-local")),
            Path::new("/Users/x/Library/Logs/cupper-local")
        );
        assert_eq!(
            log_dir(Path::new("/tmp/case/data")),
            Path::new("/tmp/case/data/logs")
        );
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(rfc3339(1_709_210_096_789), "2024-02-29T12:34:56.789Z");
        assert_eq!(rfc3339(4_107_628_799_999), "2100-03-01T23:59:59.999Z");
    }
}
