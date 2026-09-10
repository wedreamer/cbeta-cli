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
            crate::index_hint::eprint_no_index(&p);
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::env_paths::env_lock;
    use cbeta_core::{Action, Filters, Format};

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "cbeta-bench-ut-{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn mini_corpus() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini")
    }

    fn bench_cmd(format: Format) -> Command {
        Command {
            action: Action::Bench,
            q: Some("ci-minimal".into()),
            filters: Filters::default(),
            format,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        }
    }

    #[test]
    fn percentile_empty_is_zero() {
        assert_eq!(percentile_ms(&[], 50), 0.0);
        assert_eq!(percentile_ms(&[], 99), 0.0);
    }

    #[test]
    fn percentile_single_sample() {
        let s = [Duration::from_millis(10)];
        assert!((percentile_ms(&s, 50) - 10.0).abs() < 0.01);
        assert!((percentile_ms(&s, 99) - 10.0).abs() < 0.01);
    }

    #[test]
    fn run_missing_index_exits_2() {
        let _g = env_lock();
        let empty = temp_dir("empty");
        std::env::set_var("CBETA_INDEX", &empty);
        std::env::set_var("CBETA_CORPUS", mini_corpus());
        assert_eq!(run(&bench_cmd(Format::Json)), 2);
        std::env::remove_var("CBETA_INDEX");
        std::env::remove_var("CBETA_CORPUS");
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn run_index_root_error_exits_2() {
        let _g = env_lock();
        std::env::remove_var("CBETA_INDEX");
        std::env::remove_var("HOME");
        assert_eq!(run(&bench_cmd(Format::Tty)), 2);
    }

    #[test]
    fn run_with_mini_index_tty_and_json() {
        let _g = env_lock();
        let corpus = mini_corpus();
        let index = temp_dir("idx");
        std::env::set_var("CBETA_CORPUS", &corpus);
        std::env::set_var("CBETA_INDEX", &index);
        let build = Command {
            action: Action::Build,
            q: Some("ci-minimal".into()),
            filters: Filters::default(),
            format: Format::Plain,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(crate::cmd_build::run(&build, false), 0);
        assert_eq!(run(&bench_cmd(Format::Tty)), 0);
        assert_eq!(run(&bench_cmd(Format::Json)), 0);
        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        let _ = std::fs::remove_dir_all(&index);
    }
}
