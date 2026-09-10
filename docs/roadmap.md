# Roadmap

Tracking issue: https://github.com/wedreamer/cbeta-cli/issues/1

Data repo: https://github.com/wedreamer/cbeta-corpus

## P0 — human can search (LANDED 2026-09)

| ID | Repo | Work | Acceptance |
|---|---|---|---|
| C-fetch | corpus | `scripts/fetch.sh` pins xml-p5@2026R2 + metadata + gaiji | `FETCHED.yaml` has commits |
| C-scope | corpus | `scopes/taisho.yaml` + `select_scope.py` | writes MANIFEST/catalog/files/NOTICE |
| C-verify | corpus | `verify_lock.py` | excludes Y/TX/LC/YP; checksums |
| C-catalog | corpus | catalog.jsonl fields | title/author/dynasty/category/work_id |
| L-ws | cli | Rust workspace + Command/Hit | `cbeta --help`; flag ≡ JSON field table |
| L-parse | cli | TEI parser | fixtures T0235 / T1578 / T1585 emit line_id |
| L-norm | cli | OpenCC + variants + gaiji + strip punct | 空性 finds 空性 |
| L-index | cli | Tantivy char n-gram + unigram positions | `cbeta build --scope taisho` |
| L-search | cli | keyword/phrase TTY + --json | exit 0/1/2; highlight; line_id |
| L-cat | cli | catalog / info / filters | `cbeta catalog --author 玄奘` |

## P1 — proximity + verify + MCP (LANDED 2026-09)

| ID | Work | Acceptance |
|---|---|---|
| L-dsl | NEAR/BEFORE/AND/OR/NOT/? + CBReader aliases | `--explain '空性+缘生'` → near/30 |
| L-near | 2-gram recall + text_norm span distance | `真如 NEAR/16 缘起` |
| L-verify | quote check + similar | user verse → not original, top hit T1578 |
| L-get | get/read/cite + `-C` | prints `(CBETA 2026.R2, T30, no. 1578, …)` |
| L-in | `--work` / `--script` / `--explain` | 本经内搜 + 简繁显示 |
| L-mcp | `cbeta serve` stdio | every CLI flag exists on MCP |

## P2 — speed + human loop (LANDED 2026-09)

HTTP (`serve --http`) + `bench` + atomic index swap (tmp stage + rename); REPL (bare `cbeta`, colon commands); `--save/--from last`; `completion <shell>`.

## P3 — optional (not scheduled)

`semantic_search` as its own tool. Never used for `is_original`.

## Still planned (post-v0.1)

`--window` flag, `--category` filter, pager, JSONL pipe default output, fuzzy engine (1–2 char).
