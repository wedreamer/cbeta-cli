#!/usr/bin/env bash
# Lane 3 perf-full: gates + GNU time + release bin + C1 + SLO + C2 bench + PERF.md.
set -euo pipefail

EVIDENCE_DIR="${CBETA_PERF_EVIDENCE:-$PWD/.omo/evidence/perf-concurrency}"
mkdir -p "$EVIDENCE_DIR"
DEFECT_JSON="${EVIDENCE_DIR}/defect_open.json"
PERF_MD="${EVIDENCE_DIR}/PERF.md"
SUMMARY_JSONL="${EVIDENCE_DIR}/summary.jsonl"

SAMPLE_N=21
RSS_CEILING_KB=2097152

# GNU time probe: fail-closed if -f unsupported (non-GNU). Sets TIME_ELAPSED / RSS_KB.
run_time_probe() {
  local time_out=""
  if ! time_out="$(/usr/bin/time -f '%e %M' true 2>&1)"; then
    echo "perf-concurrency: FAIL — /usr/bin/time -f probe failed" >&2
    exit 1
  fi
  echo "perf-concurrency: GNU time probe ok: ${time_out}"
  TIME_ELAPSED="0"
  RSS_KB="0"
  if [[ "$time_out" =~ ([0-9]+(\.[0-9]+)?)\ +([0-9]+) ]]; then
    TIME_ELAPSED="${BASH_REMATCH[1]}"
    RSS_KB="${BASH_REMATCH[3]}"
  fi
}

# One new-process spawn under GNU time. Sets SAMPLE_ELAPSED SAMPLE_RSS SAMPLE_STDOUT SAMPLE_RC.
run_timed_sample() {
  local outfile timefile errfile rc=0
  outfile="$(mktemp)"
  timefile="$(mktemp)"
  errfile="$(mktemp)"
  # shellcheck disable=SC2064
  trap "rm -f '$outfile' '$timefile' '$errfile'" RETURN

  set +e
  env NO_COLOR=1 \
    CBETA_INDEX="${CBETA_INDEX}" \
    ${CBETA_CORPUS:+CBETA_CORPUS="${CBETA_CORPUS}"} \
    /usr/bin/time -f '%e %M' -o "$timefile" -- "$BIN" "$@" \
    >"$outfile" 2>"$errfile"
  rc=$?
  set -e

  SAMPLE_RC="$rc"
  SAMPLE_STDOUT="$(cat "$outfile")"
  SAMPLE_ELAPSED="0"
  SAMPLE_RSS="0"
  if [[ -s "$timefile" ]]; then
    local tline
    tline="$(tr -d '\n' <"$timefile")"
    if [[ "$tline" =~ ([0-9]+(\.[0-9]+)?)\ +([0-9]+) ]]; then
      SAMPLE_ELAPSED="${BASH_REMATCH[1]}"
      SAMPLE_RSS="${BASH_REMATCH[3]}"
    else
      echo "perf-concurrency: FAIL — could not parse GNU time output: ${tline}" >&2
      exit 1
    fi
  else
    echo "perf-concurrency: FAIL — empty GNU time -o output" >&2
    exit 1
  fi
}

# Correctness gates (fail-closed → exit 1, never defect_open).
assert_search_ok() {
  local out="$1"
  if ! grep -q 'T30n1578_p0268b21' <<<"$out"; then
    echo "perf-concurrency: FAIL — search missing expected line_id T30n1578_p0268b21" >&2
    exit 1
  fi
}

assert_verify_ok() {
  local out="$1"
  if ! grep -Eqi '"is_original"[[:space:]]*:[[:space:]]*true' <<<"$out"; then
    echo "perf-concurrency: FAIL — verify is_original mismatch (expected true)" >&2
    exit 1
  fi
}

assert_get_ok() {
  local out="$1"
  if ! grep -q 'T30n1578_p0268b21' <<<"$out"; then
    echo "perf-concurrency: FAIL — get wrong/missing line_id T30n1578_p0268b21" >&2
    exit 1
  fi
}

