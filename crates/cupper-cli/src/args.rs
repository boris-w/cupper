//! The command line (design 5.2.14), parsed by pure functions: the global
//! options, then one command. `repl` parses each line it reads with
//! [`command`] after [`split`].

use std::path::PathBuf;

use cupper_protocol::ids::{RequestId, SessionId};
use cupper_protocol::rpc::Scope;

pub const USAGE: &str = "usage: cupper [--data-dir <dir>] [--json] <command>

commands:
  status                                   the host's state
  sessions [--watch]                       the sessions, then their changes
  watch <session>                          the session's events as they come
  history <session>                        the session's earlier turns
  new --agent <agent> --project <path> <message>
  send <session> <text> [--wait]
  interrupt <session>
  pending [<session>]                      requests waiting for an answer
  approve|reject <session> <request> [--option <id>]
  answer <session> <request> --field <id>=<value>...
  elevate --scopes write[,approve] [--reason <text>]
  repl                                     commands from stdin, one per line,
                                           on one connection; words split at
                                           blanks, \"double quotes\" keep one
  cupper --build-info

Writing needs `elevate` first, on the same connection: use `repl`.";

/// The options before the command.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Global {
    pub data_dir: Option<PathBuf>,
    /// JSON Lines instead of text for people.
    pub json: bool,
    /// Test builds: the fd that holds the `harness` key.
    pub harness_key_fd: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    BuildInfo,
    Status,
    Sessions {
        watch: bool,
    },
    Watch(SessionId),
    History(SessionId),
    New {
        agent: String,
        project: PathBuf,
        message: String,
    },
    Send {
        session: SessionId,
        text: String,
        wait: bool,
    },
    Interrupt(SessionId),
    Pending(Option<SessionId>),
    /// `approve` (`true`) or `reject`.
    Respond {
        session: SessionId,
        request: RequestId,
        approve: bool,
        option: Option<String>,
    },
    Answer {
        session: SessionId,
        request: RequestId,
        fields: Vec<(String, String)>,
    },
    Elevate {
        scopes: Vec<Scope>,
        reason: Option<String>,
    },
    Repl,
    /// Test builds: `test/clock_advance` by this many milliseconds (design 5.2.16).
    #[cfg(feature = "test-harness")]
    ClockAdvance(u64),
    /// Test builds: follows `index` or a session and prints its whole view
    /// after each batch, to compare with another client's (E2E-NFR1).
    #[cfg(feature = "test-harness")]
    View(Option<SessionId>),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct UsageError(pub String);

fn usage(message: impl Into<String>) -> UsageError {
    UsageError(message.into())
}

/// The whole command line, without the program's name.
pub fn parse(args: &[String]) -> Result<(Global, Command), UsageError> {
    let mut global = Global::default();
    let mut rest = args;
    while let Some((first, more)) = rest.split_first() {
        let value = || {
            more.first()
                .ok_or_else(|| usage(format!("`{first}` needs a value")))
        };
        match first.as_str() {
            "--build-info" if more.is_empty() => return Ok((global, Command::BuildInfo)),
            "--json" => (global.json, rest) = (true, more),
            "--data-dir" => (global.data_dir, rest) = (Some(value()?.into()), &more[1..]),
            #[cfg(feature = "test-harness")]
            "--harness-key-fd" => {
                let fd = value()?
                    .parse()
                    .map_err(|_| usage("`--harness-key-fd` needs an fd"))?;
                (global.harness_key_fd, rest) = (Some(fd), &more[1..]);
            }
            _ => break,
        }
    }
    Ok((global, command(rest)?))
}

