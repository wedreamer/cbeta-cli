# Search modes

Distance unit is always **Chinese characters after normalization**, not words.
Default window is a paragraph / gāthā, not a juan (juan is too coarse for agents).
CBReader default distance is 30; agents should pass `distance` explicitly.

## Modes

| mode | Meaning | Engine |
|---|---|---|
| `keyword` | bag of terms, BM25 | `text_ngram` |
| `phrase` | adjacent exact string | `text_phrase` slop 0 |
| `near` | all clauses within N chars, order free | bigram AND + span check on `text_norm` |
| `before` | A then B within N chars | same, require start(A) < start(B) |
| `boolean` | must / should / must_not of the above | Tantivy BooleanQuery + span post-filter |
| `fuzzy` | 1–2 char variants | n-gram recall + RapidFuzz |
| `wildcard` | `?` = one char, ≤2 wildcards, clause ≤8 | expand then phrase |
| `dsl` | CBReader string `A+B` / `A*B` | parsed into structured clauses |

Skip in v1: full-corpus regex. Semantic is a separate tool later.

## Why PhraseQuery slop is not enough

Tantivy slop is a budget across every unigram. `PhraseQuery(["空","性","缘","生"], slop=16)` would also allow gaps *inside* 空性. Implementation:

1. Recall with AND of each clause’s char bigrams
2. Confirm on stored `text_norm` spans: `min_edge_distance ≤ N`
3. Optional first-char PhraseQuery only as a candidate cut

## Agent call

```json
{
  "mode": "near",
  "clauses": ["空性", "缘生"],
  "distance": 16,
  "ordered": false,
  "filters": { "canons": ["T"], "work_types": ["lun"] },
  "limit": 20
}
```

CLI:

```text
cbeta search --mode near --clause 空性 --clause 缘生 --distance 16 --canon T
cbeta search --dsl '空性+缘生' --distance 16
cbeta serve --mcp stdio
```

CBReader aliases (human / `dsl` only, not the primary agent syntax):
`+` near, `*` before, `&` and, `,` or, `-` not, `?` one char, `()` group.
