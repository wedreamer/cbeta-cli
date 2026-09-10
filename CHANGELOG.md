# Changelog

All notable changes to `cbeta-cli` are documented here, following the
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format and
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

v0.1.0 起：离线 CBETA 检索 CLI 的首个人用闭环（P0/P1/P2）。

## [Unreleased]

## [0.1.0] - 2026-09-10

First release of the offline CBETA search CLI. The binary is `cbeta`, built
from the `cbeta-cli` crate; corpus data is pinned upstream in
[cbeta-corpus](https://github.com/wedreamer/cbeta-corpus) at
`xml-p5@2026R2`. Indexes live in `~/.cbeta/index` by default (corpus cache
`~/.cbeta/corpus/2026R2`) and can be built per scope with
`cbeta build --scope <name>`.

### Added

P0, a human can search:

- `cbeta build`: indexes TEI P5 into Tantivy with a scope hash, so a scope
  change forces a rebuild (artifact id `{cbeta_tag}+{scope_hash}`).
- Keyword and phrase search, as `cbeta search <q>` or bare `cbeta <q>`, with
  rank, `line_id`, title, author, and highlighted snippets on a TTY and full
  hit fields (`line_id`, `work_id`, `title`, `text_raw`, `citation`, `score`,
  `cbeta_tag`) under `--json`.
- `cbeta get <line_id>` to open a passage with its CBETA citation.
- `cbeta catalog` with `--author` / `--type` / `--canon` / `--title` / `--work`
  filters, and `cbeta info` for the current index.
- ripgrep-style exit codes: 0 hit, 1 no hit, 2 usage or index error.

P1, proximity, verification, and MCP:

- Query DSL parsing: ordered phrases, `&` (AND) / `,` (OR) / `-` or English
  `NOT` (exclude), single-char `?` wildcard (max 2 per term; engine hits such
  as `莲?色` → 青蓮色/紅蓮色), `NEAR/N`, and the CBReader aliases `+`
  (near/30) and `*` (before/30). `search --explain` shows the parsed query.
  English words `AND` / `OR` are not operators.
- NEAR/BEFORE recall via 2-gram Boolean AND with char-span confirmation on
  stored `text_norm`; distance is measured in normalized Han characters, not
  tokens.
- `cbeta verify <quote>`: exact check by normalized-text hash, then similar
  candidates by n-gram and string alignment, reported as `is_original`,
  `exact_hit`, and `similar` (exit 0 either way, 2 only with no index).
- `get -C <N>` / `--context` for same-work neighbors, `read <work> --juan <N>`
  to list a juan's lines, and `cite <line_id>` to print one citation.
- `--copy` prints a ready-to-paste notes block to stdout (no OS clipboard
  dependency).
- `--work` in-work search and `--script s|t` display-only traditional/simplified
  switching; index keys and `line_id`s are never mutated.
- `cbeta serve`: stdio MCP server exposing exactly five tools,
  `cbeta_search`, `cbeta_verify_quote`, `cbeta_get_passage`,
  `cbeta_list_catalog`, and `cbeta_index_info`, mirroring the CLI flags.

P2, speed and the human loop:

- `cbeta serve --http`: HTTP transport with a REST `/search` endpoint and the
  same five MCP tools on `/mcp`.
- Interactive REPL when `cbeta` is invoked with no arguments.
- `--save last` on search and `--from last` (with `--index`) on get, backed by
  a `last.json` session file.
- `cbeta completion` for shell completion scripts.
- `cbeta bench`: query-latency benchmarking with JSON output.
- Atomic index swap: builds publish through a `CURRENT` pointer, so a failed
  build never corrupts a live index.
- GitHub Releases for four host-native binaries (`cbeta-<triple>.tar.gz` /
  `.zip` plus `sha256sums.txt`), triggered by `v*` tags.

Licensing and scope notes for this release:

- Code in this repository is MIT; CBETA scripture text is
  [CC BY-NC-SA 4.0](https://creativecommons.org/licenses/by-nc-sa/4.0/),
  non-profit use only.
- Category B collections (Y / TX / LC / YP) are not CC-licensed and are
  excluded from default indexing.

### Changed

- `get --json -C <N>` returns the context window split into `{ hit, before,
  after }`; without `-C` it returns `{ hit }` only.

### Fixed

- `catalog --json` accepts works whose metadata fields are JSON `null`.
- Documentation examples no longer reference the ghost `line_id`
  `T30n1578_p0268a12`; the verified example line is `...p0268b21`.

### Not in 0.1.0

Planned surfaces intentionally left out of this release: `--window` result
windowing, the built-in pager, JSONL-by-default piping, `--category`, the
`fuzzy` engine, `semantic_search` (P3), and publishing to crates.io. Install
from GitHub Releases or `cargo install --locked --git` / `--path` (see
README). `catalog --author` / `--title` need populated catalog.jsonl fields;
the 2026R2 sidecar currently exports those as JSON null.

[Unreleased]: https://github.com/wedreamer/cbeta-cli/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/wedreamer/cbeta-cli/releases/tag/v0.1.0