assert_info_ok() {
  local out="$1" rc="$2"
  if [[ "$rc" -ne 0 ]]; then
    echo "perf-concurrency: FAIL — info exited ${rc}" >&2
    exit 1
  fi
  if [[ -z "$out" ]]; then
    echo "perf-concurrency: FAIL — info empty stdout" >&2
    exit 1
  fi
}

# C1: 1 cold (recorded, excluded) + SAMPLE_N warm new-process runs.
run_c1_cmd() {
  local label="$1"
  shift
  local -a warm=()
  local -a warm_rss=()
  local cold_e="" cold_rss="" i=0

  echo "perf-concurrency: C1 ${label} — cold + ${SAMPLE_N} warm (new process each)"

  run_timed_sample "$@"
  case "$label" in
    search) assert_search_ok "$SAMPLE_STDOUT" ;;
    verify) assert_verify_ok "$SAMPLE_STDOUT" ;;
    get) assert_get_ok "$SAMPLE_STDOUT" ;;
    info) assert_info_ok "$SAMPLE_STDOUT" "$SAMPLE_RC" ;;
  esac
  if [[ "$SAMPLE_RC" -ne 0 ]]; then
    echo "perf-concurrency: FAIL — ${label} cold spawn exit ${SAMPLE_RC}" >&2
    exit 1
  fi
  cold_e="$SAMPLE_ELAPSED"
  cold_rss="$SAMPLE_RSS"
  echo "perf-concurrency: C1 ${label} cold elapsed=${cold_e}s rss_kb=${cold_rss} (excluded from p50/p99)"

  for ((i = 1; i <= SAMPLE_N; i++)); do
    run_timed_sample "$@"
    case "$label" in
      search) assert_search_ok "$SAMPLE_STDOUT" ;;
      verify) assert_verify_ok "$SAMPLE_STDOUT" ;;
      get) assert_get_ok "$SAMPLE_STDOUT" ;;
      info) assert_info_ok "$SAMPLE_STDOUT" "$SAMPLE_RC" ;;
    esac
    if [[ "$SAMPLE_RC" -ne 0 ]]; then
      echo "perf-concurrency: FAIL — ${label} warm[${i}] exit ${SAMPLE_RC}" >&2
      exit 1
    fi
    warm+=("$SAMPLE_ELAPSED")
    warm_rss+=("$SAMPLE_RSS")
  done

  if [[ ${#warm[@]} -ne 21 ]]; then
    echo "perf-concurrency: FAIL — ${label} warm count ${#warm[@]} != 21" >&2
    exit 1
  fi
  test "${#warm[@]}" -eq 21

  local -a sorted=()
  mapfile -t sorted < <(printf '%s\n' "${warm[@]}" | sort -n)
  local p50="${sorted[10]}"
  local p99="${sorted[20]}"
  local max_rss=0 r
  for r in "${warm_rss[@]}"; do
    if (( r > max_rss )); then
      max_rss=$r
    fi
  done

  echo "perf-concurrency: C1 ${label} p50=${p50}s p99=${p99}s max_rss_kb=${max_rss} (n=${#warm[@]} SAMPLE_N=${SAMPLE_N})"
  case "$label" in
    search)
      SEARCH_P50="$p50"
      SEARCH_P99="$p99"
      SEARCH_RSS="$max_rss"
      SEARCH_COLD_S="$cold_e"
      SEARCH_COLD_RSS="$cold_rss"
      ;;
    verify)
      VERIFY_P50="$p50"
      VERIFY_P99="$p99"
      VERIFY_RSS="$max_rss"
      VERIFY_COLD_S="$cold_e"
      VERIFY_COLD_RSS="$cold_rss"
      ;;
    get)
      GET_P50="$p50"
      GET_P99="$p99"
      GET_RSS="$max_rss"
      GET_COLD_S="$cold_e"
      GET_COLD_RSS="$cold_rss"
      ;;
    info)
      INFO_P50="$p50"
      INFO_P99="$p99"
      INFO_RSS="$max_rss"
      INFO_COLD_S="$cold_e"
      INFO_COLD_RSS="$cold_rss"
      ;;
  esac
}

