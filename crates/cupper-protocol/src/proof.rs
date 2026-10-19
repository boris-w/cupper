//! The key proofs of `dev_key` and `harness` authentication (design 5.8.1),
//! shared by the host, the CLI and the desktop's native extension.
//!
//! `proof = HMAC-SHA256(key, label ‖ h_nonce ‖ c_nonce ‖ pid)`: the label is
//! `c2h` for the client's proof and `h2c` for the host's; the nonces are the
//! 32 raw bytes of `host/hello` and of the client's `auth`; the pid, 4 bytes
//! big-endian, is the prover's own, which the verifier takes from the socket.
//! A process relaying the handshake in the middle has a pid of its own, so
//! the proofs it passes on do not hold.

use hmac::{Hmac, Mac};
use sha2::Sha256;

/// A key, a nonce or a proof; 64 hex digits on the wire.
pub type Bytes32 = [u8; 32];

/// Which side proves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// The client, in `initialize`'s `auth`.
    ClientToHost,
    /// The host, in `initialize`'s `host_proof`.
    HostToClient,
}

impl Direction {
    fn label(self) -> &'static [u8; 3] {
        match self {
            Self::ClientToHost => b"c2h",
            Self::HostToClient => b"h2c",
        }
    }
}

/// What a proof is bound to besides the pid: the nonces of one handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Channel {
    pub h_nonce: Bytes32,
    pub c_nonce: Bytes32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProofError {
    #[error("a proof is 64 hex digits")]
    Malformed,
    #[error("the proof does not hold")]
    Mismatch,
}

impl Channel {
    /// The proof, in hex, that the side going `direction`, whose pid is
    /// `pid`, holds `key`.
    pub fn prove(&self, key: &Bytes32, direction: Direction, pid: i32) -> String {
        to_hex(&self.mac(key, direction, pid).finalize().into_bytes())
    }

    /// Checks `proof` against [`prove`](Self::prove), comparing in constant
    /// time (`verify_slice`).
    pub fn check(
        &self,
        key: &Bytes32,
        direction: Direction,
        pid: i32,
        proof: &str,
    ) -> Result<(), ProofError> {
        let proof = parse_hex32(proof).ok_or(ProofError::Malformed)?;
        self.mac(key, direction, pid)
            .verify_slice(&proof)
            .map_err(|_| ProofError::Mismatch)
    }

    fn mac(&self, key: &Bytes32, direction: Direction, pid: i32) -> Hmac<Sha256> {
        let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC takes keys of any length");
        mac.update(direction.label());
        mac.update(&self.h_nonce);
        mac.update(&self.c_nonce);
        mac.update(&pid.to_be_bytes());
        mac
    }
}

/// Lowercase hex digits.
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 64 hex digits, either case.
pub fn parse_hex32(text: &str) -> Option<Bytes32> {
    let digits = text.as_bytes();
    if digits.len() != 64 {
        return None;
    }
    let mut bytes = [0; 32];
    for (byte, pair) in bytes.iter_mut().zip(digits.chunks_exact(2)) {
        let digit = |d: u8| char::from(d).to_digit(16);
        *byte = u8::try_from(digit(pair[0])? << 4 | digit(pair[1])?).ok()?;
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: Bytes32 = [1; 32];
    const CHANNEL: Channel = Channel {
        h_nonce: [2; 32],
        c_nonce: [3; 32],
    };

    #[test]
    fn unit_proof_binding_known_answer() {
        // Computed independently (Python's hmac): the byte layout other
        // languages implement.
        let key: Bytes32 = std::array::from_fn(|i| i as u8);
        let channel = Channel {
            h_nonce: [0xaa; 32],
            c_nonce: [0xbb; 32],
        };
        let pid = 0x0102_0304;
        assert_eq!(
            channel.prove(&key, Direction::ClientToHost, pid),
            "6851285dda55129ca1b078ea1150d3772c4a6442fb90ab04a0f788b47c0553be"
        );
        assert_eq!(
            channel.prove(&key, Direction::HostToClient, pid),
            "70708b5e0e8be2cbf88f3a3f1d2c0b68b2723898d29037b2734907ac5478a6af"
        );
    }

    #[test]
    fn unit_proof_binding_any_other_input_fails() {
        let proof = CHANNEL.prove(&KEY, Direction::ClientToHost, 42);
        assert_eq!(
            CHANNEL.check(&KEY, Direction::ClientToHost, 42, &proof),
            Ok(())
        );
        let upper = proof.to_uppercase();
        assert_eq!(
            CHANNEL.check(&KEY, Direction::ClientToHost, 42, &upper),
            Ok(())
        );

        let h_nonce = Channel {
            h_nonce: [4; 32],
            ..CHANNEL
        };
        let c_nonce = Channel {
            c_nonce: [4; 32],
            ..CHANNEL
        };
        let swapped = Channel {
            h_nonce: CHANNEL.c_nonce,
            c_nonce: CHANNEL.h_nonce,
        };
        let others = [
            ([9; 32], CHANNEL, Direction::ClientToHost, 42),
            (KEY, h_nonce, Direction::ClientToHost, 42),
            (KEY, c_nonce, Direction::ClientToHost, 42),
            (KEY, swapped, Direction::ClientToHost, 42),
            (KEY, CHANNEL, Direction::HostToClient, 42),
            (KEY, CHANNEL, Direction::ClientToHost, 43),
            (KEY, CHANNEL, Direction::ClientToHost, 42 << 8),
        ];
        for (key, channel, direction, pid) in others {
            let checked = channel.check(&key, direction, pid, &proof);
            assert_eq!(
                checked,
                Err(ProofError::Mismatch),
                "{channel:?} {direction:?} {pid}"
            );
        }
    }

    #[test]
    fn unit_proof_binding_format_is_64_hex_digits() {
        let proof = CHANNEL.prove(&KEY, Direction::HostToClient, 7);
        let bad = [
            &proof[1..],
            &format!("{proof}0"),
            &format!("g{}", &proof[1..]),
            &"é".repeat(32),
        ];
        for text in bad {
            assert_eq!(parse_hex32(text), None, "{text}");
            let checked = CHANNEL.check(&KEY, Direction::HostToClient, 7, text);
            assert_eq!(checked, Err(ProofError::Malformed), "{text}");
        }
        let bytes: Bytes32 = std::array::from_fn(|i| (i * 8) as u8);
        assert_eq!(parse_hex32(&to_hex(&bytes)), Some(bytes));
        assert_eq!(&to_hex(&bytes)[..8], "00081018");
    }
}
