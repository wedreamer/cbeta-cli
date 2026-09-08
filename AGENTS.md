# PROJECT KNOWLEDGE BASE

**Generated:** 2026-09-08T23:03:57+08:00
**Commit:** 811a159
**Branch:** master

## OVERVIEW

Offline CBETA search CLI (`cbeta`). Humans type the CLI; MCP/HTTP are the same `Command` over another transport. Rust 2021 Cargo workspace. **Scaffold (2026-09-08):** parse/index/search are stubs; search without `--json`/`--explain` exits 2.

Corpus is **not** this repo: [wedreamer/cbeta-corpus](https://github.com/wedreamer/cbeta-corpus) pins `xml-p5@2026R2`. Tracking: [#1](https://github.com/wedreamer/cbeta-cli/issues/1). Product contract lives in README + `docs/` — CLI examples there are the API, not current runtime.

## AGENT LANGUAGE

- 对用户优先说**简体中文**。代码、标识符、命令、路径、crate 名保持原文。
- 与产品约定分开：经文输入简体、展示默认繁體（`--script s` 才简体）。不要把对话语言改成繁体。

## STRUCTURE

```
cbeta-cli/
├── Cargo.toml              # workspace; clap pinned =4.5.23 (rustc 1.75)
├── crates/cbeta-core/      # Command / Hit / Filters / parse_query  ← only real logic
├── crates/cbeta-parse/     # TEI P5 stub
├── crates/cbeta-index/     # Tantivy stub; default dir ~/.cbeta/
├── crates/cbeta-search/    # keyword/near/verify stub
├── crates/cbeta-cli/       # bin name `cbeta`; clap → Command
└── docs/                   # search-modes.md, human-ux.md, roadmap.md
```

No `src/` at root. No `tests/` dirs. No CI. Data/scripts stay in cbeta-corpus.

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Shared protocol | `crates/cbeta-core/src/lib.rs` | `Action` `Command` `Hit` `Filters` `ParsedQuery` `parse_query` |
| CLI surface | `crates/cbeta-cli/src/main.rs` | no-subcommand → `Action::Search` |
| Query DSL contract | `docs/search-modes.md` | agents: structured `clauses`, not CBReader string |
| TTY/JSON/exit | `docs/human-ux.md` | MCP field ≡ CLI flag |
| P0–P3 work | `docs/roadmap.md` | L-parse / L-index / L-search not started |
| Corpus lock | sibling `cbeta-corpus` | `scopes/taisho.yaml`, Category B exclude |

## CODE MAP

Centrality unmeasured (rust-analyzer not installed; codegraph timed out). Graph is small: only `cbeta-cli` → `cbeta-core`. Parse/index/search not wired.

| Symbol | Type | Location | Refs | Role |
|--------|------|----------|------|------|
| `main` | fn | `crates/cbeta-cli/src/main.rs` | entry | clap → `Command`; json/explain print; else exit 2 |
| `parse_query` | fn | `crates/cbeta-core/src/lib.rs` | cli | `+`→near/30, `*`→before/30, `NEAR/N`, `?` wildcard |
| `Command` | struct | `cbeta-core` | cli+future MCP | one payload for CLI and `cbeta serve` |
| `Hit` | struct | `cbeta-core` | unused yet | `line_id` `work_id` `citation` `score` |
| `Action` | enum | `cbeta-core` | cli | Search/Verify/Get/Read/Cite/Catalog/Info/Build/Serve |
| `Filters` | struct | `cbeta-core` | cli | canons/works/authors/categories/types/juans |
| `ParseError` | struct | `cbeta-core` | parse_query | fullwidth ops → exit 2 |
| `parse_placeholder` | fn | `cbeta-parse` | none | P0 stub |
| `index_placeholder` | fn | `cbeta-index` | none | P0 stub |
| `search_placeholder` | fn | `cbeta-search` | none | P0 stub |

## CONVENTIONS

- Workspace deps only in root `[workspace.dependencies]`; members use `.workspace = true`.
- Keep `clap = "=4.5.23"`. Unpin only after rustc >1.75 is the floor.
- Library errors: `thiserror`. CLI exits: rg semantics **0 hit / 1 no-hit / 2 usage or index**.
- Distance = **normalized 汉字**, never tokens. CBReader default window 30 chars.
- 产品：查询输入简体；经文展示默认繁體（`--script s` 才简体）。对话语言见 AGENT LANGUAGE（优先简体中文）。
- TTY default human; pipe → JSONL; `--json` never default. `NO_COLOR=1` / non-TTY drop color.
- Artifact id `{cbeta_tag}+{scope_hash}` e.g. `2026R2+a3f91c2e`. Scope change ⇒ rebuild.
- Tests: inline `#[cfg(test)]` in the crate under test. `cargo test -p cbeta-core -- plus_is_near_30 --exact`.
- Golden later: same call must assert TTY text **and** `--json` schema (`docs/human-ux.md`).

## ANTI-PATTERNS (THIS PROJECT)

- Do not vendor CBETA XML here. Corpus repo only.
- Do not index Category B by default: **Y / TX / LC / YP** (not CC).
- Do not add a 7th search tool or `search_*` explosion. Six: search / verify / get / catalog / info / serve.
- Do not ship regex-over-corpus in v1.
- Do not use semantic search for `verify` / `is_original`. Separate optional P3 tool.
- Do not treat CBReader `+ * & , - ?` as the agent API. Agents pass structured `clauses`.
- Do not accept fullwidth `＋＊＆，—？`. Reject; ask for halfwidth or `NEAR/N`.
- Do not add MCP-only knobs. Every MCP field has a CLI flag and vice versa.
- Do not hide pager, highlight, `--copy`, 本经内搜, `build` progress, completion behind MCP.
- Do not implement near as Tantivy `PhraseQuery` slop alone. Recall 2-gram AND, confirm on stored `text_norm` spans.

## UNIQUE STYLES

- Bare `cbeta 色即是空` is search. Subcommand optional.
- `line_id` shape: `T31n1585_p0001a12` (canon+vol `n` work `_p` page col line).
- Citation: `(CBETA 2026.R2, T30, no. 1578, p. 268, a12)`.
- Human hit line always: rank, line_id, title, 作译者, juan, highlighted snippet.
- `window`: `paragraph` (agent default) | `gatha` | `juan` (CBReader-coarse).
- Near engine: Boolean AND of char 2-grams → confirm char-span on `text_norm`.

## COMMANDS

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo test -p cbeta-core -- plus_is_near_30 --exact
cargo clippy --workspace --all-targets
cargo fmt --all
cargo run -p cbeta-cli -- --help
cargo run -p cbeta-cli -- search --explain --json '空性+缘生'
cargo install --path crates/cbeta-cli
# product (not implemented): cbeta build --scope taisho
```

No CI yet. rustc may be missing on some agent hosts.

## NOTES

- `cbeta-cli` currently depends only on `cbeta-core`. Wire `cbeta-search` / `cbeta-index` when L-search/L-index land.
- `Action::Read` / `Cite` exist on the type; clap surface today: Search, Verify, Get, Catalog, Info, Build, Serve.
- `--json` today dumps the **Command**, not hits (index missing).
- P0 fixtures named in roadmap: T0235 / T1578 / T1585 must emit `line_id`.
- License split: code MIT; 经文 CC BY-NC-SA 4.0, non-profit only.