run_record_only() {
  local label="$1"
  shift
  echo "perf-concurrency: record-only ${label}"
  run_timed_sample "$@"
  if [[ "$SAMPLE_RC" -ne 0 ]]; then
    echo "perf-concurrency: FAIL — record-only ${label} exit ${SAMPLE_RC}" >&2
    exit 1
  fi
  echo "perf-concurrency: record-only ${label} elapsed=${SAMPLE_ELAPSED}s rss_kb=${SAMPLE_RSS} rc=0"
}

# True if observed (float seconds) is strictly less than ceiling.
float_lt() {
  awk -v a="$1" -v b="$2" 'BEGIN { exit !(a + 0 < b + 0) }'
}

# Write defect_open.json for an SLO miss, then exit 0 (not crash).
write_defect_open() {
  local metric="$1" observed="$2" ceiling="$3" rss_kb="$4" cmd="$5"
  cat >"$DEFECT_JSON" <<EOF
{
  "status": "defect_open",
  "metric": "${metric}",
  "observed": ${observed},
  "ceiling": ${ceiling},
  "rss_kb": ${rss_kb},
  "rss_ceiling_kb": ${RSS_CEILING_KB},
  "binary": "release",
  "scope": "taisho",
  "cmd": "${cmd}"
}
EOF
  echo "perf-concurrency: SLO miss → wrote ${DEFECT_JSON} metric=${metric} observed=${observed} ceiling=${ceiling}"
  exit 0
}

# Todo 27: compare C1 p50 / search RSS ceilings. First miss → defect_open + exit 0.
compare_slo() {
  echo "perf-concurrency: SLO compare (search/verify p50<0.200; get/info p50<0.050; search max_rss<=${RSS_CEILING_KB})"
  if ! float_lt "${SEARCH_P50}" "0.200"; then
    write_defect_open "search_p50_s" "${SEARCH_P50}" "0.200" "${SEARCH_RSS}" "search"
  fi
  if ! float_lt "${VERIFY_P50}" "0.200"; then
    write_defect_open "verify_p50_s" "${VERIFY_P50}" "0.200" "${VERIFY_RSS}" "verify"
  fi
  if ! float_lt "${GET_P50}" "0.050"; then
    write_defect_open "get_p50_s" "${GET_P50}" "0.050" "${GET_RSS}" "get"
  fi
  if ! float_lt "${INFO_P50}" "0.050"; then
    write_defect_open "info_p50_s" "${INFO_P50}" "0.050" "${INFO_RSS}" "info"
  fi
  # Per-process max RSS (never sum CONC_N). Integer compare.
  if (( SEARCH_RSS > RSS_CEILING_KB )); then
    write_defect_open "search_rss_kb" "${SEARCH_RSS}" "${RSS_CEILING_KB}" "${SEARCH_RSS}" "search"
  fi
  echo "perf-concurrency: SLO all pass (no defect_open)"
  rm -f "$DEFECT_JSON"
}

