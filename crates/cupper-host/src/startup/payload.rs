//! The startup payload on stdin (design 5.8.1, 5.2.13 step 1; frozen list
//! 5.12.5 #9).
//!
//! The launcher writes one line of JSON and closes stdin:
//! `{"dev_key"?: "<64 hex>", "harness_key"?: "<64 hex>", "test_config"?: {...}}`.
//!
//! - Only `dev-auth` builds read `dev_key`; only `test-harness` builds read
//!   `harness_key` and `test_config`. The code that parses a field exists
//!   only in the builds that read it; other builds skip the field unparsed,
//!   so a `dev-auth` build ignores even a malformed `test_config`.
//! - Release builds, which have neither feature, do not read stdin at all.
//! - No line within [`TIMEOUT`], a line that is not such an object, or a key
//!   other than these three: a [`PayloadError`], and the host exits with 1.
//! - The keys never travel in argv, the environment or a file, where the same
//!   user could read them (SP-4b).

use std::io;
use std::time::Duration;

#[cfg(any(feature = "dev-auth", feature = "test-harness"))]
pub use self::reader::{SecretKey, load};
#[cfg(feature = "test-harness")]
use super::test_config::TestConfig;
#[cfg(not(any(feature = "dev-auth", feature = "test-harness")))]
use crate::clock::Clock;
use crate::exit::HostExit;

/// How long the host waits for the payload line.
pub const TIMEOUT: Duration = Duration::from_secs(2);

/// The longest payload line, newline excluded.
pub const MAX_LEN: usize = 1 << 20;

/// What the payload gives this build. A release build gets nothing.
#[derive(Debug, Default)]
pub struct Payload {
    #[cfg(feature = "dev-auth")]
    pub dev_key: Option<SecretKey>,
    #[cfg(feature = "test-harness")]
    pub harness_key: Option<SecretKey>,
    #[cfg(feature = "test-harness")]
    pub test_config: TestConfig,
}

