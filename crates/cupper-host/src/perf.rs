//! The samples of the PERF tests (design 5.9.6), in test builds only. A test
//! hands each metric's samples to [`record`], which appends them as one JSON
//! line to the file that `CUPPER_PERF_OUT` names; `cargo xtask ci perf`
//! reads it, works out p50, p95 and the maximum and keeps them in the
//! history. Without the variable the samples only go to stderr.

use std::fs::OpenOptions;
use std::io::Write;

use serde_json::json;

/// Keeps the samples of `metric`, in `unit`.
pub fn record(metric: &str, unit: &str, samples: &[f64]) {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let median = sorted.get(sorted.len() / 2).copied().unwrap_or_default();
    eprintln!(
        "{metric}: {} samples, median {median:.2} {unit}",
        samples.len()
    );
    let Some(path) = std::env::var_os("CUPPER_PERF_OUT") else {
        return;
    };
    let line = json!({"metric": metric, "unit": unit, "samples": samples});
    let mut out = OpenOptions::new().create(true).append(true).open(path);
    let written = out.as_mut().map(|file| writeln!(file, "{line}"));
    written
        .expect("CUPPER_PERF_OUT takes the samples")
        .expect("written");
}
