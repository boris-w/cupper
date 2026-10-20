//! CLI versions (design 5.6.6): the first `X.Y.Z` that `<cli> --version`
//! prints, and how it compares with the range verified for the pinned
//! adapter. The range comes in as version strings from the embedded manifest;
//! telling a too-old CLI by its "Authentication required" is the agent
//! runtime's (`cli_too_old`).

use std::path::Path;

use cupper_sys::{Command, Env};

use super::run::run;
use crate::clock::Clock;
use crate::durations::{Durations, VERSION_PROBE};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is not a version X.Y.Z")]
pub struct BadVersion(pub String);

impl Version {
    /// The first `X.Y.Z` in `text`, as in `2.1.296 (Claude Code)`,
    /// `codex-cli 0.160.1` or `git version 2.39.5 (Apple Git-154)`.
    pub fn find(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        (0..bytes.len()).find_map(|start| {
            let follows = |b: u8| b.is_ascii_digit() || b == b'.';
            // A digit is a char boundary; `text` may hold any UTF-8.
            let starts =
                bytes[start].is_ascii_digit() && (start == 0 || !follows(bytes[start - 1]));
            starts
                .then(|| Self::prefix(&text[start..]))
                .flatten()
                .map(|(version, _)| version)
        })
    }

    /// Exactly `X.Y.Z`.
    pub fn parse(text: &str) -> Result<Self, BadVersion> {
        match Self::prefix(text) {
            Some((version, "")) => Ok(version),
            _ => Err(BadVersion(text.to_owned())),
        }
    }

    /// `X.Y.Z` at the start of `text`, and what follows it.
    fn prefix(mut text: &str) -> Option<(Self, &str)> {
        let mut numbers = [0u64; 3];
        for (index, number) in numbers.iter_mut().enumerate() {
            if index > 0 {
                text = text.strip_prefix('.')?;
            }
            let digits = text.bytes().take_while(u8::is_ascii_digit).count();
            *number = text.get(..digits).filter(|d| !d.is_empty())?.parse().ok()?;
            text = &text[digits..];
        }
        let [major, minor, patch] = numbers;
        Some((
            Self {
                major,
                minor,
                patch,
            },
            text,
        ))
    }
}

/// The range verified for an adapter version (design 5.6.6), as the manifest
/// writes it: inclusive bounds, and below `unsupported_below` a CLI is known
/// to fail.
#[derive(Debug, Clone, Copy)]
pub struct VerifiedRange<'a> {
    pub min: &'a str,
    pub max: &'a str,
    pub unsupported_below: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionClass {
    Verified,
    NewerUntested,
    OlderUntested,
    Unsupported,
}

pub fn classify(version: Version, range: &VerifiedRange) -> Result<VersionClass, BadVersion> {
    let (min, max) = (Version::parse(range.min)?, Version::parse(range.max)?);
    let floor = range.unsupported_below.map(Version::parse).transpose()?;
    Ok(if floor.is_some_and(|floor| version < floor) {
        VersionClass::Unsupported
    } else if version < min {
        VersionClass::OlderUntested
    } else if version > max {
        VersionClass::NewerUntested
    } else {
        VersionClass::Verified
    })
}

/// The version `binary --version` prints, with `env` (the caller's
/// allow-listed environment) and the [`VERSION_PROBE`] time limit; `None`
/// when it fails, times out or prints no version.
pub async fn probe(
    clock: &dyn Clock,
    durations: &Durations,
    binary: &Path,
    env: Env,
) -> Option<Version> {
    let mut command = Command::new(binary);
    command.arg("--version").env(env);
    let output = run(clock, command, durations.get(VERSION_PROBE))
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    output
        .status
        .success()
        .then(|| Version::find(&text))
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;
    use crate::driver::probe::testkit::script;
    use crate::store::testutil::TempDir;

    fn v(text: &str) -> Version {
        Version::parse(text).expect(text)
    }

    #[test]
    fn unit_version_classify_finds_the_first_version_in_cli_output() {
        for (output, want) in [
            ("2.1.296 (Claude Code)\n", "2.1.296"),
            ("codex-cli 0.160.1\n", "0.160.1"),
            ("git version 2.39.5 (Apple Git-154)\n", "2.39.5"),
            ("git-lfs/3.4.0 (GitHub; darwin arm64; go 1.21.1)\n", "3.4.0"),
            ("tool™ 1.2 build 10.20.30-beta", "10.20.30"),
        ] {
            assert_eq!(Version::find(output), Some(v(want)), "{output}");
        }
        for output in ["", "no version", "1.2", "v.1.2"] {
            assert_eq!(Version::find(output), None, "{output}");
        }
        assert!(Version::parse("2.1.296 (Claude Code)").is_err());
    }

    #[test]
    fn unit_version_classify_places_a_version_against_the_verified_range() {
        let codex = VerifiedRange {
            min: "0.150.1",
            max: "0.163.0",
            unsupported_below: Some("0.140.0"),
        };
        for (version, class) in [
            ("0.150.1", VersionClass::Verified),
            ("0.163.0", VersionClass::Verified),
            ("0.163.1", VersionClass::NewerUntested),
            ("0.149.9", VersionClass::OlderUntested),
            ("0.140.0", VersionClass::OlderUntested),
            ("0.139.9", VersionClass::Unsupported),
        ] {
            assert_eq!(classify(v(version), &codex), Ok(class), "{version}");
        }
        let claude = VerifiedRange {
            min: "2.1.200",
            max: "2.1.296",
            unsupported_below: None,
        };
        assert_eq!(
            classify(v("1.0.0"), &claude),
            Ok(VersionClass::OlderUntested)
        );
        let broken = VerifiedRange {
            max: "latest",
            ..claude
        };
        assert_eq!(
            classify(v("2.1.200"), &broken),
            Err(BadVersion("latest".into()))
        );
    }

    #[tokio::test]
    async fn unit_version_classify_probes_a_fake_cli() {
        let dir = TempDir::new("version-probe");
        let clock = ManualClock::new();
        let durations = Durations::default();
        let ok = script(
            dir.path(),
            "claude",
            "[ \"$1\" = --version ] && echo '2.1.296 (Claude Code)'",
        );
        let failing = script(dir.path(), "broken", "echo '9.9.9'; exit 3");
        let silent = script(dir.path(), "silent", "echo 'no version'");
        assert_eq!(
            probe(&clock, &durations, &ok, Env::new()).await,
            Some(v("2.1.296"))
        );
        assert_eq!(probe(&clock, &durations, &failing, Env::new()).await, None);
        assert_eq!(probe(&clock, &durations, &silent, Env::new()).await, None);
    }
}
