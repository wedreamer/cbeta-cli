# cbeta-cli

离线查 [CBETA](https://www.cbeta.org/) 经论。给人用的是命令行；MCP / HTTP 是同一套 `Command` 的运输。

```bash
cbeta 真性有为空
```

数据仓库：[wedreamer/cbeta-corpus](https://github.com/wedreamer/cbeta-corpus)
跟踪：[#1 v0.1 人用最小闭环](https://github.com/wedreamer/cbeta-cli/issues/1)

> 状态（**2026-09-08**）：仓库还是脚手架。`Cargo.toml` workspace 已立，**尚不能真正搜索**。下面命令是产品契约，不是当前能跑通的 CLI。

## 安装（目标）

```bash
# 1. 拉经文（不进本仓库）
git clone https://github.com/wedreamer/cbeta-corpus.git
cd cbeta-corpus && ./scripts/fetch.sh

# 2. 装 CLI
git clone https://github.com/wedreamer/cbeta-cli.git
cd cbeta-cli
cargo install --path crates/cbeta-cli

# 3. 建索引
cbeta build --scope taisho
```

索引默认在 `~/.cbeta/`。

## 人怎么用

```bash
# 随手搜（无子命令也默认 search）
cbeta 色即是空
cbeta search 无常 --canon T

# CBReader 肌肉记忆：+ 是 NEAR，? 是单字通配
cbeta search '空性+缘生' --type lun
cbeta search '莲?色'

# 本经内搜、字距邻近
cbeta search '真如 NEAR/16 缘起' --work T1585

# 这句是不是原文（从微信/笔记直接贴）
cbeta verify '真性有为空，缘生故如幻，无为无起灭，不实若空华。'

# 打开出处，像 rg -C
cbeta get T30n1578_p0268a12 -C 4
cbeta read T0235 --juan 1
cbeta cite T30n1578_p0268a12
# (CBETA 2026.R2, T30, no. 1578, p. 268, a12)

# 先找书再搜正文
cbeta catalog --author 玄奘 --type lun
cbeta info
```

TTY 默认人可读：高亮、`line_id`、经名、作译者。管道默认 JSONL；`--json` 与 MCP 同 schema。
退出码跟 rg：`0` 有命中，`1` 无命中，`2` 用法或索引错误。

## 搜索方式

距离单位是归一化后的 **汉字**，不是 token。CBReader 默认邻近距 30 字。

| 方式 | 例子 |
|---|---|
| keyword | `cbeta search 空性` |
| phrase | `cbeta search '真如远离'` |
| near | `cbeta search '空性 NEAR/16 缘生'` 或 `'空性+缘生'` |
| before | `cbeta search '空性 BEFORE/16 缘生'` 或 `'空性*缘生'` |
| boolean | `AND / OR / NOT`，CBReader `& , -` |
| wildcard | `'莲?色'` → 莲華色、莲花色 |
| fuzzy | 1–2 字误差 / 异体 |
| verify | 是否原文；不是则返近句 |

Agent 请用结构化 `clauses`（未来的 agent API：当前 `Command` 只有字符串 `q`，走 `parse_query`，`clauses` 字段尚未实现），不要拼 CBReader DSL。语义搜索是独立工具，不参与「是不是原文」。

详见 [docs/search-modes.md](docs/search-modes.md)、[docs/human-ux.md](docs/human-ux.md)。

## 两个仓库

| 仓库 | 职责 |
|---|---|
| [cbeta-corpus](https://github.com/wedreamer/cbeta-corpus) | 锁定 xml-p5@2026R2、筛选 scope、校验、导出 catalog |
| **cbeta-cli** | 解析、索引、人用 CLI、引文校验、`cbeta serve` |

产物 ID：`{cbeta_tag}+{scope_hash}`，例 `2026R2+a3f91c2e`。改 scope 必须重建索引。

```text
crates/cbeta-core     Command / Hit / Filters
crates/cbeta-parse    TEI P5
crates/cbeta-index    Tantivy
crates/cbeta-search   keyword / near / verify
crates/cbeta-cli      二进制名 cbeta
```

## 版权

- **本仓库代码**：MIT
- **CBETA 经文**：[CC BY-NC-SA 4.0](https://creativecommons.org/licenses/by-nc-sa/4.0/)，限非营利。见 [cbeta.org/copyright](https://cbeta.org/copyright)
- **Category B**（Y / TX / LC / YP）不是 CC，默认不索引

## 路线图

P0 让人能搜 → P1 邻近 / 校验 / MCP → P2 HTTP 与 REPL → P3 语义搜索（可选）。

见 [docs/roadmap.md](docs/roadmap.md)。
