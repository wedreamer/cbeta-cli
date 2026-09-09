//! `cbeta bench`: CLI-only keyword/phrase/near micro-benchmark.

use std::time::{Duration, Instant};

use cbeta_core::{parse_query, Command, Filters, Format};
use cbeta_search::{open_search_index, search_on, Error as SearchError, DEFAULT_LIMIT};
use serde::Serialize;

use crate::cmd_catalog::load_info;
use crate::env_paths::index_root;

const WARMUP: usize = 5;
const SAMPLES: usize = 50;

#[derive(Serialize)]
struct ModeStats {
    p50_ms: f64,
    p99_ms: f64,
    qps: f64,
    n: u64,
}

#[derive(Serialize)]
struct BenchReport {
    scope: String,
    artifact_id: String,
    keyword: ModeStats,
    phrase: ModeStats,
    near: ModeStats,
}

/// Run micro-benchmark; exit 0 on success / 2 no-index or parse error.
///
/// Does not rebuild. Times only [`search_on`] on a reused reader.
pub fn run(cmd: &Command) -> i32 {
    let root = match index_root() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };

    let (index, fields, art) = match open_search_index(&root) {
        Ok(t) => t,
        Err(SearchError::NoIndex(p)) => {
            eprintln!("no index found under {p}; run: cbeta build --scope <name>");
            return 2;
        }
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };

    let reader = match index.reader() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };

    let artifact_id = match load_info() {
        Ok(info) => info.artifact_id,
        Err(_) => {
            // WHY: product id is tag+hash; on-disk dir is tag-hash.
            let name = art.file_name().and_then(|s| s.to_str()).unwrap_or_default();
            if let Some((tag, hash)) = name.split_once('-') {
                format!("{tag}+{hash}")
            } else {
                name.to_string()
            }
        }
    };

    let scope = cmd.q.clone().unwrap_or_default();
    let filters = Filters::default();

    let keyword_q = match parse_query("真性有为空") {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let mut phrase_q = match parse_query("如幻緣生故") {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    // WHY: parse_query alone is keyword; phrase lane needs explicit mode.
    phrase_q.mode = "phrase".into();
    let near_q = match parse_query("真性+缘生") {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };

    let keyword =
        match measure_mode(|| search_on(&reader, &fields, &keyword_q, &filters, DEFAULT_LIMIT)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{e}");
                return 2;
            }
        };
    let phrase =
        match measure_mode(|| search_on(&reader, &fields, &phrase_q, &filters, DEFAULT_LIMIT)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{e}");
                return 2;
            }
        };
    let near = match measure_mode(|| search_on(&reader, &fields, &near_q, &filters, DEFAULT_LIMIT))
    {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };

    let report = BenchReport {
        scope,
        artifact_id,
        keyword,
        phrase,
        near,
    };

    match cmd.format {
        Format::Json => {
            #[allow(clippy::expect_used)]
            {
                // WHY: serde_json pretty never fails on this plain struct.
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).expect("bench json")
                );
            }
        }
        Format::Tty | Format::Plain | Format::Jsonl => {
            println!(
                "bench scope={} artifact={} n={}",
                report.scope, report.artifact_id, SAMPLES
            );
            print_mode("keyword", &report.keyword);
            print_mode("phrase", &report.phrase);
            print_mode("near", &report.near);
        }
    }

    0
}

fn print_mode(name: &str, s: &ModeStats) {
    println!(
        "  {name}: p50={:.3}ms p99={:.3}ms qps={:.1}",
        s.p50_ms, s.p99_ms, s.qps
    );
}

fn measure_mode<F, T>(mut once: F) -> Result<ModeStats, SearchError>
where
    F: FnMut() -> Result<T, SearchError>,
{
    for _ in 0..WARMUP {
        let _ = once()?;
    }

    let mut samples = Vec::with_capacity(SAMPLES);
    let wall = Instant::now();
    for _ in 0..SAMPLES {
        let t0 = Instant::now();
        let _ = once()?;
        samples.push(t0.elapsed());
    }
    let wall = wall.elapsed();

    samples.sort_unstable();
    let n = samples.len();
    let p50 = percentile_ms(&samples, 50);
    let p99 = percentile_ms(&samples, 99);
    let secs = wall.as_secs_f64().max(f64::EPSILON);
    let qps = n as f64 / secs;

    Ok(ModeStats {
        p50_ms: p50,
        p99_ms: p99,
        qps,
        n: n as u64,
    })
}

/// Nearest-rank percentile on sorted samples: index `(n-1)*pct/100`.
fn percentile_ms(sorted: &[Duration], pct: usize) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let n = sorted.len();
    let idx = (n - 1).saturating_mul(pct) / 100;
    sorted[idx.min(n - 1)].as_secs_f64() * 1000.0
}