/// One command and its arguments.
pub fn command(words: &[String]) -> Result<Command, UsageError> {
    let (name, args) = words.split_first().ok_or_else(|| usage("no command"))?;
    let mut flags = Flags::split(args)?;
    let command = match name.as_str() {
        "status" => Command::Status,
        "sessions" => Command::Sessions {
            watch: flags.switch("--watch"),
        },
        "watch" => Command::Watch(flags.id(0)?),
        "history" => Command::History(flags.id(0)?),
        "new" => Command::New {
            agent: flags.required("--agent")?,
            project: flags.required("--project")?.into(),
            message: flags.text(0)?,
        },
        "send" => Command::Send {
            session: flags.id(0)?,
            text: flags.text(1)?,
            wait: flags.switch("--wait"),
        },
        "interrupt" => Command::Interrupt(flags.id(0)?),
        "pending" if flags.positional.is_empty() => Command::Pending(None),
        "pending" => Command::Pending(Some(flags.id(0)?)),
        "approve" | "reject" => Command::Respond {
            session: flags.id(0)?,
            request: flags.id(1)?,
            approve: name == "approve",
            option: flags.value("--option")?,
        },
        "answer" => {
            let fields = flags.values("--field").into_iter().map(|field| {
                let (id, value) = field
                    .split_once('=')
                    .ok_or_else(|| usage("`--field` takes <id>=<value>"))?;
                Ok((id.to_owned(), value.to_owned()))
            });
            let fields = fields.collect::<Result<Vec<_>, UsageError>>()?;
            Command::Answer {
                session: flags.id(0)?,
                request: flags.id(1)?,
                fields,
            }
        }
        "elevate" => {
            let scopes = flags.required("--scopes")?;
            let scopes = scopes.split(',').map(|scope| match scope {
                "write" => Ok(Scope::Write),
                "approve" => Ok(Scope::Approve),
                other => Err(usage(format!("no scope `{other}` to ask for"))),
            });
            Command::Elevate {
                scopes: scopes.collect::<Result<_, _>>()?,
                reason: flags.value("--reason")?,
            }
        }
        "repl" => Command::Repl,
        #[cfg(feature = "test-harness")]
        "clock-advance" => {
            let ms = flags.text(0)?;
            let ms = ms
                .parse()
                .map_err(|_| usage(format!("`{ms}` is not milliseconds")))?;
            Command::ClockAdvance(ms)
        }
        #[cfg(feature = "test-harness")]
        "view"
            if flags
                .positional
                .first()
                .is_some_and(|stream| stream == "index") =>
        {
            flags.text(0)?;
            Command::View(None)
        }
        #[cfg(feature = "test-harness")]
        "view" => Command::View(Some(flags.id(0)?)),
        other => return Err(usage(format!("no command `{other}`"))),
    };
    flags.done(name)?;
    Ok(command)
}

/// A command's arguments: `--flag value` pairs, switches, and the rest in
/// order. Each is taken once; [`Flags::done`] refuses what is left.
struct Flags {
    pairs: Vec<(String, Option<String>)>,
    positional: Vec<String>,
    taken: usize,
}

/// Flags that take no value.
const SWITCHES: &[&str] = &["--watch", "--wait"];

impl Flags {
    fn split(args: &[String]) -> Result<Self, UsageError> {
        let (mut pairs, mut positional) = (Vec::new(), Vec::new());
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            if !arg.starts_with("--") {
                positional.push(arg.clone());
            } else if SWITCHES.contains(&arg.as_str()) {
                pairs.push((arg.clone(), None));
            } else {
                let value = args
                    .next()
                    .ok_or_else(|| usage(format!("`{arg}` needs a value")))?;
                pairs.push((arg.clone(), Some(value.clone())));
            }
        }
        Ok(Self {
            pairs,
            positional,
            taken: 0,
        })
    }

    fn switch(&mut self, name: &str) -> bool {
        let found = self
            .pairs
            .iter()
            .position(|(flag, value)| flag == name && value.is_none());
        found.map(|at| self.pairs.remove(at)).is_some()
    }

    fn values(&mut self, name: &str) -> Vec<String> {
        let (taken, kept) = std::mem::take(&mut self.pairs)
            .into_iter()
            .partition(|(flag, _)| flag == name);
        self.pairs = kept;
        taken
            .into_iter()
            .filter_map(|(_, value): (String, Option<String>)| value)
            .collect()
    }

    fn value(&mut self, name: &str) -> Result<Option<String>, UsageError> {
        let mut values = self.values(name);
        match values.len() {
            0 | 1 => Ok(values.pop()),
            _ => Err(usage(format!("`{name}` given twice"))),
        }
    }

    fn required(&mut self, name: &str) -> Result<String, UsageError> {
        self.value(name)?
            .ok_or_else(|| usage(format!("`{name}` is needed")))
    }

    fn text(&mut self, at: usize) -> Result<String, UsageError> {
        self.taken = self.taken.max(at + 1);
        self.positional
            .get(at)
            .cloned()
            .ok_or_else(|| usage("an argument is missing"))
    }

    fn id<T: std::str::FromStr>(&mut self, at: usize) -> Result<T, UsageError> {
        let text = self.text(at)?;
        text.parse()
            .map_err(|_| usage(format!("`{text}` is not a valid ID")))
    }

    fn done(self, name: &str) -> Result<(), UsageError> {
        if let Some((flag, _)) = self.pairs.first() {
            return Err(usage(format!("`{name}` takes no `{flag}`")));
        }
        match self.positional.get(self.taken) {
            Some(extra) => Err(usage(format!("`{name}`: unexpected `{extra}`"))),
            None => Ok(()),
        }
    }
}

