//! The adapters' stderr (design 5.11.1). The host reads each adapter
//! instance's stderr pipe: every line gets the time and the instance ID, is
//! redacted in full and goes to `adapter-<agent>.log` (5 MB × 3). The last
//! 64 KB of each instance also stay in memory, for the details of an `error`
//! item (MS-5) and the diagnostics bundle. Spawning adapters and handing
//! their pipes over is the agent runtime's (T055).

use std::collections::VecDeque;
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::task::JoinHandle;

use super::full::Redactor;
use super::log::rfc3339;
use super::rotate::Rotating;
use crate::clock::Clock;

/// What each instance keeps of its latest stderr.
pub const TAIL_BYTES: usize = 64 * 1024;
const MAX_BYTES: u64 = 5 * 1024 * 1024;
const FILES: usize = 3;
/// Longer lines are cut into pieces of this size.
const MAX_LINE: usize = 16 * 1024;

/// One agent's adapter log, which all its instances share.
pub struct AdapterLog {
    file: Mutex<Rotating>,
    clock: Arc<dyn Clock>,
    redactor: Redactor,
}

/// The latest redacted stderr of one instance, whole lines, at most
/// [`TAIL_BYTES`].
#[derive(Debug, Clone, Default)]
pub struct Tail(Arc<Mutex<VecDeque<String>>>);

impl Tail {
    pub fn text(&self) -> String {
        let lines = self.0.lock().unwrap_or_else(|e| e.into_inner());
        lines
            .iter()
            .flat_map(|line| [line.as_str(), "\n"])
            .collect()
    }

    fn push(&self, line: String) {
        let mut lines = self.0.lock().unwrap_or_else(|e| e.into_inner());
        lines.push_back(line);
        let mut size: usize = lines.iter().map(|line| line.len() + 1).sum();
        while size > TAIL_BYTES {
            size -= lines.pop_front().map_or(0, |line| line.len() + 1);
        }
    }
}

impl AdapterLog {
    /// `dir/adapter-<agent>.log`, `agent` being `claude` or `codex`.
    pub fn open(
        dir: &Path,
        agent: &str,
        clock: Arc<dyn Clock>,
        redactor: Redactor,
    ) -> io::Result<Arc<Self>> {
        let file = Rotating::open(dir, &format!("adapter-{agent}.log"), MAX_BYTES, FILES)?;
        Ok(Arc::new(Self {
            file: Mutex::new(file),
            clock,
            redactor,
        }))
    }

    /// Reads `stderr` of `instance` until it closes; the tail fills as lines
    /// arrive.
    pub fn read(
        self: &Arc<Self>,
        instance: String,
        stderr: impl AsyncRead + Unpin + Send + 'static,
    ) -> (Tail, JoinHandle<()>) {
        let tail = Tail::default();
        let (log, filled) = (Arc::clone(self), tail.clone());
        let reading = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = Vec::new();
            while let Ok(true) = next_line(&mut reader, &mut line).await {
                let text = String::from_utf8_lossy(&line);
                let text = log.redactor.text(text.trim_end_matches(['\n', '\r']));
                let time = rfc3339(log.clock.now_wall_ms());
                let entry = format!("{time} {instance} {text}");
                // A log that cannot be written has nowhere to say so.
                let _ = log.file.lock().map(|mut file| file.write_line(&entry));
                filled.push(text);
            }
        });
        (tail, reading)
    }
}

/// The next line, newline included, or up to [`MAX_LINE`] bytes of it;
/// `false` at the end.
async fn next_line<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    line: &mut Vec<u8>,
) -> io::Result<bool> {
    line.clear();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok(!line.is_empty());
        }
        let newline = available.iter().position(|b| *b == b'\n');
        let take = newline.map_or(available.len(), |at| at + 1);
        let room = MAX_LINE - line.len();
        line.extend_from_slice(&available[..take.min(room)]);
        reader.consume(take.min(room));
        if newline.is_some_and(|at| at < room) || line.len() == MAX_LINE {
            return Ok(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tokio::io::AsyncWriteExt;
    use tokio::net::unix::pipe;

    use super::*;
    use crate::clock::ManualClock;
    use crate::store::testutil::TempDir;

    /// Writes `lines` into a pipe that `instance` of a claude log reads.
    async fn read_lines(dir: &TempDir, lines: &[String]) -> Tail {
        let redactor = Redactor::new(Some("/Users/jdoe-example"), None);
        let log = AdapterLog::open(dir.path(), "claude", Arc::new(ManualClock::new()), redactor)
            .expect("open");
        let (mut write, read) = pipe::pipe().expect("a pipe");
        let (tail, reading) = log.read("i_0199a1b2".into(), read);
        for line in lines {
            write.write_all(line.as_bytes()).await.expect("write");
        }
        drop(write);
        reading.await.expect("the reader ends");
        tail
    }

    #[tokio::test]
    async fn unit_redaction_adapter_stderr_gets_time_instance_and_redaction() {
        let dir = TempDir::new("adapter-log");
        let lines = [
            "starting in /Users/jdoe-example/proj\n".to_owned(),
            "token sk-ant-api03-AbCdEf0123456789xyz\r\n".to_owned(),
            "no newline at the end".to_owned(),
        ];
        let tail = read_lines(&dir, &lines).await;
        let log = fs::read_to_string(dir.path().join("adapter-claude.log")).expect("log");
        assert_eq!(
            log,
            "2026-01-01T00:00:00.000Z i_0199a1b2 starting in ~/proj\n\
             2026-01-01T00:00:00.000Z i_0199a1b2 token sk-<redacted>\n\
             2026-01-01T00:00:00.000Z i_0199a1b2 no newline at the end\n"
        );
        assert_eq!(
            tail.text(),
            "starting in ~/proj\ntoken sk-<redacted>\nno newline at the end\n"
        );
    }

    #[tokio::test]
    async fn unit_redaction_adapter_stderr_tail_keeps_the_last_64_kb() {
        let dir = TempDir::new("adapter-tail");
        let lines: Vec<String> = (0..100)
            .map(|n| format!("{n:04} {}\n", "x".repeat(1019)))
            .collect();
        let tail = read_lines(&dir, &lines).await.text();
        assert!(tail.len() <= TAIL_BYTES, "{}", tail.len());
        assert!(tail.len() > TAIL_BYTES - 1025, "{}", tail.len());
        assert!(
            tail.starts_with("0037 ") && tail.ends_with(&lines[99]),
            "whole lines, the newest"
        );
        let log = fs::read_to_string(dir.path().join("adapter-claude.log")).expect("log");
        assert_eq!(log.lines().count(), 100, "the log keeps everything");

        let long = "y".repeat(MAX_LINE + 10);
        let tail = read_lines(&TempDir::new("adapter-long"), &[long])
            .await
            .text();
        let pieces: Vec<usize> = tail.lines().map(str::len).collect();
        assert_eq!(pieces, [MAX_LINE, 10]);
    }
}
