# Search modes for CLI and MCP agents

Distance unit is always **Chinese characters after normalization**, not tokens.
CBReader default proximity is 30 characters; agents should pass `distance` explicitly.

## Tools (keep six; do not explode into search_*)

| CLI | MCP | Purpose |
| --- | --- | --- |
| `cbeta search` | `cbeta_search` | lexical / proximity / boolean / fuzzy / wildcard |
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
| `boolean` | AND / OR / NOT of terms, phrases, or near-clauses |
| `fuzzy` | 1–2 char typo / 异体 after n-gram recall |
| `wildcard` | unknown single char `?`, max 2 wildcards |
| `dsl` | power-user string: CBReader `+ * & , - ?` or `NEAR/16` |

Do not ship regex-over-corpus in v1. Semantic search is a later extra tool, never mixed into verify.

## Structured near (preferred for agents)

```json
{
  "mode": "near",
  "clauses": [
    {"text": "空性"},
    {"text": "缘生", "within_chars": 16, "ordered": false}
  ],
  "filters": {"canons": ["T"], "work_types": ["lun"]},
  "window": "paragraph",
  "limit": 20
}
```

`window`: `paragraph` (default, agent-friendly) | `gatha` | `juan` (CBReader-compatible, coarser).

## String DSL (humans + CBReader aliases)

Preferred agent string (self-describing):

```text
空性 NEAR/16 缘生
真如 BEFORE/8 依他起 NOT 外道
莲?色 AND 阿罗汉
```

Accepted CBReader aliases (parser only; do not document these as the agent API):

```text
空性+缘生          # NEAR default 30
空性*缘生          # BEFORE default 30
佛陀&阿难          # AND same window
莲?色,莲花色       # OR + wildcard
佛陀-佛陀曰        # EXCLUDE
```

Fullwidth ＋＊＆，？ are rejected with a clear error asking for halfwidth.

## Engine notes

Tantivy `PhraseQuery` slop is a budget over unigrams and cannot encode
“multi-char phrase A near multi-char phrase B” without tearing A/B apart.
Implementation:

1. Recall with Boolean AND of each clause’s char 2-grams
2. Confirm on stored `text_norm` using character-span distances
3. Optional PhraseQuery on first/last chars of each clause as a candidate cut

## Golden proximity cases

- `真如 NEAR/16 缘起` inside 瑜伽/唯识部类
- `莲?色` → 莲華色 and 莲花色
- user verse via `verify`, not `near`