/// A line of `repl` as words: split at blanks; double quotes keep blanks
/// in one word and are taken away.
pub fn split(line: &str) -> Result<Vec<String>, UsageError> {
    let (mut words, mut word, mut quoted, mut started) = (Vec::new(), String::new(), false, false);
    for c in line.chars() {
        match c {
            '"' => (quoted, started) = (!quoted, true),
            c if c.is_whitespace() && !quoted => {
                if std::mem::take(&mut started) {
                    words.push(std::mem::take(&mut word));
                }
            }
            c => (word, started) = (word + c.encode_utf8(&mut [0; 4]), true),
        }
    }
    if quoted {
        return Err(usage("a double quote is not closed"));
    }
    if started {
        words.push(word);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        split(line).unwrap()
    }

    #[cfg(feature = "test-harness")]
    #[test]
    fn unit_cli_args_clock_advance_in_test_builds() {
        let advance = command(&words("clock-advance 150")).unwrap();
        assert_eq!(advance, Command::ClockAdvance(150));
        assert!(command(&words("clock-advance soon")).is_err());
        assert!(command(&words("clock-advance")).is_err());
        assert_eq!(command(&words("view index")).unwrap(), Command::View(None));
        let session = SessionId::from_uuid(1);
        let view = command(&words(&format!("view {session}"))).unwrap();
        assert_eq!(view, Command::View(Some(session)));
        assert!(command(&words("view sessions")).is_err());
    }

    #[test]
    fn unit_cli_args_commands_options_and_mistakes() {
        let (s, r) = (|| SessionId::from_uuid(1), RequestId::from_uuid(2));
        let (global, status) = parse(&words("--data-dir /d --json status")).unwrap();
        assert_eq!(
            (global.data_dir, global.json, status),
            (Some("/d".into()), true, Command::Status)
        );
        assert_eq!(parse(&words("--build-info")).unwrap().1, Command::BuildInfo);
        let send = command(&words(&format!("send {} \"two words\" --wait", s()))).unwrap();
        assert_eq!(
            send,
            Command::Send {
                session: s(),
                text: "two words".into(),
                wait: true
            }
        );
        let new = command(&words("new --project /p --agent claude hello")).unwrap();
        let expected = Command::New {
            agent: "claude".into(),
            project: "/p".into(),
            message: "hello".into(),
        };
        assert_eq!(new, expected);
        assert_eq!(command(&words("pending")).unwrap(), Command::Pending(None));
        let scopes = command(&words("elevate --scopes write,approve")).unwrap();
        let elevate = Command::Elevate {
            scopes: vec![Scope::Write, Scope::Approve],
            reason: None,
        };
        assert_eq!(scopes, elevate);
        let answer = command(&words(&format!(
            "answer {} {r} --field a=1 --field b=x=y",
            s()
        )));
        let fields = vec![("a".into(), "1".into()), ("b".into(), "x=y".into())];
        assert_eq!(
            answer.unwrap(),
            Command::Answer {
                session: s(),
                request: r,
                fields
            }
        );
        let mistakes = [
            "status --watch",
            "send",
            "watch not-an-id",
            "elevate --scopes privileged",
            "new --agent claude hello",
            "sessions extra",
            "frobnicate",
            "--data-dir",
        ];
        for line in mistakes {
            assert!(parse(&words(line)).is_err(), "{line}");
        }
        assert!(
            parse(&words("--harness-key-fd 3 status")).is_ok() == cfg!(feature = "test-harness")
        );
        assert_eq!(words(r#"  a "b c"  "" d"#), ["a", "b c", "", "d"]);
        assert!(split("\"open").is_err());
    }
}
