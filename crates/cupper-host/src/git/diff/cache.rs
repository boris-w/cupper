//! `DiffCache` (design 5.2.10): the diffs `git/diff` made, by `diff_token`,
//! for `git/file_diff` and the hunk operations to refer to. Each workspace
//! keeps its latest [`KEPT`]; a diff is gone `ttl` after it was made. A pure
//! core: the caller passes the time.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cupper_protocol::ids::DiffToken;

use crate::clock::MonoTime;

/// The diffs a workspace keeps.
pub const KEPT: usize = 32;

#[derive(Debug)]
pub struct DiffCache<V> {
    ttl: Duration,
    /// Oldest first.
    diffs: VecDeque<Cached<V>>,
}

#[derive(Debug)]
struct Cached<V> {
    token: DiffToken,
    workspace: PathBuf,
    made: MonoTime,
    diff: Arc<V>,
}

impl<V> DiffCache<V> {
    pub fn new(ttl: Duration) -> Self {
        let diffs = VecDeque::new();
        Self { ttl, diffs }
    }

    /// Keeps `diff` of `workspace`, made `now`, dropping the workspace's
    /// oldest beyond [`KEPT`].
    pub fn insert(&mut self, token: DiffToken, workspace: &Path, diff: V, now: MonoTime) {
        self.expire(now);
        let (workspace, diff) = (workspace.to_owned(), Arc::new(diff));
        let made = now;
        let mine = |c: &&Cached<V>| c.workspace == workspace;
        if self.diffs.iter().filter(mine).count() == KEPT {
            let oldest = self.diffs.iter().position(|c| c.workspace == workspace);
            self.diffs.remove(oldest.expect("the workspace has diffs"));
        }
        self.diffs.push_back(Cached {
            token,
            workspace,
            made,
            diff,
        });
    }

    /// Diff `token` of `workspace`, unless it is gone.
    pub fn get(&mut self, token: &DiffToken, workspace: &Path, now: MonoTime) -> Option<Arc<V>> {
        self.expire(now);
        let found = self.diffs.iter().find(|c| c.token == *token);
        found
            .filter(|c| c.workspace == workspace)
            .map(|c| Arc::clone(&c.diff))
    }

    fn expire(&mut self, now: MonoTime) {
        while (self.diffs.front())
            .is_some_and(|c| now.saturating_duration_since(c.made) >= self.ttl)
        {
            self.diffs.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{Clock, ManualClock};

    const TTL: Duration = Duration::from_secs(600);

    fn token(n: usize) -> DiffToken {
        format!("dt_{n}").parse().unwrap()
    }

    #[test]
    fn unit_git_diff_cache_keeps_32_per_workspace_for_10_minutes() {
        let clock = ManualClock::new();
        let (a, b) = (Path::new("/a"), Path::new("/b"));
        let mut cache = DiffCache::new(TTL);
        for n in 0..=KEPT {
            cache.insert(token(n), a, n, clock.now_mono());
        }
        cache.insert(token(100), b, 100, clock.now_mono());
        let now = clock.now_mono();
        // The oldest of `a` went; `b`'s did not count against `a`.
        assert!(cache.get(&token(0), a, now).is_none());
        assert_eq!(cache.get(&token(1), a, now).as_deref(), Some(&1));
        assert_eq!(cache.get(&token(100), b, now).as_deref(), Some(&100));
        // Another workspace's token is not this one's.
        assert!(cache.get(&token(100), a, now).is_none());

        clock.advance(TTL - Duration::from_millis(1));
        cache.insert(token(200), a, 200, clock.now_mono());
        assert!(cache.get(&token(KEPT), a, clock.now_mono()).is_some());
        clock.advance(Duration::from_millis(1));
        let now = clock.now_mono();
        assert!(cache.get(&token(KEPT), a, now).is_none(), "ten minutes on");
        assert_eq!(cache.get(&token(200), a, now).as_deref(), Some(&200));
    }
}