#[derive(Debug, thiserror::Error)]
pub enum PayloadError {
    #[error("no startup payload within {0:?}")]
    Timeout(Duration),
    #[error("stdin ended without a startup payload")]
    Missing,
    #[error("the startup payload is longer than {MAX_LEN} bytes")]
    TooLong,
    #[error("cannot read the startup payload: {0}")]
    Read(#[source] io::Error),
    #[error("malformed startup payload: {0}")]
    Malformed(#[from] serde_json::Error),
}

impl PayloadError {
    pub fn exit(&self) -> HostExit {
        HostExit::Error
    }
}

/// A release build does not touch stdin (design 5.8.1).
#[cfg(not(any(feature = "dev-auth", feature = "test-harness")))]
pub async fn load(
    _input: impl io::Read,
    _clock: &dyn Clock,
    _timeout: Duration,
) -> Result<Payload, PayloadError> {
    Ok(Payload {})
}

#[cfg(any(feature = "dev-auth", feature = "test-harness"))]
mod reader {
    use std::fmt;
    use std::future::poll_fn;
    use std::io::{BufRead, BufReader, Read};
    use std::pin::Pin;
    use std::task::Poll;

    use serde::de::{self, Error as _, Visitor};
    use serde::{Deserialize, Deserializer};

    use super::*;
    use crate::clock::Clock;

    /// Reads the payload line from `input` and parses it. A helper thread does
    /// the blocking read; the wait is a timer on `clock` set to `timeout`.
    pub async fn load(
        input: impl Read + Send + 'static,
        clock: &dyn Clock,
        timeout: Duration,
    ) -> Result<Payload, PayloadError> {
        let mut timer = clock.timer_at(clock.now_mono() + timeout);
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        std::thread::Builder::new()
            .name("startup-payload".into())
            .spawn(move || {
                // After a timeout nobody is listening any more.
                let _ = tx.send(read_line(input));
            })
            .map_err(PayloadError::Read)?;
        let line = poll_fn(|cx| {
            if let Poll::Ready(line) = Pin::new(&mut rx).poll(cx) {
                return Poll::Ready(line.unwrap_or_else(|_| {
                    Err(PayloadError::Read(io::Error::other(
                        "the payload reader stopped",
                    )))
                }));
            }
            Pin::new(&mut timer)
                .poll(cx)
                .map(|()| Err(PayloadError::Timeout(timeout)))
        })
        .await?;
        parse(&line)
    }

    /// Up to the first newline or the end of `input`, whichever comes first.
    pub(super) fn read_line(input: impl Read) -> Result<Vec<u8>, PayloadError> {
        let mut line = Vec::new();
        BufReader::new(input.take(MAX_LEN as u64 + 1))
            .read_until(b'\n', &mut line)
            .map_err(PayloadError::Read)?;
        if line.last() == Some(&b'\n') {
            line.pop();
        } else if line.len() > MAX_LEN {
            return Err(PayloadError::TooLong);
        }
        if line.is_empty() {
            return Err(PayloadError::Missing);
        }
        Ok(line)
    }

    pub(super) fn parse(line: &[u8]) -> Result<Payload, PayloadError> {
        // serde would also take a JSON array as a struct, field by position.
        if line.trim_ascii_start().first() != Some(&b'{') {
            return Err(serde_json::Error::custom("the payload must be a JSON object").into());
        }
        let wire: Wire = serde_json::from_slice(line)?;
        Ok(Payload {
            #[cfg(feature = "dev-auth")]
            dev_key: wire.dev_key,
            #[cfg(feature = "test-harness")]
            harness_key: wire.harness_key,
            #[cfg(feature = "test-harness")]
            test_config: wire.test_config.unwrap_or_default(),
        })
    }

    /// The payload as written. A field this build does not read is skipped
    /// with `IgnoredAny`, which checks only that it is JSON.
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Wire {
        #[cfg(feature = "dev-auth")]
        #[serde(default)]
        dev_key: Option<SecretKey>,
        #[cfg(not(feature = "dev-auth"))]
        #[serde(default, rename = "dev_key")]
        _dev_key: de::IgnoredAny,
        #[cfg(feature = "test-harness")]
        #[serde(default)]
        harness_key: Option<SecretKey>,
        #[cfg(not(feature = "test-harness"))]
        #[serde(default, rename = "harness_key")]
        _harness_key: de::IgnoredAny,
        #[cfg(feature = "test-harness")]
        #[serde(default)]
        test_config: Option<TestConfig>,
        #[cfg(not(feature = "test-harness"))]
        #[serde(default, rename = "test_config")]
        _test_config: de::IgnoredAny,
    }

    /// A 32-byte key from the payload, written as 64 hex digits. Neither its
    /// `Debug` nor a parse error shows the digits.
    #[derive(Clone)]
    pub struct SecretKey([u8; 32]);

    impl SecretKey {
        pub fn expose(&self) -> &[u8; 32] {
            &self.0
        }

        fn from_hex(text: &str) -> Option<Self> {
            if text.len() != 64 {
                return None;
            }
            let mut key = [0; 32];
            for (byte, pair) in key.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
                *byte = hex_digit(pair[0])? << 4 | hex_digit(pair[1])?;
            }
            Some(Self(key))
        }
    }

    fn hex_digit(digit: u8) -> Option<u8> {
        char::from(digit)
            .to_digit(16)
            .and_then(|value| u8::try_from(value).ok())
    }

    impl fmt::Debug for SecretKey {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("SecretKey(..)")
        }
    }

    impl<'de> Deserialize<'de> for SecretKey {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct HexKey;

            impl Visitor<'_> for HexKey {
                type Value = SecretKey;

                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a key of 64 hex digits")
                }

