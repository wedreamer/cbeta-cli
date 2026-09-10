# Search modes for CLI and MCP agents

Distance unit is always **Chinese characters after normalization**, not tokens.
CBReader default proximity is 30 characters; agents should pass `distance` explicitly.

## Tools (keep six; do not explode into search_*)

| CLI | MCP | Purpose |
| --- | --- | --- |
| `cbeta search` | `cbeta_search` | lexical / proximity / boolean / wildcard (`fuzzy` planned) |
| `cbeta verify` | `cbeta_verify_quote` | original-text check + similar sentences |
| `cbeta get` | `cbeta_get_passage` | fetch context by line_id |
| `cbeta catalog` | `cbeta_list_catalog` | canons / works / authors / categories |
| `cbeta info` | `cbeta_index_info` | tag + scope_hash |
| `cbeta serve` | process | stdio or HTTP MCP |

## `cbeta_search` modes

| mode | When an agent should pick it |
| --- | --- |
| `keyword` | bag of terms, BM25, default |
| `phrase` | exact adjacent sequence, 偈颂 / 术语 |
| `near` | A within N chars of B, order ignored |
| `before` | A then B within N chars, order required |
| `boolean` | `&` AND / `,` OR / `-` or English `NOT` exclude (no lexical `AND`/`OR`) |
| `fuzzy` | **planned** — 1–2 char typo / 异体 after n-gram recall (no engine path today) |
| `wildcard` | unknown single char `?`, max 2 per term (shipped; 2026R2 `莲?色` hits 青蓮色/紅蓮色) |
| `dsl` | power-user string: CBReader `+ * & , - ?` or `NEAR/16` |

Do not ship regex-over-corpus in v1. Semantic search is a later extra tool, never mixed into verify.

## Structured near (MCP `clauses` landed; richer shape still planned)

MCP `cbeta_search` accepts `clauses: Vec<String>` (plus `mode`) and maps them to `ParsedQuery` (default near/30; `before` → ordered/30; `keyword`/`phrase` pass through). Today clauses are plain strings: the per-clause object shape below (`within_chars`, `ordered`) is **planned**. There is still **no `Command.clauses` type and no `--clauses` CLI flag**, and no `--window` / `--limit` on the clap surface; the CLI keeps the string `q` through `parse_query`.

```json
{
  "mode": "near",
  "clauses": [
    {"text": "空性"},
    {"text": "缘生", "within_chars": 16, "ordered": false}
  ],
  "filters": {"canons": ["T"], "types": ["lun"]},
  "window": "paragraph",
  "limit": 20
}
```

`filters.types` matches the current `Filters.types` field name. `window`: `paragraph` (default, agent-friendly) | `gatha` | `juan` (CBReader-compatible, coarser). `window` / `limit` do not exist on `Command` or any transport yet; per-clause `within_chars` / `ordered` are planned (MCP `clauses` is `Vec<String>` today).

## String DSL (humans + CBReader aliases)

Preferred agent string (self-describing):

```text
空性 NEAR/16 缘生
真如 BEFORE/8 依他起 NOT 外道
莲?色&阿罗汉
```

Accepted CBReader aliases (parser only; do not document these as the agent API):

```text
空性+缘生          # NEAR default 30
空性*缘生          # BEFORE default 30
佛陀&阿难          # AND same window
莲?色,莲花色       # OR + wildcard
佛陀-佛陀曰        # EXCLUDE
```

Fullwidth `—` `＋` `＊` `＆` `？` are rejected with a clear error asking for halfwidth (`，` is not rejected; only the five operators above are).

## Engine notes

Tantivy `PhraseQuery` slop is a budget over unigrams and cannot encode
“multi-char phrase A near multi-char phrase B” without tearing A/B apart.
Implementation:

1. Recall with Boolean AND of each clause’s char 2-grams
2. Confirm on stored `text_norm` using character-span distances
3. Optional PhraseQuery on first/last chars of each clause as a candidate cut

## Golden proximity cases

- `真如 NEAR/16 缘起` inside 瑜伽/唯识部类 (2-gram recall + char-span confirm, shipped)
- `莲?色` → 青蓮色 / 紅蓮色 on 2026R2 (single-char `?` wildcard, shipped)
- user verse via `verify`, not `near`
