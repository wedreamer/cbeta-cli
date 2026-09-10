# Human-first CLI

`cbeta` is a tool people type all day. MCP is the same commands over a socket.
If a scholar cannot finish a task in one or two shell lines, the API is wrong.

## Output contract

| Audience | Flag | Shape |
|---|---|---|
| Human TTY | default | color, columns, highlight (pager planned) |
| Script | `--plain` | no color, stable columns |
| Agent / MCP / jq | `--json` | same schema as MCP tool result |

`NO_COLOR=1` and non-TTY automatically drop color. `--json` is never the default; piped JSONL output stays planned (pass `--json` explicitly today).

Human hit line:

```text
  1  T30n1578_p0268b21  大乘掌珍論  玄奘譯  卷1
     真性有為空 如幻緣生故
       ▲▲▲▲
```

Always show: rank, line_id, title, author/translator, juan, highlighted snippet.
`--copy` prints a citation block ready for notes:
`T30n1578_p0268b21 大乘掌珍論卷1：真性有為空，如幻緣生故…`

## Everyday recipes (API must cover each)

```bash
cbeta search '空性+缘起' --canon T --type lun
cbeta search --work T1585 '真如 NEAR/16 缘起'
cbeta search '莲?色' --type jing   # --category 仍为 planned,见下
cbeta verify '真性有为空，缘生故如幻，无为无起灭，不实若空华。'
cbeta get T30n1578_p0268b21 -C4
cbeta read T0235 --juan 1
cbeta catalog --author 玄奘 --type lun
cbeta info
```

`--category 阿含部类` filters remain **planned**: today's clap surface has `--type` (lun/jing/…) but no `--category` flag. Single-char `?` wildcard is shipped (2026R2: `莲?色` hits 青蓮色/紅蓮色). `catalog --author` / `--title` filter catalog.jsonl fields; the 2026R2 sidecar currently has those as JSON null (corpus export, not a CLI fill-from-TEI gap).

## REPL (landed in v0.1.0)

Bare `cbeta` (no argv) opens the REPL. Query lines run search; colon commands manage the session:

```text
cbeta
> 空性+缘起
> :scope T1585      # 本经内搜, same effect as --work on one-shot CLI
> :open 3           # print hit 3 with context
> :copy 3           # citation block for hit 3 to stdout
> :verify 真性有为空…
> :q                # or :quit / EOF
```

No `:help` yet. Colon commands only: a query line does not accept clap flags (inline flags like `--canon T` on a query are not supported); set filters with `:scope` or exit and use the one-shot CLI.

`:scope` is 本经内搜. Same as `--work` on the one-shot CLI.

## Reverse-validation rules

1. Every MCP field has a CLI flag. No MCP-only knobs.
2. Every weekly human flag exists on the MCP tool.
3. Golden tests assert CLI TTY text *and* `--json` schema from the same call.
4. CBReader halfwidth `+ * & , - ?` must type in zsh without quoting hell where possible; document when quotes are required.
5. Input accepts 简体; display default 繁體 (`--script s` to show simplified). `--script s|t` has LANDED as a global clap flag, display-only: it never mutates `line_id` or index keys.
6. Default window for humans can stay paragraph; `--window juan` exists for CBReader muscle memory. `--window` remains planned UX, not on the clap surface yet.

## What we will not hide behind MCP

pager, highlight, citation copy, 本经内搜, progress bars on `build`, shell completion.
These stay CLI features. JSON only carries the data those features need (`line_id`, spans, title).
