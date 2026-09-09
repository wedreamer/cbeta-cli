# PROJECT KNOWLEDGE BASE

## OVERVIEW

Offline CBETA search CLI (`cbeta`). Humans type the CLI; MCP/HTTP are the same `Command` over another transport. Rust 2021 Cargo workspace. **P0 landed (2026-09):** `build`, keyword/phrase search (TTY + `--json` Hits), `get` by `line_id`, `catalog` / `info`. **P1 L-get:** `get -C` / `--context`, `read --juan`, `cite`, `--copy` (stdout notes block, no OS clipboard). `get --json -C N` → `{ hit, before, after }`; without `-C` → `{ hit }` only. `Command.context` / `Command.copy` are transport fields (Search ignores `-C`). **P1 L-verify landed:** `verify` → `VerifyReport` (`is_original` / `exact_hit` / `similar`); hash(norm) exact + n-gram/alignment similar; exit 0 even when not original. `--explain --json` still dumps **Command** (parse dump, not hits). `serve` is stdio MCP (five tools); see `cli_mcp`.

Corpus is **not** this repo: sibling [cbeta-corpus](https://github.com/wedreamer/cbeta-corpus) pins `xml-p5@2026R2`. Do not vendor CBETA XML here. README/`docs/` examples are the **product contract**, not current runtime; humans own README recipes, do not rewrite UX examples as if implemented.

## AGENT LANGUAGE

- 对用户优先说**简体中文**。代码、标识符、命令、路径、crate 名保持原文。
- 与产品约定分开：经文查询输入简体、展示默认繁體，这是 intent。`--script s|t` 已落地（display-only）；`--window` 仍为 planned UX（`docs/human-ux.md` 规则 5–6）。不要把对话语言改成繁体。

## STRUCTURE

```
cbeta-cli/
├── Cargo.toml              # workspace; clap pinned =4.5.23
├── crates/cbeta-core/      # Command / Hit / VerifyReport / Filters / parse_query
├── crates/cbeta-parse/     # TEI P5 + 繁简/异体/缺字/去标点
├── crates/cbeta-index/     # Tantivy; default dir ~/.cbeta/
├── crates/cbeta-search/    # keyword/phrase + verify (near Boolean AND in P0; no span confirm)
├── crates/cbeta-cli/       # bin name `cbeta`; clap → Command; tests/ = CLI contracts
└── docs/                   # search-modes.md, human-ux.md, roadmap.md
```

Crate graph: `cbeta-cli` → `cbeta-core` + `cbeta-parse` + `cbeta-index` + `cbeta-search`. Data/scripts stay in cbeta-corpus. Never commit `.omo/` or `.codegraph/`.

## TOOLCHAIN / DEPS

- MSRV **unknown**. Host observed rustc 1.98.1. Do not add `rust-toolchain.toml`; do not assert a rustc floor.
- `clap` stays `=4.5.23` until someone proves why to move it.
- Workspace deps only in root `[workspace.dependencies]`; members use `.workspace = true`.
- `thiserror` is a declared dep for library errors, but `ParseError` in cbeta-core is hand-rolled today. Do not migrate it in this PR.

## STYLE / COMMENTS

- Comments say WHY, not WHAT; the code already says what.
- rustdoc on every `pub` item.
- Production logic stays ≤250 LOC per module; split rather than bloat.
- Library crates: no `unwrap` / `expect`—return errors. The binary may `expect` only with a stated reason.

## GIT SIGNING

Configure a local Git signing key (GPG or SSH) and bind it to the GitHub account as a **Signing key** (Settings → SSH and GPG keys). An authentication key does not count until it is also added as a signing key. Never `--no-gpg-sign`.

## TESTS (dual-tier + inline + local usability)

`cargo test` is **necessary but not sufficient**. A slice is not closed until a scholar-shaped run of the real `cbeta` binary succeeds against a real (mini) index. Agents must not claim P0/P1 done from unit tests alone.

### Automated (CI)

- Inline `#[cfg(test)]` in the crate under test, e.g. `cargo test -p cbeta-core -- plus_is_near_30 --exact`.
- `crates/cbeta-cli/tests/cli_usability.rs`: **runs in CI**; scholar recipes (build, TTY keyword, `--json` hit fields, no-hit 1, catalog `--author`/`--type`/`--canon`, info, `--mode phrase`, get known/ghost `line_id`, verify exact/variant JSON). This **locks** the recipes; it does **not** replace the local protocol below.
- `crates/cbeta-cli/tests/cli_product_hits.rs`: product JSON hits (not ignored).
- `crates/cbeta-cli/tests/cli_scaffold_contract.rs`: mixed. search / get / catalog / info / build / verify / serve are product; MCP handshake in `cli_mcp`.
- Workspace **line** coverage ≥95 via `cargo llvm-cov` (command in COMMANDS).
- CI exists: `.github/workflows/ci.yml`.

### Local usability protocol (mandatory before claiming done)

Run the real binary as a 学者 would. Record **command, env, stdout, stderr, exit** for each step. Same recipe twice when the surface has both: TTY (no `--json`) and `--json`.

1. Isolate CI mini: `CBETA_CORPUS` = in-repo `crates/cbeta-cli/tests/fixtures/mini`, `CBETA_INDEX` = a temp dir, `NO_COLOR=1`. Mini is **necessary** for CI, **not sufficient** to close P0.
2. Build: `cbeta build --scope ci-minimal` → exit 0.
3. P0 scholar recipes against that index:
   - bare `cbeta 真性有为空` → TTY has rank, `T30n1578_p0268b21`, title 大乘掌珍論
   - `cbeta search --json 真性有为空` → `hits[]` with `line_id` / `work_id` / `title` / `text_raw` / `citation` / `score` / `cbeta_tag`
   - no-hit (`xyzzy-not-in-corpus`) → exit 1
   - `cbeta catalog --author 玄奘` lists T1578
   - `cbeta catalog --type lun --canon T`
   - `cbeta info`
   - `cbeta search --mode phrase '如幻緣生故'` hits `T30n1578_p0268b21`
   - `cbeta get T30n1578_p0268b21` exit 0; ghost `T30n1578_p0268a12` exit 1
   - `cbeta get T30n1578_p0268b21 -C 4` TTY neighbors + citation; `--json` → `{ hit, before, after }`
   - `cbeta read T0235 --juan 1`; `cbeta cite T30n1578_p0268b21`; `cbeta get … --copy` stdout block
   - `cbeta verify '真性有為空，如幻緣生故'` → is_original=true, line_id b21
   - `cbeta verify --json '真性有为空，缘生故如幻…'` → is_original=false, similar[0].work_id=T1578; never Command dump
4. Mini corpus is **T0235 + T1578 only**. Do **not** assert `catalog --title 成唯識` or T1585 `line_id`s against mini. Mini T1578 has few lines — `-C 4` may return fewer than 4 neighbors.
5. Closing P0/P1 **also** requires a transcript against `CBETA_CORPUS=$HOME/.cbeta/corpus/2026R2` (real xml-p5, not mini). Record command/env/stdout/stderr/exit. Do not claim “usable” from `cargo test` alone.

### Not product yet (do not lock)

- `--explain --json` still dumps Command.
- NEAR char-span confirm, `--window` remain P1+.

## PARSE SCOPE (do not freeze the old bug)

`parse_query` runs **only** on `Action::Search`. `verify` / `get` / `build` keep raw `q`. Do not add tests that send those through search-query parsing.

## CLI SURFACE (today)

- clap subcommands: Search, Verify, Get, Read, Cite, Catalog, Info, Build, Serve.
- Bare `cbeta 色即是空` is search (no-subcommand → `Action::Search`).
- `get -C` / `--context <N>`: same-work neighbors by sorted `line_id`. Search ignores `-C`.
- `read <work> --juan <N>` lists lines; `cite <line_id>` prints citation; `--copy` prints notes block to stdout (no xclip/arboard).
- `--work` / `--author` / `--type` / `--canon` / `--title` filter search and catalog.
- `--script s|t`: display-only 简/繁; never mutates `line_id` / index keys.
- `--mode keyword|phrase` overrides `parsed_query.mode` on Search (unknown value → exit 2).
- `serve`: stdio MCP only (five tools); logs on stderr; no HTTP.
- Exits follow rg semantics: **0 hit / 1 no-hit / 2 usage or index**. `verify` exits **0** for both original and not-original (check command); **2** no index. `serve` runs until client disconnect.
- TTY default human; `--json` never default. `NO_COLOR=1` / non-TTY drop color. Pipe JSONL is planned, not the search `--json` pretty document.

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

- `line_id`: `T31n1585_p0001a12` (canon+vol `n` work `_p` page col line). Citation: `(CBETA 2026.R2, T30, no. 1578, p. 268, b21)`.
- Human hit line: rank, line_id, title, 作译者, juan, highlighted snippet.
- Artifact id `{cbeta_tag}+{scope_hash}` e.g. `2026R2+a3f91c2e`. Scope change ⇒ rebuild.
- P0 fixtures in roadmap: T0235 / T1578 / T1585 must emit `line_id`.
- License split: code MIT; 经文 CC BY-NC-SA 4.0, non-profit only.

## COMMANDS

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo test -p cbeta-core -- plus_is_near_30 --exact
cargo test -p cbeta-cli --test cli_usability
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo llvm-cov --workspace --all-targets --locked --fail-under-lines 95 --show-missing-lines
# isolate + scholar (see TESTS local usability protocol)
CBETA_CORPUS=crates/cbeta-cli/tests/fixtures/mini CBETA_INDEX=/tmp/cbeta-use NO_COLOR=1 \
  cargo run -p cbeta-cli -- build --scope ci-minimal
CBETA_CORPUS=crates/cbeta-cli/tests/fixtures/mini CBETA_INDEX=/tmp/cbeta-use NO_COLOR=1 \
  cargo run -p cbeta-cli -- 真性有为空
CBETA_CORPUS=crates/cbeta-cli/tests/fixtures/mini CBETA_INDEX=/tmp/cbeta-use NO_COLOR=1 \
  cargo run -p cbeta-cli -- verify --json '真性有为空，缘生故如幻，无为无起灭，不实若空华。'
# --explain --json still dumps Command (not hits)
cargo run -p cbeta-cli -- search --explain --json '空性+缘生'
```