# Resolve bench --scope: CBETA_BENCH_SCOPE or artifact name from meta/CURRENT (not ci-minimal on taisho).
resolve_bench_scope() {
  if [[ -n "${CBETA_BENCH_SCOPE:-}" ]]; then
    BENCH_SCOPE="$CBETA_BENCH_SCOPE"
    return
  fi
  BENCH_SCOPE=""
  if [[ -f "${CBETA_INDEX}/cbeta-meta.json" ]]; then
    BENCH_SCOPE="$(
      python3 -c '
import json,sys
p=sys.argv[1]
try:
  d=json.load(open(p))
  for k in ("scope","scope_name","artifact_scope"):
    if isinstance(d.get(k), str) and d[k]:
      print(d[k]); raise SystemExit
  aid=d.get("artifact_id") or d.get("id") or ""
  if isinstance(aid,str) and "+" in aid:
    print(aid.split("+",1)[0]); raise SystemExit
except Exception:
  pass
' "${CBETA_INDEX}/cbeta-meta.json" 2>/dev/null || true
    )"
  fi
  if [[ -z "$BENCH_SCOPE" && -e "${CBETA_INDEX}/CURRENT" ]]; then
    local cur
    cur="$(readlink -f "${CBETA_INDEX}/CURRENT" 2>/dev/null || cat "${CBETA_INDEX}/CURRENT" 2>/dev/null || true)"
    local base
    base="$(basename "${cur:-}")"
    # e.g. 2026R2-a3f91c2e or taisho-...
    if [[ "$base" == *"-"* ]]; then
      BENCH_SCOPE="${base%%-*}"
    else
      BENCH_SCOPE="$base"
    fi
  fi
  if [[ -z "$BENCH_SCOPE" ]]; then
    BENCH_SCOPE="taisho"
  fi
}

# Todo 28: C2 bench — record only; MUST NOT fail on keyword.p99_ms >= 20.
run_c2_bench() {
  resolve_bench_scope
  echo "perf-concurrency: C2 bench --json --scope ${BENCH_SCOPE} (n=50; notes only, no p99 gate)"
  local bench_out bench_err rc=0
  bench_out="$(mktemp)"
  bench_err="$(mktemp)"
  # shellcheck disable=SC2064
  trap "rm -f '$bench_out' '$bench_err'" RETURN

  set +e
  env NO_COLOR=1 CBETA_INDEX="${CBETA_INDEX}" \
    ${CBETA_CORPUS:+CBETA_CORPUS="${CBETA_CORPUS}"} \
    "$BIN" bench --scope "$BENCH_SCOPE" --json \
    >"$bench_out" 2>"$bench_err"
  rc=$?
  set -e

  if [[ "$rc" -ne 0 ]]; then
    echo "perf-concurrency: FAIL — bench exit ${rc}; stderr=$(tr '\n' ' ' <"$bench_err")" >&2
    exit 1
  fi

  C2_RAW="$(cat "$bench_out")"
  # Parse keyword/phrase/near stats without failing on high p99.
  eval "$(
    python3 - "$bench_out" <<'PY'
import json, sys
path = sys.argv[1]
with open(path) as f:
    d = json.load(f)
for mode in ("keyword", "phrase", "near"):
    m = d.get(mode) or {}
    def num(k, default=0):
        v = m.get(k, default)
        try:
            return float(v)
        except Exception:
            return float(default)
    p50 = num("p50_ms")
    p99 = num("p99_ms")
    qps = num("qps")
    n = int(num("n", 50))
    tag = mode.upper()
    print(f'C2_{tag}_P50_MS={p50}')
    print(f'C2_{tag}_P99_MS={p99}')
    print(f'C2_{tag}_QPS={qps}')
    print(f'C2_{tag}_N={n}')
print(f'C2_SCOPE={json.dumps(d.get("scope") or "")}')
print(f'C2_ARTIFACT={json.dumps(d.get("artifact_id") or "")}')
PY
  )"

  # Notes: C2 (in-process ms) vs C1 (new-process seconds) — different units; record delta as notes only.
  C2_NOTES="C2 bench n=50 scope=${BENCH_SCOPE}; keyword p50_ms=${C2_KEYWORD_P50_MS} p99_ms=${C2_KEYWORD_P99_MS} qps=${C2_KEYWORD_QPS}; phrase p50_ms=${C2_PHRASE_P50_MS} p99_ms=${C2_PHRASE_P99_MS}; near p50_ms=${C2_NEAR_P50_MS} p99_ms=${C2_NEAR_P99_MS}; vs C1 search_p50_s=${SEARCH_P50} (new-process wall; not comparable 1:1). No p99_ms gate."
  echo "perf-concurrency: C2 notes: ${C2_NOTES}"
  # Explicit: never compare p99_ms against 20 as pass/fail.
}

