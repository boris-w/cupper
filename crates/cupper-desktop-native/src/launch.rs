//! Starting the host (design 5.7.2, 5.8.5): through `cupper_sys::spawn`, in
//! a session of its own, with exactly the environment the caller chose. The
//! startup payload, when the build has one, goes in as one line on stdin,
//! which is closed right after (design 5.8.1).

use std::fs::File;
use std::io::{self, Write};
use std::process::ExitStatus;

use cupper_sys::{Child, Command, Stdio};

/// Starts `cmd` in a session of its own. `payload`, one line of JSON ending in
/// a newline, is written to its stdin, which is then closed; without one,
/// stdin is `/dev/null`.
pub fn start(cmd: &mut Command, payload: Option<&[u8]>) -> cupper_sys::Result<Child> {
    let stdin = if payload.is_some() {
        Stdio::Pipe
    } else {
        Stdio::Null
    };
    let mut child = cupper_sys::spawn(cmd.new_session().stdin(stdin))?;
    if let (Some(payload), Some(stdin)) = (payload, child.stdin.take()) {
        // A few hundred bytes fit the pipe, so this does not wait for the
        // child. It fails only when the child has already exited, and then
        // its exit status says why. Dropping the file closes stdin.
        let _ = File::from(stdin).write_all(payload);
    }
    Ok(child)
}

/// Waits for `child` on a thread of its own, which reaps it, then calls
/// `exited` there. The host can run for days, so this keeps no thread of
/// Node's pool busy.
pub fn on_exit<F>(mut child: Child, exited: F) -> io::Result<()>
where
    F: FnOnce(cupper_sys::Result<ExitStatus>) + Send + 'static,
{
    std::thread::Builder::new()
        .name("cupper-host-wait".into())
        .spawn(move || exited(child.wait()))
        .map(drop)
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::os::unix::process::ExitStatusExt;
    use std::sync::mpsc;

    use cupper_sys::{Signal, process_info};

    use super::*;

    fn exit_of(child: Child) -> ExitStatus {
        let (tx, rx) = mpsc::channel();
        on_exit(child, move |status| tx.send(status.unwrap()).unwrap()).unwrap();
        rx.recv().unwrap()
    }

    #[test]
    fn unit_native_launch_writes_one_line_then_closes_stdin() {
        let script = r#"read line; echo "$line"; read more || echo eof; exec sleep 60"#;
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", script]).stdout(Stdio::Pipe);
        let mut child = start(&mut cmd, Some(b"{\"dev_key\":\"ab\"}\n")).unwrap();
        let mut lines = BufReader::new(File::from(child.stdout.take().unwrap())).lines();
        assert_eq!(lines.next().unwrap().unwrap(), r#"{"dev_key":"ab"}"#);
        assert_eq!(lines.next().unwrap().unwrap(), "eof", "stdin is closed");

        let host = process_info(child.pid()).unwrap();
        let me = process_info(std::process::id() as i32).unwrap();
        assert_eq!(host.sid, child.pid(), "a session of its own");
        assert_ne!(host.sid, me.sid);
        assert!(child.group().signal(Signal::Term).unwrap());
        assert_eq!(exit_of(child).signal(), Some(15));
    }

    #[test]
    fn unit_native_launch_without_payload_stdin_is_null() {
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "read line || exit 7"]);
        assert_eq!(exit_of(start(&mut cmd, None).unwrap()).code(), Some(7));
    }
}
