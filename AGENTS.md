# PROJECT KNOWLEDGE BASE

## OVERVIEW

Offline CBETA search CLI (`cbeta`). Humans type the CLI; MCP/HTTP are the same `Command` over another transport. Rust 2021 Cargo workspace. **Scaffold state (2026-09):** parse/index/search are stubs. Search without `--json`/`--explain` exits 2; `--json` today dumps the **Command**, not hits. The Command→Hits JSON flip is atomic at L-search, not before.

Corpus is **not** this repo: sibling [cbeta-corpus](https://github.com/wedreamer/cbeta-corpus) pins `xml-p5@2026R2`. Do not vendor CBETA XML here. README/`docs/` examples are the **product contract**, not current runtime; humans own README recipes, do not rewrite UX examples as if implemented.

## AGENT LANGUAGE

- 对用户优先说**简体中文**。代码、标识符、命令、路径、crate 名保持原文。
- 与产品约定分开：经文查询输入简体、展示默认繁體（`--script s` 才简体）。不要把对话语言改成繁体。

## STRUCTURE

```
cbeta-cli/
├── Cargo.toml              # workspace; clap pinned =4.5.23
├── crates/cbeta-core/      # Command / Hit / Filters / parse_query  ← only real logic
├── crates/cbeta-parse/     # TEI P5 stub
├── crates/cbeta-index/     # Tantivy stub; default dir ~/.cbeta/
├── crates/cbeta-search/    # keyword/near/verify stub
├── crates/cbeta-cli/       # bin name `cbeta`; clap → Command; tests/ = CLI contracts
└── docs/                   # search-modes.md, human-ux.md, roadmap.md
```

Crate graph: only `cbeta-cli` → `cbeta-core` is wired. Parse/index/search are not depended on yet; wire them when L-search/L-index land. Data/scripts stay in cbeta-corpus. Never commit `.omo/` or `.codegraph/`.

## TOOLCHAIN / DEPS

- MSRV **unknown**. Host observed rustc 1.98.1. Do not add `rust-toolchain.toml`; do not assert a rustc floor.
- `clap` stays `=4.5.23` until someone proves why to move it.
- Workspace deps only in root `[workspace.dependencies]`; members use `.workspace = true`.
- `thiserror` is a declared dep for library errors, but `ParseError` in cbeta-core is hand-rolled today. Do not migrate it in this PR.

## GIT SIGNING

Configure a local Git signing key (GPG or SSH) and bind it to the GitHub account as a **Signing key** (Settings → SSH and GPG keys). An authentication key does not count until it is also added as a signing key. Never `--no-gpg-sign`.

## TESTS (dual-tier + inline)

- Inline `#[cfg(test)]` in the crate under test, e.g. `cargo test -p cbeta-core -- plus_is_near_30 --exact`.
- `crates/cbeta-cli/tests/cli_scaffold_contract.rs`: **runs in CI**; asserts today's scaffold behavior (Command dump, exit 2).
- `crates/cbeta-cli/tests/cli_product_hits.rs`: `#[ignore = "L-search"]`; the future product contract, expected to fail until L-search.
- CI exists: `.github/workflows/ci.yml`.

## KNOWN SCAFFOLD BUG (do not freeze)

`main.rs` runs `parse_query` as a **global pre-step** on any `q`, so `verify <text>`, `get <line_id>` and `build --scope` all pass through search-query parsing. This is a scaffold bug. Do not treat it as product protocol; do not add tests that lock it in.

## CLI SURFACE (today)

- clap subcommands: Search, Verify, Get, Catalog, Info, Build, Serve. `Action::Read` / `Action::Cite` exist on the type only, not in clap.
- Bare `cbeta 色即是空` is search (no-subcommand → `Action::Search`).
- Exits follow rg semantics: **0 hit / 1 no-hit / 2 usage or index**.
- TTY default human; pipe → JSONL; `--json` never default. `NO_COLOR=1` / non-TTY drop color.

## QUERY DSL (contract in docs/search-modes.md)

- Distance = **normalized 汉字**, never tokens. CBReader default window 30 chars; `+` → near/30, `*` → before/30, `NEAR/N`, `?` single-char wildcard.
- Fullwidth rejection set (verified in `cbeta-core/src/lib.rs`): `— ＋ ＊ ＆ ？`. Fullwidth comma `，` is **not** rejected.
- Do not treat CBReader `+ * & , - ?` as the agent API. Agents pass structured `clauses` (not yet a type; do not invent one in this PR).
- Do not implement near as Tantivy `PhraseQuery` slop alone. Recall via 2-gram Boolean AND, confirm char-span on stored `text_norm`.

## ANTI-PATTERNS

- Do not index Category B by default: **Y / TX / LC / YP** (not CC).
- Six tools only: search / verify / get / catalog / info / serve. No 7th search tool, no `search_*` explosion.
- Do not ship regex-over-corpus in v1.
- Do not use semantic search for `verify` / `is_original`. Separate optional P3 tool.
- Do not add MCP-only knobs. Every MCP field has a CLI flag and vice versa.
- Do not hide pager, highlight, `--copy`, 本经内搜, `build` progress, completion behind MCP.

## PRODUCT SHAPE (target, mostly unimplemented)

- `line_id`: `T31n1585_p0001a12` (canon+vol `n` work `_p` page col line). Citation: `(CBETA 2026.R2, T30, no. 1578, p. 268, a12)`.
- Human hit line: rank, line_id, title, 作译者, juan, highlighted snippet.
- Artifact id `{cbeta_tag}+{scope_hash}` e.g. `2026R2+a3f91c2e`. Scope change ⇒ rebuild.
- P0 fixtures in roadmap: T0235 / T1578 / T1585 must emit `line_id`.
- License split: code MIT; 经文 CC BY-NC-SA 4.0, non-profit only.

## COMMANDS

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo test -p cbeta-core -- plus_is_near_30 --exact
cargo clippy --workspace --all-targets
cargo fmt --all
cargo run -p cbeta-cli -- search --explain --json '空性+缘生'
# product (not implemented): cbeta build --scope taisho
```