# Todo 29: PERF.md + summary.jsonl from this FULL run.
write_perf_evidence() {
  local host
  host="$(hostname 2>/dev/null || echo unknown)"
  local ts
  ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

  cat >"$PERF_MD" <<EOF
# PERF — cbeta lane 3 (perf-concurrency)

- timestamp_utc: ${ts}
- hostname: ${host}
- binary: release (\`${BIN}\`)
- SAMPLE_N: ${SAMPLE_N} (1 cold excluded + ${SAMPLE_N} warm new-process)
- scope_label: taisho (SLO fields); bench_scope: ${BENCH_SCOPE:-?}
- rss_ceiling_kb: ${RSS_CEILING_KB} (per process, never sum CONC_N)

## C1 warm percentiles (seconds)

| cmd | cold_s | cold_rss_kb | p50_s | p99_s | max_rss_kb | ceiling_p50_s |
|-----|--------|-------------|-------|-------|------------|---------------|
| search --json | ${SEARCH_COLD_S} | ${SEARCH_COLD_RSS} | ${SEARCH_P50} | ${SEARCH_P99} | ${SEARCH_RSS} | 0.200 |
| verify --json | ${VERIFY_COLD_S} | ${VERIFY_COLD_RSS} | ${VERIFY_P50} | ${VERIFY_P99} | ${VERIFY_RSS} | 0.200 |
| get -C 4 --json | ${GET_COLD_S} | ${GET_COLD_RSS} | ${GET_P50} | ${GET_P99} | ${GET_RSS} | 0.050 |
| info --json | ${INFO_COLD_S} | ${INFO_COLD_RSS} | ${INFO_P50} | ${INFO_P99} | ${INFO_RSS} | 0.050 |

## C2 bench (in-process, n=50) — notes only

- keyword: p50_ms=${C2_KEYWORD_P50_MS} p99_ms=${C2_KEYWORD_P99_MS} qps=${C2_KEYWORD_QPS} n=${C2_KEYWORD_N}
- phrase: p50_ms=${C2_PHRASE_P50_MS} p99_ms=${C2_PHRASE_P99_MS} qps=${C2_PHRASE_QPS} n=${C2_PHRASE_N}
- near: p50_ms=${C2_NEAR_P50_MS} p99_ms=${C2_NEAR_P99_MS} qps=${C2_NEAR_QPS} n=${C2_NEAR_N}
- notes: ${C2_NOTES}

C2 p99 is **not** a pass/fail gate (historical 20ms is notes-only).
EOF

  # summary.jsonl: one object per logical row (overwrite file each FULL run).
  SAMPLE_N="$SAMPLE_N" RSS_CEILING_KB="$RSS_CEILING_KB" \
  SEARCH_COLD_S="$SEARCH_COLD_S" SEARCH_COLD_RSS="$SEARCH_COLD_RSS" \
  SEARCH_P50="$SEARCH_P50" SEARCH_P99="$SEARCH_P99" SEARCH_RSS="$SEARCH_RSS" \
  VERIFY_COLD_S="$VERIFY_COLD_S" VERIFY_COLD_RSS="$VERIFY_COLD_RSS" \
  VERIFY_P50="$VERIFY_P50" VERIFY_P99="$VERIFY_P99" VERIFY_RSS="$VERIFY_RSS" \
  GET_COLD_S="$GET_COLD_S" GET_COLD_RSS="$GET_COLD_RSS" \
  GET_P50="$GET_P50" GET_P99="$GET_P99" GET_RSS="$GET_RSS" \
  INFO_COLD_S="$INFO_COLD_S" INFO_COLD_RSS="$INFO_COLD_RSS" \
  INFO_P50="$INFO_P50" INFO_P99="$INFO_P99" INFO_RSS="$INFO_RSS" \
  BENCH_SCOPE="${BENCH_SCOPE:-}" C2_NOTES="${C2_NOTES:-}" \
  C2_KEYWORD_P50_MS="${C2_KEYWORD_P50_MS}" C2_KEYWORD_P99_MS="${C2_KEYWORD_P99_MS}" \
  C2_KEYWORD_QPS="${C2_KEYWORD_QPS}" C2_KEYWORD_N="${C2_KEYWORD_N}" \
  C2_PHRASE_P50_MS="${C2_PHRASE_P50_MS}" C2_PHRASE_P99_MS="${C2_PHRASE_P99_MS}" \
  C2_PHRASE_QPS="${C2_PHRASE_QPS}" C2_PHRASE_N="${C2_PHRASE_N}" \
  C2_NEAR_P50_MS="${C2_NEAR_P50_MS}" C2_NEAR_P99_MS="${C2_NEAR_P99_MS}" \
  C2_NEAR_QPS="${C2_NEAR_QPS}" C2_NEAR_N="${C2_NEAR_N}" \
  SUMMARY_JSONL="$SUMMARY_JSONL" \
  python3 - <<'PY'
import json, os
def f(k, default=0.0):
    try:
        return float(os.environ.get(k, default))
    except Exception:
        return float(default)
def i(k, default=0):
    try:
        return int(float(os.environ.get(k, default)))
    except Exception:
        return int(default)
n = i("SAMPLE_N", 21)
rows = [
  {"kind":"c1","cmd":"search","binary":"release","sample_n":n,
   "cold_s":f("SEARCH_COLD_S"),"cold_rss_kb":i("SEARCH_COLD_RSS"),
   "p50_s":f("SEARCH_P50"),"p99_s":f("SEARCH_P99"),"max_rss_kb":i("SEARCH_RSS"),
   "ceiling_p50_s":0.200,"rss_ceiling_kb":i("RSS_CEILING_KB",2097152)},
  {"kind":"c1","cmd":"verify","binary":"release","sample_n":n,
   "cold_s":f("VERIFY_COLD_S"),"cold_rss_kb":i("VERIFY_COLD_RSS"),
   "p50_s":f("VERIFY_P50"),"p99_s":f("VERIFY_P99"),"max_rss_kb":i("VERIFY_RSS"),
   "ceiling_p50_s":0.200},
  {"kind":"c1","cmd":"get","binary":"release","sample_n":n,
   "cold_s":f("GET_COLD_S"),"cold_rss_kb":i("GET_COLD_RSS"),
   "p50_s":f("GET_P50"),"p99_s":f("GET_P99"),"max_rss_kb":i("GET_RSS"),
   "ceiling_p50_s":0.050},
  {"kind":"c1","cmd":"info","binary":"release","sample_n":n,
   "cold_s":f("INFO_COLD_S"),"cold_rss_kb":i("INFO_COLD_RSS"),
   "p50_s":f("INFO_P50"),"p99_s":f("INFO_P99"),"max_rss_kb":i("INFO_RSS"),
   "ceiling_p50_s":0.050},
  {"kind":"c2_bench","n":50,"scope":os.environ.get("BENCH_SCOPE",""),
   "keyword":{"p50_ms":f("C2_KEYWORD_P50_MS"),"p99_ms":f("C2_KEYWORD_P99_MS"),
              "qps":f("C2_KEYWORD_QPS"),"n":i("C2_KEYWORD_N",50)},
   "phrase":{"p50_ms":f("C2_PHRASE_P50_MS"),"p99_ms":f("C2_PHRASE_P99_MS"),
             "qps":f("C2_PHRASE_QPS"),"n":i("C2_PHRASE_N",50)},
   "near":{"p50_ms":f("C2_NEAR_P50_MS"),"p99_ms":f("C2_NEAR_P99_MS"),
           "qps":f("C2_NEAR_QPS"),"n":i("C2_NEAR_N",50)},
   "notes":os.environ.get("C2_NOTES","")},
]
path = os.environ["SUMMARY_JSONL"]
with open(path, "w") as out:
    for row in rows:
        out.write(json.dumps(row, ensure_ascii=False) + "\n")
PY

  echo "perf-concurrency: wrote ${PERF_MD}"
  echo "perf-concurrency: wrote ${SUMMARY_JSONL}"
}

# (1) Crash gate: nonzero, no defect_open.json (delete stale if present).
if [[ "${CBETA_PERF_DRY:-}" == "crash" ]]; then
  echo "perf-concurrency: crash gate (intentional fail-closed)" >&2
  rm -f "$DEFECT_JSON"
  exit 1
fi

# (2) DRY=1: GNU time probe (mandatory, no index) + fake SLO miss → defect_open.json.
if [[ "${CBETA_PERF_DRY:-}" == "1" ]]; then
  run_time_probe
  observed="0.300"
  ceiling="0.200"
  cat >"$DEFECT_JSON" <<EOF
{
  "status": "defect_open",
  "metric": "search_p50_s",
  "observed": ${observed},
  "ceiling": ${ceiling},
  "rss_kb": ${RSS_KB},
  "rss_ceiling_kb": ${RSS_CEILING_KB},
  "binary": "release",
  "scope": "taisho",
  "cmd": "search"
}
EOF
  echo "perf-concurrency: DRY=1 wrote ${DEFECT_JSON} (fake search_p50 miss; no SAMPLE_N loop)"
  exit 0
fi

# (3) Skip unless CBETA_FULL is exactly 1 — no cargo, no index work.
if [[ "${CBETA_FULL:-}" != "1" ]]; then
  echo "perf-concurrency: SKIP — set CBETA_FULL=1 for full lane (not an SLO pass)"
  exit 0
fi

# (4) CBETA_FULL=1: require usable index; never default to $HOME/.cbeta/index.
if [[ -z "${CBETA_INDEX:-}" ]]; then
  echo "perf-concurrency: missing CBETA_INDEX" >&2
  exit 2
fi
if [[ ! -f "${CBETA_INDEX}/cbeta-meta.json" ]]; then
  echo "perf-concurrency: missing index meta at ${CBETA_INDEX}/cbeta-meta.json" >&2
  exit 2
fi
if [[ ! -e "${CBETA_INDEX}/CURRENT" ]]; then
  echo "perf-concurrency: missing CURRENT under ${CBETA_INDEX}" >&2
  exit 2
fi

# (5) Index present: probe + release bin + C1 + SLO + C2 + PERF evidence.
run_time_probe
echo "perf-concurrency: building release binary (cargo build --release -p cbeta-cli --locked)"
cargo build --release -p cbeta-cli --locked
BIN="${PWD}/target/release/cbeta"
if [[ ! -x "$BIN" ]]; then
  echo "perf-concurrency: FAIL — missing release bin at ${BIN}" >&2
  exit 1
fi
echo "perf-concurrency: release bin ready at ${BIN} (time_elapsed=${TIME_ELAPSED}s rss_kb=${RSS_KB})"

run_c1_cmd search --json "真性有为空"
run_c1_cmd verify verify --json "真性有為空，如幻緣生故"
run_c1_cmd get get "T30n1578_p0268b21" -C 4 --json
run_c1_cmd info info --json

run_record_only catalog catalog --author "玄奘"
run_record_only read read "T0235" --juan 1

echo "perf-concurrency: C1 loop done SAMPLE_N=${SAMPLE_N}"
echo "perf-concurrency: summary search_p50=${SEARCH_P50:-?} verify_p50=${VERIFY_P50:-?} get_p50=${GET_P50:-?} info_p50=${INFO_P50:-?}"

# C2 before SLO so notes exist even if SLO writes defect and exits 0.
run_c2_bench
export BENCH_SCOPE C2_NOTES
write_perf_evidence

# SLO last among measurements: miss → defect_open + exit 0; crash paths already exited 1.
compare_slo
exit 0
