# Roadmap

Tracking: https://github.com/wedreamer/cbeta-cli/issues/1  
Data repo: https://github.com/wedreamer/cbeta-corpus

## Now — v0.2 corpus lifecycle

| Surface | Status |
|---|---|
| `cbeta fetch --release TAG\|latest --scope …` | landed (pins + FETCHED.yaml; never CURRENT) |
| `cbeta releases` / `releases --remote` | landed |
| `cbeta use TAG` / `use --default` / `current` | landed (build then switch CURRENT) |
| `cbeta pull` / `pull --apply` | landed (report/fetch; never CURRENT) |
| `cbeta build` incremental + `--full` + PROGRESS resume | landed |
| `cbeta gc` / `prune` / `prune --dry-run` | landed (never delete CURRENT) |
| flock + disk precheck on fetch/build | landed |
| `cbeta info` TTY extras (stale / disk hint) | landed |
| no-index / ghost-get human hints | landed |

## Next

- Scope materialization polish inside fetch (`select_scope` parity)
- Stronger offline mirror docs / `CBETA_GIT_BASE` recipes
- `info --json` optional stale fields (serde default; no 7th MCP tool)

## Later

- `--window` flag (human-ux rule 5–6)
- pager
- JSONL pipe default
- fuzzy engine (1–2 char)
- P3 `semantic_search` as its own optional tool (never for `is_original`)
- `cbeta --release TAG get` (compat; not scheduled)

## Landed earlier (v0.1)

**P0** keyword/phrase search, `build`, `get`, `catalog`/`info`.  
**P1** near/before + span, `verify`, `get -C`, `read`/`cite`/`--copy`, `--script`, stdio MCP (five tools).  
**P2** HTTP serve, REPL, `--save/--from last`, completion, `bench`, atomic index swap.

Citation stays `(CBETA 2026.R2, T30, no. 1578, p. 268, b21)`.
