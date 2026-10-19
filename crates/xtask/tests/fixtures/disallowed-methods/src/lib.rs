//! Every call below is on the `disallowed-methods` list of the workspace `clippy.toml`,
//! so `cargo clippy -- -D warnings` must fail on this crate, once per listed method.

use std::time::Duration;

pub fn std_time() {
    let mono = std::time::Instant::now();
    let _ = mono.elapsed();
    let wall = std::time::SystemTime::now();
    let _ = wall.elapsed();
    std::thread::sleep(Duration::ZERO);
}

pub async fn tokio_time() {
    let start = tokio::time::Instant::now();
    tokio::time::sleep(Duration::ZERO).await;
    tokio::time::sleep_until(start).await;
    let _ = tokio::time::interval(Duration::from_millis(1));
    let _ = tokio::time::interval_at(start, Duration::from_millis(1));
    let _ = tokio::time::timeout(Duration::ZERO, async {}).await;
    let _ = tokio::time::timeout_at(start, async {}).await;
}

pub fn ids() -> uuid::Uuid {
    uuid::Uuid::now_v7()
}
