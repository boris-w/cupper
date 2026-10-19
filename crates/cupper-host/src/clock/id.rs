//! UUIDv7 generation on the host's clock (design 5.2.16, 5.4.4).

use std::sync::{Arc, Mutex, PoisonError};

use uuid::Uuid;

use super::Clock;

/// The host's only source of UUIDv7s. The 48-bit timestamp is the wall time of
/// the shared [`Clock`], so an ID carries the same millisecond as the `ts` of the
/// event that introduces it.
///
/// The remaining 74 bits are a counter: random at each new millisecond (with the
/// top bit clear, so it cannot run over) and incremented within one, so IDs from
/// one generator are strictly increasing while the wall clock does not go back.
/// The random part comes from a generator seeded once; `IdGen::seeded` (tests and
/// `test-harness` builds) fixes the seed, which together with a `ManualClock`
/// makes the IDs reproducible.
#[derive(Debug)]
pub struct IdGen {
    clock: Arc<dyn Clock>,
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    last_ms: Option<u64>,
    counter: u128,
    rng: SplitMix64,
}

impl IdGen {
    /// Seeded from the operating system's random source.
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        let seed = getrandom::u64().expect("the operating system's random source is available");
        Self::with_seed(clock, seed)
    }

    #[cfg(any(test, feature = "test-harness"))]
    pub fn seeded(clock: Arc<dyn Clock>, seed: u64) -> Self {
        Self::with_seed(clock, seed)
    }

    fn with_seed(clock: Arc<dyn Clock>, seed: u64) -> Self {
        Self {
            clock,
            state: Mutex::new(State {
                last_ms: None,
                counter: 0,
                rng: SplitMix64(seed),
            }),
        }
    }

    /// A UUIDv7 stamped with the clock's current wall time.
    pub fn new_id(&self) -> Uuid {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        // Read under the lock, so concurrent callers stay in order.
        let ms = self.clock.now_wall_ms();
        state.mint(ms)
    }

    /// A UUIDv7 stamped with `wall_ms`. For pure cores, which take the time as
    /// the `now` of their `step` instead of reading the clock.
    pub fn new_id_at(&self, wall_ms: u64) -> Uuid {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .mint(wall_ms)
    }
}

impl State {
    fn mint(&mut self, ms: u64) -> Uuid {
        if self.last_ms == Some(ms) {
            self.counter += 1;
        } else {
            // 64 + 9 random bits: the counter's top (74th) bit stays clear.
            let high = u128::from(self.rng.next_u64());
            let low = u128::from(self.rng.next_u64() & 0x1ff);
            self.counter = high << 9 | low;
            self.last_ms = Some(ms);
        }
        compose_v7(ms, self.counter)
    }
}

/// RFC 9562 UUIDv7: 48-bit Unix milliseconds, version 7, 12 bits of `rand_a`,
/// the RFC variant, 62 bits of `rand_b`; the 74-bit `counter` fills both.
fn compose_v7(ms: u64, counter: u128) -> Uuid {
    const MS_MASK: u64 = (1 << 48) - 1;
    const RAND_B_MASK: u128 = (1 << 62) - 1;
    let rand_a = (counter >> 62) & 0xfff;
    let rand_b = counter & RAND_B_MASK;
    Uuid::from_u128(u128::from(ms & MS_MASK) << 80 | 0x7 << 76 | rand_a << 64 | 0b10 << 62 | rand_b)
}

/// SplitMix64 (Steele, Lea and Flood, 2014): IDs need uniqueness, not secrecy.
#[derive(Debug)]
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use uuid::{Variant, Version};

    use super::*;
    use crate::clock::ManualClock;

    /// The timestamp as the `uuid` crate decodes it, in Unix milliseconds.
    fn unix_ms(id: Uuid) -> u64 {
        let (secs, nanos) = id.get_timestamp().expect("a v7 timestamp").to_unix();
        secs * 1_000 + u64::from(nanos) / 1_000_000
    }

    fn generator(seed: u64) -> (ManualClock, IdGen) {
        let clock = ManualClock::new();
        let ids = IdGen::seeded(Arc::new(clock.clone()), seed);
        (clock, ids)
    }

    #[test]
    fn unit_idgen_timestamp_is_the_clock_wall_time() {
        let (clock, ids) = generator(7);
        for step in [0, 1, 999, 86_400_000] {
            clock.advance(Duration::from_millis(step));
            let id = ids.new_id();
            assert_eq!(id.get_version(), Some(Version::SortRand));
            assert_eq!(id.get_variant(), Variant::RFC4122);
            assert_eq!(unix_ms(id), clock.now_wall_ms());
        }
        let at = ids.new_id_at(1_234_567);
        assert_eq!(at.get_version(), Some(Version::SortRand));
        assert_eq!(unix_ms(at), 1_234_567);
    }

    #[test]
    fn unit_idgen_is_reproducible_under_manual_clock() {
        let run = |seed| {
            let (clock, ids) = generator(seed);
            let mut out = Vec::new();
            for _ in 0..3 {
                out.push(ids.new_id());
                out.push(ids.new_id());
                clock.advance(Duration::from_millis(40));
            }
            out
        };
        assert_eq!(run(42), run(42));
        assert_ne!(run(42), run(43));
    }

    #[test]
    fn unit_idgen_ids_increase_within_and_across_milliseconds() {
        let (clock, ids) = generator(1);
        let mut minted = Vec::new();
        for _ in 0..4 {
            minted.extend((0..1_000).map(|_| ids.new_id()));
            clock.advance(Duration::from_millis(1));
        }
        assert!(minted.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
