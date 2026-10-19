//! The `harness` key of e2e builds (design 5.8.1, 5.2.14): `--harness-key-fd
//! <n>` names an inherited fd that carries the startup payload's
//! `harness_key`, 64 hex digits on one line. A key never travels in argv or
//! the environment, which the same user can read.

use std::fmt;
use std::io::{self, BufRead, BufReader, Read};

use cupper_protocol::proof::{self, Bytes32};

/// The key; its `Debug` does not show it.
#[derive(Clone, PartialEq, Eq)]
pub struct HarnessKey(Bytes32);

#[derive(Debug, thiserror::Error)]
pub enum HarnessKeyError {
    #[error("cannot read the harness key from fd {fd}: {source}")]
    Read { fd: i32, source: io::Error },
    #[error("the harness key must be 64 hex digits")]
    Malformed,
}

impl HarnessKey {
    pub fn new(key: Bytes32) -> Self {
        Self(key)
    }

    /// Reads the key from the inherited fd `fd`, up to a newline or its end.
    /// `/dev/fd/<fd>` opens a copy of it, so nothing claims the fd itself.
    pub fn read_fd(fd: i32) -> Result<Self, HarnessKeyError> {
        let read = |source| HarnessKeyError::Read { fd, source };
        let file = std::fs::File::open(format!("/dev/fd/{fd}")).map_err(read)?;
        let mut line = Vec::new();
        BufReader::new(file.take(66))
            .read_until(b'\n', &mut line)
            .map_err(read)?;
        let digits = line.strip_suffix(b"\n").unwrap_or(&line);
        let digits = std::str::from_utf8(digits).map_err(|_| HarnessKeyError::Malformed)?;
        proof::parse_hex32(digits)
            .map(Self)
            .ok_or(HarnessKeyError::Malformed)
    }

    pub(crate) fn expose(&self) -> &Bytes32 {
        &self.0
    }
}

impl fmt::Debug for HarnessKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HarnessKey(..)")
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::os::fd::AsRawFd;

    use super::*;

    fn read(text: &[u8]) -> Result<HarnessKey, HarnessKeyError> {
        let (reader, mut writer) = io::pipe().unwrap();
        writer.write_all(text).unwrap();
        drop(writer);
        HarnessKey::read_fd(reader.as_raw_fd())
    }

    #[test]
    fn unit_cli_harness_key_is_64_hex_digits_on_an_fd() {
        let key: Bytes32 = std::array::from_fn(|i| i as u8);
        let hex = proof::to_hex(&key);
        for text in [hex.clone(), format!("{hex}\n"), format!("{hex}\nmore")] {
            assert_eq!(read(text.as_bytes()).unwrap(), HarnessKey::new(key));
        }
        for bad in [&hex[1..], &format!("{hex}0"), "", "\n"] {
            let err = read(bad.as_bytes()).unwrap_err();
            assert!(matches!(err, HarnessKeyError::Malformed), "{bad}: {err}");
        }
        let err = HarnessKey::read_fd(-1).unwrap_err();
        assert!(matches!(err, HarnessKeyError::Read { fd: -1, .. }), "{err}");
        assert_eq!(format!("{:?}", HarnessKey::new(key)), "HarnessKey(..)");
    }
}
