# cbeta-cli

离线查 [CBETA](https://www.cbeta.org/) 经论。给人用的是命令行；MCP / HTTP 是同一套 `Command` 的运输。

```bash
cbeta 真性有为空
```

数据仓库：[wedreamer/cbeta-corpus](https://github.com/wedreamer/cbeta-corpus)
跟踪：[#1 v0.1 人用最小闭环](https://github.com/wedreamer/cbeta-cli/issues/1)

> 状态（**2026-09-10**）：**v0.1 搜索闭环已落地**；**v0.2 语料生命周期（Now）**：`fetch` / `releases` / `use` / `current` / `pull` / `gc` / `prune` / 增量 `build`。仍 Later：`--window`、分页器（pager）、管道 JSONL 默认、`fuzzy`、P3 semantic。

## 安装

### 预编译二进制（GitHub Releases）

`v*` tag 会构建四个 host-native 产物（另附 `sha256sums.txt`）：

| 平台 | 文件 |
|---|---|
| Linux x86_64 | `cbeta-x86_64-unknown-linux-gnu.tar.gz` |
| macOS Apple Silicon | `cbeta-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `cbeta-x86_64-apple-darwin.tar.gz` |
| Windows x86_64 | `cbeta-x86_64-pc-windows-msvc.zip` |

每个归档含 `cbeta`（Windows 为 `cbeta.exe`）、`LICENSE`、`README.md`。

```bash
# 例：Linux x86_64（把解出的 cbeta 放到 PATH）
curl -fsSL -O https://github.com/wedreamer/cbeta-cli/releases/latest/download/cbeta-x86_64-unknown-linux-gnu.tar.gz
tar -xzf cbeta-x86_64-unknown-linux-gnu.tar.gz
```

首次 tag 发布前，用下面的 cargo 安装。

### 从源码

```bash
# 远程
cargo install --locked --git https://github.com/wedreamer/cbeta-cli.git cbeta-cli

# 或本地 clone
git clone https://github.com/wedreamer/cbeta-cli.git
cd cbeta-cli
cargo install --locked --path crates/cbeta-cli
```

经文不进本仓库。首次准备语料与索引：

```bash
cbeta fetch --release 2026R2 --scope taisho
cbeta use 2026R2 --scope taisho
# 或：cbeta build --scope taisho
```

索引默认在 `~/.cbeta/index`；语料缓存 `~/.cbeta/corpus/<tag>/`，由 `CURRENT` 指向当前标签。镜像可设 `CBETA_GIT_BASE`。

## 人怎么用

```bash
# 裸 cbeta 进 REPL；带词直接搜（无子命令也默认 search）
cbeta            # REPL：> 空性+缘起，:scope T1578，:open 3，:copy 3，:verify <原句>，:q 退出
cbeta 色即是空
cbeta search 无常 --canon T

# CBReader 肌肉记忆：+ 是 NEAR/30（带 span 确认），* 是 BEFORE/30
cbeta search '空性+缘生' --type lun
cbeta search '真如 NEAR/16 缘起' --work T1585   # 本经内搜

# 这句是不是原文（从微信/笔记直接贴）
cbeta verify '真性有为空，缘生故如幻，无为无起灭，不实若空华。'

# 打开出处，像 rg -C
cbeta get T30n1578_p0268b21 -C 4
cbeta read T0235 --juan 1
cbeta cite T30n1578_p0268b21
# (CBETA 2026.R2, T30, no. 1578, p. 268, b21)

# 保存这次搜索结果，下次按序号取（`--copy` 是开关，序号走 `--index`）
cbeta search --save last 真性有为空
cbeta get --from last --index 1 -C 4
cbeta get --from last --index 1 --copy   # 引文块打到 stdout（不碰系统剪贴板）

# 显示脚本：默认繁體，--script s 看简体（只影响显示，不改 line_id / 索引键）
cbeta get T30n1578_p0268b21 --script s

# 先找书再搜正文。`--author`/`--title` 过滤 catalog.jsonl 字段；
# 2026R2 sidecar 目前多为 null（语料导出，不是 CLI 漏填），会空命中。
cbeta catalog --author 玄奘 --type lun
cbeta info

# 装 shell 补全 / 跑微基准
cbeta completion zsh > ~/.zfunc/_cbeta
cbeta bench --scope taisho
```

TTY 默认人可读：高亮、`line_id`、经名、作译者。`--json` 输出与 MCP 同 schema，但**永不默认**；管道 JSONL 默认输出仍是 **planned**（现在管道里要靠 `--json`）。`--plain` 去颜色、稳定列。
退出码跟 rg：`0` 有命中，`1` 无命中，`2` 用法或索引错误；`verify` 例外，原文与否都退 `0`，只有没索引才 `2`。

## 搜索方式

距离单位是归一化后的 **汉字**，不是 token。CBReader 默认邻近距 30 字。

| 方式 | 例子 | 状态 |
|---|---|---|
| keyword | `cbeta search 空性` | 已落地 |
| phrase | `cbeta search --mode phrase '真如远离'` | 已落地 |
| near | `cbeta search '空性 NEAR/16 缘生'` 或 `'空性+缘生'` | 已落地（2-gram 召回 + 字符 span 确认） |
| before | `cbeta search '空性 BEFORE/16 缘生'` 或 `'空性*缘生'` | 已落地 |
| boolean | `&`（AND）、`,`（OR）、`-` 或 `NOT`（排除） | 已落地 |
| wildcard | `'莲?色'` | 已落地（单字 `?`，每词最多 2 个；2026R2 可命中青蓮色/紅蓮色） |
| fuzzy | 1–2 字误差 / 异体 | **planned**，引擎未落地 |
| verify | 是否原文；不是则返近句 | 已落地 |

Agent 用 MCP `cbeta_search` 的结构化 `clauses`（`Vec<String>`，映射为 `ParsedQuery`，默认 near/30），不要拼 CBReader DSL。CLI 侧 `Command` 仍只有字符串 `q` 走 `parse_query`，没有 `Command.clauses` 类型。语义搜索是独立工具（P3 可选），不参与「是不是原文」。

```bash
# 看解析结果（--explain --json 目前 dump Command/parsed_query，不是 hits）
cbeta search --explain --json '空性+缘生'
```

## MCP / HTTP

`cbeta serve` 起 stdio MCP；`cbeta serve --http 127.0.0.1:1873` 起 HTTP。五个工具，与 CLI 一一对应：

| MCP 工具 | CLI |
|---|---|
| `cbeta_search` | `cbeta search` |
| `cbeta_verify_quote` | `cbeta verify` |
| `cbeta_get_passage` | `cbeta get` / `read` / `cite` |
| `cbeta_list_catalog` | `cbeta catalog` |
| `cbeta_index_info` | `cbeta info` |

详见 [docs/search-modes.md](docs/search-modes.md)、[docs/human-ux.md](docs/human-ux.md)。

## 两个仓库

| 仓库 | 职责 |
|---|---|
| [cbeta-corpus](https://github.com/wedreamer/cbeta-corpus) | 锁定 xml-p5@2026R2、筛选 scope、校验、导出 catalog |
| **cbeta-cli** | 解析、索引、人用 CLI、引文校验、`cbeta serve` |

产物 ID：`{cbeta_tag}+{scope_hash}`，例 `2026R2+a3f91c2e`。改 scope 必须重建索引（`build` 走临时目录 + 原子切换）。

```text
crates/cbeta-core     Command / Hit / ParsedQuery / Filters
crates/cbeta-parse    TEI P5
crates/cbeta-index    Tantivy
crates/cbeta-search   keyword / near（span 确认）/ verify
crates/cbeta-cli      二进制名 cbeta
```

## 版权

- **本仓库代码**：MIT
- **CBETA 经文**：[CC BY-NC-SA 4.0](https://creativecommons.org/licenses/by-nc-sa/4.0/)，限非营利。见 [cbeta.org/copyright](https://cbeta.org/copyright)
- **Category B**（Y / TX / LC / YP）不是 CC，默认不索引

## 路线图

P0 搜索 → P1 邻近 / 校验 / MCP → P2 HTTP、REPL、`--save`、completion、`bench` 均已落地（2026-09）。P3 语义搜索（可选）。

见 [docs/roadmap.md](docs/roadmap.md)。
