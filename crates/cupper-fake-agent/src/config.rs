//! Start configuration (design 5.9.2, part 1), from the command line only: the
//! host passes the program and arguments of `test_config.agents` (5.9.9), and
//! its environment rules would drop our own variables.

use std::num::ParseIntError;
use std::path::{Path, PathBuf};

pub const USAGE: &str =
    "usage: cupper-fake-agent [--persona claude|codex] [--auth logged-in|logged-out]
                         [--fault <name>[@<segment>]]... [--segment <n>] [--segment-file <path>]
faults: exit_on_start, initialize_hang, set_config_error, resume_error, linger";

const FAULTS: &[&str] = &[
    "exit_on_start",
    "initialize_hang",
    "set_config_error",
    "resume_error",
    // Ignore SIGTERM, and stay after stdin closes (the host's closing order, 5.6.8).
    "linger",
];

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{0}\n\n{USAGE}")]
    Usage(String),
    #[error("`--segment` needs a number")]
    Segment(#[source] ParseIntError),
    #[error("cannot use the segment file {}", path.display())]
    SegmentFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the segment file {} holds no number", path.display())]
    SegmentValue {
        path: PathBuf,
        #[source]
        source: ParseIntError,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Persona {
    Claude,
    Codex,
}

#[derive(Debug)]
pub struct Config {
    pub persona: Persona,
    pub logged_in: bool,
    /// The process segment this instance plays: `--segment`, advanced by one
    /// per start through `--segment-file` so a restarted agent goes on with
    /// the next segment (the replay of multi-instance recordings, T052).
    pub segment: u64,
    /// Faults active in this segment.
    faults: Vec<String>,
}

impl Config {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, ConfigError> {
        let mut config = Config {
            persona: Persona::Claude,
            logged_in: true,
            segment: 0,
            faults: Vec::new(),
        };
        let mut faults = Vec::new();
        let mut segment_file = None;
        let mut args = args.into_iter();
        while let Some(flag) = args.next() {
            let Some(value) = args.next() else {
                return Err(ConfigError::Usage(format!("`{flag}` needs a value")));
            };
            match (flag.as_str(), value.as_str()) {
                ("--persona", "claude") => config.persona = Persona::Claude,
                ("--persona", "codex") => config.persona = Persona::Codex,
                ("--auth", "logged-in") => config.logged_in = true,
                ("--auth", "logged-out") => config.logged_in = false,
                ("--fault", _) => faults.push(value),
                ("--segment", n) => config.segment = n.parse().map_err(ConfigError::Segment)?,
                ("--segment-file", _) => segment_file = Some(PathBuf::from(value)),
                _ => return Err(ConfigError::Usage(format!("bad option `{flag} {value}`"))),
            }
        }
        if let Some(path) = segment_file {
            config.segment = next_segment(&path, config.segment)?;
        }
        for fault in faults {
            let (name, only_in) = fault.split_once('@').unwrap_or((&fault, ""));
            if !FAULTS.contains(&name) {
                return Err(ConfigError::Usage(format!("unknown fault `{name}`")));
            }
            if only_in.is_empty() || only_in.parse() == Ok(config.segment) {
                config.faults.push(name.to_owned());
            }
        }
        Ok(config)
    }

    pub fn fault(&self, name: &str) -> bool {
        self.faults.iter().any(|f| f == name)
    }
}

/// Reads the segment this start plays (the file's number, or `first` when it
/// does not exist yet) and stores the one after it for the next start.
fn next_segment(path: &Path, first: u64) -> Result<u64, ConfigError> {
    let file_error = |source| ConfigError::SegmentFile {
        path: path.to_owned(),
        source,
    };
    let segment = match std::fs::read_to_string(path) {
        Ok(text) => text
            .trim()
            .parse()
            .map_err(|source| ConfigError::SegmentValue {
                path: path.to_owned(),
                source,
            })?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => first,
        Err(e) => return Err(file_error(e)),
    };
    std::fs::write(path, (segment + 1).to_string()).map_err(file_error)?;
    Ok(segment)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &str) -> Result<Config, ConfigError> {
        Config::parse(args.split_whitespace().map(str::to_owned))
    }

    #[test]
    fn unit_fake_scripts_config_defaults_and_faults_per_segment() {
        let c = parse("").unwrap();
        assert_eq!(
            (c.persona, c.logged_in, c.segment),
            (Persona::Claude, true, 0)
        );
        let c = parse("--persona codex --auth logged-out --segment 2").unwrap();
        assert_eq!(
            (c.persona, c.logged_in, c.segment),
            (Persona::Codex, false, 2)
        );
        let c = parse("--segment 1 --fault resume_error@0 --fault set_config_error@1").unwrap();
        assert!(c.fault("set_config_error") && !c.fault("resume_error"));
        assert!(matches!(
            parse("--fault no_such_fault"),
            Err(ConfigError::Usage(_))
        ));
        assert!(matches!(parse("--persona"), Err(ConfigError::Usage(_))));
        assert!(matches!(parse("--segment x"), Err(ConfigError::Segment(_))));
    }

    #[test]
    fn unit_fake_scripts_config_segment_file_advances_per_start() {
        let path = std::env::temp_dir().join(format!("cupper-fake-segment-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let args = format!("--segment 5 --segment-file {}", path.display());
        let starts: Vec<u64> = (0..3).map(|_| parse(&args).unwrap().segment).collect();
        std::fs::write(&path, "x").unwrap();
        let broken = parse(&args);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(starts, [5, 6, 7]);
        assert!(matches!(broken, Err(ConfigError::SegmentValue { .. })));
    }
}