                fn visit_str<E: de::Error>(self, text: &str) -> Result<SecretKey, E> {
                    SecretKey::from_hex(text)
                        .ok_or_else(|| E::custom("a key must be 64 hex digits"))
                }
            }

            deserializer.deserialize_str(HexKey)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;

    #[cfg(not(any(feature = "dev-auth", feature = "test-harness")))]
    #[test]
    fn unit_release_features_payload_is_not_read() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::task::{Context, Poll, Waker};

        /// Stdin that records whether anyone read it.
        struct Tripwire(Arc<AtomicBool>);

        impl io::Read for Tripwire {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                self.0.store(true, Ordering::SeqCst);
                let line = br#"{"harness_key": "00", "test_config": {"failpoints": []}}"#;
                let len = line.len().min(buf.len());
                buf[..len].copy_from_slice(&line[..len]);
                Ok(len)
            }
        }

        let read = Arc::new(AtomicBool::new(false));
        let clock = ManualClock::new();
        let mut load = std::pin::pin!(load(Tripwire(Arc::clone(&read)), &clock, TIMEOUT));
        // Ready at the first poll, with the clock standing still: no waiting.
        let Poll::Ready(payload) = load.as_mut().poll(&mut Context::from_waker(Waker::noop()))
        else {
            panic!("a release build waited for stdin");
        };
        // A release payload has no field to carry a key or a test config.
        let Payload {} = payload.unwrap();
        assert!(!read.load(Ordering::SeqCst), "a release build read stdin");
        assert_eq!(clock.next_deadline(), None);
    }

    #[cfg(any(feature = "dev-auth", feature = "test-harness"))]
    mod reading {
        use std::io::{PipeWriter, Write};
        use std::task::{Context, Poll, Waker};

        use super::*;
        use crate::startup::payload::reader::{parse, read_line};

        const KEY: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
        const OTHER_KEY: &str = "F0F1F2F3F4F5F6F7F8F9FAFBFCFDFEFF00000000000000000000000000000000";

        fn runtime() -> tokio::runtime::Runtime {
            // No time driver: a real timer would panic.
            tokio::runtime::Builder::new_current_thread()
                .build()
                .expect("build a runtime")
        }

        fn stdin() -> (io::PipeReader, PipeWriter) {
            io::pipe().expect("create a pipe")
        }

        fn poll<F: Future>(future: std::pin::Pin<&mut F>) -> Poll<F::Output> {
            future.poll(&mut Context::from_waker(Waker::noop()))
        }

        #[test]
        fn unit_startup_payload_reads_one_json_line() {
            let line = format!(
                r#"{{"dev_key": "{KEY}", "harness_key": "{OTHER_KEY}", "test_config": {{"clock": "manual"}}}}"#
            );
            let (reader, mut writer) = stdin();
            // The newline ends the payload: what follows is ignored, and the
            // end of stdin is not awaited.
            writeln!(writer, "{line}\nnot part of the payload").unwrap();
            let clock = ManualClock::new();
            let payload = runtime()
                .block_on(load(reader, &clock, TIMEOUT))
                .expect("a well-formed payload");
            #[cfg(feature = "dev-auth")]
            assert_eq!(payload.dev_key.unwrap().expose()[31], 0x1f);
            #[cfg(feature = "test-harness")]
            {
                assert_eq!(payload.harness_key.unwrap().expose()[0], 0xf0);
                assert_eq!(
                    payload.test_config.clock,
                    crate::startup::test_config::ClockKind::Manual
                );
            }
            drop(writer);

            // Every field is optional, and the end of stdin also ends the line.
            let (reader, mut writer) = stdin();
            writer.write_all(b"{}").unwrap();
            drop(writer);
            runtime()
                .block_on(load(reader, &clock, TIMEOUT))
                .expect("an empty payload");
        }

        #[test]
        fn unit_startup_payload_times_out_after_two_seconds_of_clock_time() {
            assert_eq!(TIMEOUT, Duration::from_secs(2));
            let clock = ManualClock::new();
            let (reader, writer) = stdin();
            let mut load = std::pin::pin!(load(reader, &clock, TIMEOUT));
            assert!(poll(load.as_mut()).is_pending());
            clock.advance(TIMEOUT - Duration::from_millis(1));
            assert!(poll(load.as_mut()).is_pending());
            clock.advance(Duration::from_millis(1));
            let Poll::Ready(Err(err)) = poll(load.as_mut()) else {
                panic!("no timeout at {TIMEOUT:?}");
            };
            assert!(matches!(err, PayloadError::Timeout(_)), "{err}");
            assert_eq!(err.exit().code(), 1);
            // Lets the reader thread finish.
            drop(writer);
        }

        #[test]
        fn unit_startup_payload_line_just_before_the_deadline_counts() {
            let clock = ManualClock::new();
            let (reader, mut writer) = stdin();
            let mut load = std::pin::pin!(load(reader, &clock, TIMEOUT));
            assert!(poll(load.as_mut()).is_pending());
            clock.advance(TIMEOUT - Duration::from_millis(1));
            writer.write_all(b"{}\n").unwrap();
            runtime()
                .block_on(load.as_mut())
                .expect("the line came in time");
        }

        #[test]
        fn unit_startup_payload_malformed_exits_1() {
            let mut bad = vec![
                "not json".to_owned(),
                "[]".to_owned(),
                r#"[null, null, null]"#.to_owned(),
                r#""dev_key""#.to_owned(),
                "{} {}".to_owned(),
                r#"{"devkey": "00"}"#.to_owned(),
                r#"{"test_config": {}, "extra": 1}"#.to_owned(),
            ];
            #[cfg(feature = "dev-auth")]
            bad.extend([
                r#"{"dev_key": "00"}"#.to_owned(),
                r#"{"dev_key": 7}"#.to_owned(),
                format!(r#"{{"dev_key": "{}zz"}}"#, &KEY[..62]),
            ]);
            #[cfg(feature = "test-harness")]
            bad.extend([
                format!(r#"{{"harness_key": "{KEY}0"}}"#),
                r#"{"test_config": {"clock": "virtual"}}"#.to_owned(),
                r#"{"test_config": {"failpoints": [{"name": "no.such"}]}}"#.to_owned(),
                r#"{"test_config": "manual"}"#.to_owned(),
            ]);
            for line in &bad {
                let err = parse(line.as_bytes()).expect_err(line);
                assert!(matches!(err, PayloadError::Malformed(_)), "{line}: {err}");
                assert_eq!(err.exit().code(), 1, "{line}");
            }

            let missing: [&[u8]; 3] = [b"", b"\n", b"\nnext"];
            for input in missing {
                let err = read_line(input).unwrap_err();
                assert!(matches!(err, PayloadError::Missing), "{input:?}: {err}");
                assert_eq!(err.exit().code(), 1);
            }
            let long = vec![b' '; MAX_LEN + 1];
            let err = read_line(long.as_slice()).unwrap_err();
            assert!(matches!(err, PayloadError::TooLong), "{err}");
            let mut longest = vec![b' '; MAX_LEN];
            longest.push(b'\n');
            assert_eq!(read_line(longest.as_slice()).unwrap().len(), MAX_LEN);
        }

        #[test]
        fn unit_startup_payload_key_stays_out_of_errors_and_debug() {
            let short = &KEY[..60];
            let line = format!(r#"{{"dev_key": "{short}", "harness_key": "{short}"}}"#);
            let err = parse(line.as_bytes()).unwrap_err();
            assert!(!err.to_string().contains(short), "{err}");
            let key: SecretKey = serde_json::from_str(&format!(r#""{KEY}""#)).unwrap();
            assert_eq!(format!("{key:?}"), "SecretKey(..)");
        }

        /// The self-use build: `dev-auth` without `test-harness`.
        #[cfg(all(feature = "dev-auth", not(feature = "test-harness")))]
        #[test]
        fn unit_startup_payload_dev_auth_build_skips_harness_fields() {
            let line = format!(
                r#"{{"dev_key": "{KEY}", "harness_key": "not a key", "test_config": {{"no_such_injection": true}}}}"#
            );
            let payload = parse(line.as_bytes()).expect("harness fields are skipped");
            assert_eq!(payload.dev_key.unwrap().expose()[1], 0x01);
        }

        /// The integration-test build: `test-harness` without `dev-auth`.
        #[cfg(all(feature = "test-harness", not(feature = "dev-auth")))]
        #[test]
        fn unit_startup_payload_harness_build_skips_dev_key() {
            let line = format!(
                r#"{{"dev_key": ["not", "a", "key"], "harness_key": "{KEY}", "test_config": {{"notifier": "record"}}}}"#
            );
            let payload = parse(line.as_bytes()).expect("dev_key is skipped");
            assert_eq!(payload.harness_key.unwrap().expose()[2], 0x02);
            assert_eq!(
                payload.test_config.notifier,
                crate::startup::test_config::NotifierKind::Record
            );
        }
    }
}
