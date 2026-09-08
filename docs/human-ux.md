# Human-first CLI

`cbeta` is a tool people type all day. MCP is the same commands over a socket.
If a scholar cannot finish a task in one or two shell lines, the API is wrong.

## Output contract

| Audience | Flag | Shape |
|---|---|---|
| Human TTY | default | color, columns, highlight, pager |
| Script | `--plain` | no color, stable columns |
| Agent / MCP / jq | `--json` | same schema as MCP tool result |

`NO_COLOR=1` and non-TTY automatically drop color. `--json` is never the default.

Human hit line:

```text
  1  T31n1585_p0001a12  成唯識論  玄奘譯  卷1
     真如遠離能所取故說名空
         ▲▲           ▲▲
```

Always show: rank, line_id, title, author/translator, juan, highlighted snippet.
`--copy` prints a citation block ready for notes:
`T31n1585_p0001a12 成唯識論卷1：…`

## Everyday recipes (API must cover each)

```bash
cbeta search '空性+缘起' --canon T --type lun
cbeta search --work T1585 '真如 NEAR/16 缘起'
cbeta search '莲?色' --category 阿含部类
cbeta verify '真性有为空，缘生故如幻，无为无起灭，不实若空华。'
cbeta get T31n1585_p0001a12 -C4
cbeta read T0235 --juan 1
cbeta catalog --author 玄奘 --type lun
cbeta info
```

REPL (optional v1.1, but design now):

```text
cbeta
> 空性+缘起 --canon T
> :open 3
> :verify 真性有为空…
> :scope T1585
> :copy 3
```

`:scope` is 本经内搜. Same as `--work` on the one-shot CLI.

## Reverse-validation rules

1. Every MCP field has a CLI flag. No MCP-only knobs.
2. Every weekly human flag exists on the MCP tool.
3. Golden tests assert CLI TTY text *and* `--json` schema from the same call.
4. CBReader halfwidth `+ * & , - ?` must type in zsh without quoting hell where possible; document when quotes are required.
5. Input accepts 简体; display default 繁體 (`--script s` to show simplified).
6. Default window for humans can stay paragraph; `--window juan` exists for CBReader muscle memory.

## What we will not hide behind MCP

pager, highlight, citation copy, 本经内搜, progress bars on `build`, shell completion.
These stay CLI features. JSON only carries the data those features need (`line_id`, spans, title).
