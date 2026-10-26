//! `perf` (nightly, under the machine lock; design 5.9.6): the host's PERF
//! cases on a release build, one at a time, and PERF-snapshot (T186). Each
//! test hands its samples to the file in `CUPPER_PERF_OUT`; the job works
//! out n, p50, p95 and the maximum of each metric, appends them to
//! [`HISTORY`] and fails when a metric's p50 is over [`REGRESSION`] times the
//! median p50 of its history on the same kind of machine. CI runners are
//! not reference hardware, so only the trend counts here.
//!
//! A history line holds the date, the commit, the version, the generic model
//! name (`facts::machine`), the metric, its unit, n, p50, p95 and max: no
//! machine name and no path.

use std::fs;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::ci::ctx::JobCtx;
use crate::ci::{facts, git};
use crate::clock;

/// The history of every nightly run, oldest first.
pub const HISTORY: &str = "testdata/perf/history.jsonl";
/// A p50 over this many times the history's median p50 is a regression.
pub const REGRESSION: f64 = 1.5;

pub fn run(ctx: &mut JobCtx) -> Result<()> {
    let samples = ctx.step_file("samples", "jsonl");
    let _ = fs::remove_file(&samples);
    // The host's tests start the fake agent beside their own binary (T055).
    let mut fake_agent = ctx.cargo();
    fake_agent.args(["build", "--release", "-p", "cupper-fake-agent", "--locked"]);
    ctx.check("fake-agent", &mut fake_agent)?;
    let mut host = ctx.cargo();
    host.args([
        "test",
        "--release",
        "-p",
        "cupper-host",
        "--features",
        "test-harness",
    ])
    .args([
        "--locked",
        "--test",
        "perf",
        "--",
        "--ignored",
        "--test-threads=1",
    ])
    .env("CUPPER_PERF_OUT", &samples);
    ctx.libtest("perf-host", &mut host)?;
    let mut snapshot = ctx.cargo();
    snapshot
        .args([
            "test",
            "--release",
            "-p",
            "cupper-host",
            "--lib",
            "--locked",
        ])
        .args(["--", "--ignored", "perf_snapshot"])
        .env("CUPPER_PERF_OUT", &samples);
    ctx.libtest("perf-snapshot", &mut snapshot)?;

    let text = fs::read_to_string(&samples).context("the PERF tests wrote no samples")?;
    let measured = text
        .lines()
        .map(Measured::parse)
        .collect::<Result<Vec<_>>>()?;
    let history_path = ctx.root.join(HISTORY);
    let previous = fs::read_to_string(&history_path).unwrap_or_default();
    let history: Vec<Value> = previous
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()
        .context("read the PERF history")?;
    let run = Run {
        date: date(u64::try_from(clock::unix_ms() / 86_400_000).unwrap_or(0)),
        commit: git::head(ctx.root)?[..12].to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        machine: facts::machine(ctx.root),
    };
    let lines: Vec<Value> = measured.iter().map(|m| m.line(&run)).collect();
    let regressions = regressions(&history, &lines);
    let added: String = lines.iter().map(|line| format!("{line}\n")).collect();
    fs::create_dir_all(history_path.parent().expect("a directory"))?;
    fs::write(&history_path, previous + &added).context("write the PERF history")?;
    for line in &lines {
        let text = |key: &str| line[key].as_str().unwrap_or_default().to_owned();
        let (metric, unit) = (text("metric"), text("unit"));
        eprintln!(
            "  {metric:<30} p50 {} p95 {} max {} {unit}",
            line["p50"], line["p95"], line["max"]
        );
    }
    if regressions.is_empty() {
        ctx.passed += 1;
        return Ok(());
    }
    ctx.failed += 1;
    bail!("regressed: {}", regressions.join("; "))
}

/// One metric's samples, as a test recorded them.
#[derive(Debug, PartialEq)]
struct Measured {
    metric: String,
    unit: String,
    samples: Vec<f64>,
}

/// What every history line of a run shares.
struct Run {
    date: String,
    commit: String,
    version: String,
    machine: String,
}

impl Measured {
    fn parse(line: &str) -> Result<Self> {
        let value: Value = serde_json::from_str(line).context("a samples line")?;
        let text = |key: &str| {
            let found = value[key].as_str().map(str::to_owned);
            found.with_context(|| format!("no `{key}` in a samples line"))
        };
        let samples = value["samples"].as_array().context("no `samples`")?;
        let samples: Option<Vec<f64>> = samples.iter().map(Value::as_f64).collect();
        Ok(Self {
            metric: text("metric")?,
            unit: text("unit")?,
            samples: samples.context("a sample that is not a number")?,
        })
    }

    /// Its history line: n, p50, p95 and max at two decimals. With fewer
    /// than 20 samples p95 is the maximum.
    fn line(&self, run: &Run) -> Value {
        let mut sorted = self.samples.clone();
        sorted.sort_by(f64::total_cmp);
        let at = |p: f64| {
            let index = ((sorted.len() as f64 - 1.0) * p).round() as usize;
            round(sorted.get(index).copied().unwrap_or_default())
        };
        let max = round(sorted.last().copied().unwrap_or_default());
        let p95 = if sorted.len() < 20 { max } else { at(0.95) };
        json!({"date": run.date, "commit": run.commit, "version": run.version,
            "machine": run.machine, "metric": self.metric, "unit": self.unit,
            "n": sorted.len(), "p50": at(0.5), "p95": p95, "max": max})
    }
}

fn round(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// The metrics of `lines` whose p50 is over [`REGRESSION`] times the median
/// p50 of the same metric on the same machine in `history`; a metric
/// without history passes.
fn regressions(history: &[Value], lines: &[Value]) -> Vec<String> {
    let mut found = Vec::new();
    for line in lines {
        let same = |h: &&Value| h["metric"] == line["metric"] && h["machine"] == line["machine"];
        let mut past: Vec<f64> = history
            .iter()
            .filter(same)
            .filter_map(|h| h["p50"].as_f64())
            .collect();
        if past.is_empty() {
            continue;
        }
        past.sort_by(f64::total_cmp);
        let middle = past.len() / 2;
        let median = match past.len() % 2 {
            1 => past[middle],
            _ => (past[middle - 1] + past[middle]) / 2.0,
        };
        let p50 = line["p50"].as_f64().unwrap_or_default();
        if p50 > REGRESSION * median {
            let metric = line["metric"].as_str().unwrap_or_default();
            found.push(format!("{metric} p50 {p50} over {REGRESSION} × {median}"));
        }
    }
    found
}

/// `YYYY-MM-DD` of the day `days` after 1970-01-01 (UTC).
fn date(days: u64) -> String {
    // Howard Hinnant's `civil_from_days`, for days since the epoch.
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run() -> Run {
        Run {
            date: "2026-10-26".into(),
            commit: "0123456789ab".into(),
            version: "0.0.0".into(),
            machine: "MacBook Pro (Apple M5 Pro)".into(),
        }
    }

    fn measured(metric: &str, samples: &[f64]) -> Value {
        let samples = samples.to_vec();
        let (metric, unit) = (metric.to_owned(), "ms".to_owned());
        Measured {
            metric,
            unit,
            samples,
        }
        .line(&run())
    }

    #[test]
    fn unit_ci_perf_lines_hold_the_percentiles_and_nothing_of_the_machine() {
        let samples: Vec<f64> = (1..=30).map(f64::from).collect();
        let line = measured("index_snapshot", &samples);
        assert_eq!(
            (&line["n"], &line["p50"], &line["p95"], &line["max"]),
            (&json!(30), &json!(16.0), &json!(29.0), &json!(30.0))
        );
        // Few samples: p95 is the maximum.
        let few = measured("snapshot", &[3.0, 1.0, 2.0]);
        assert_eq!((&few["p50"], &few["p95"]), (&json!(2.0), &json!(3.0)));
        let keys: Vec<&String> = line.as_object().unwrap().keys().collect();
        let mut keys: Vec<&str> = keys.into_iter().map(String::as_str).collect();
        keys.sort_unstable();
        let expected = [
            "commit", "date", "machine", "max", "metric", "n", "p50", "p95", "unit", "version",
        ];
        assert_eq!(keys, expected);
        let parsed = Measured::parse(r#"{"metric": "m", "unit": "ms", "samples": [1, 2.5]}"#);
        assert_eq!(parsed.unwrap().samples, [1.0, 2.5]);
        assert!(Measured::parse(r#"{"metric": "m", "unit": "ms", "samples": ["x"]}"#).is_err());
        assert_eq!(
            (date(0), date(20_752)),
            ("1970-01-01".into(), "2026-10-26".into())
        );
        assert_eq!(date(11_016), "2000-02-29");
    }

    #[test]
    fn unit_ci_perf_regression_is_over_one_and_a_half_times_the_median() {
        let past = |metric: &str, machine: &str, p50: f64| json!({"metric": metric, "machine": machine, "p50": p50});
        let ours = "MacBook Pro (Apple M5 Pro)";
        let history = [
            past("index_snapshot", ours, 10.0),
            past("index_snapshot", ours, 12.0),
            past("index_snapshot", ours, 40.0),
            past("index_snapshot", "Mac mini (Apple M2)", 1.0),
            past("host_memory", ours, 30.0),
        ];
        let now = |metric: &str, p50: f64| measured(metric, &[p50]);
        // The median of 10, 12 and 40 is 12: up to 18 passes.
        assert!(regressions(&history, &[now("index_snapshot", 18.0)]).is_empty());
        let found = regressions(&history, &[now("index_snapshot", 18.5)]);
        assert_eq!(found, ["index_snapshot p50 18.5 over 1.5 × 12"]);
        // Another machine's history and a metric without any count for nothing.
        assert!(regressions(&history, &[now("host_cold_start", 1000.0)]).is_empty());
        let even = [past("m", ours, 10.0), past("m", ours, 20.0)];
        assert!(regressions(&even, &[now("m", 22.5)]).is_empty());
        assert_eq!(regressions(&even, &[now("m", 22.6)]).len(), 1);
    }
}
