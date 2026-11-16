# ux CLI 设计（0.3.1 实现状态）

> 本文描述当前代码的实际设计，是定稿说明而非提案。用户手册与 Agent 契约见 `README.md` 和 `ux --help`；变更记录与版本路线见 `CHANGELOG.md`；结构化路径匹配的独立设计见 `docs/path-spec-design.md`。

## 1. 产品定义

`ux` 是一个本地、单进程、串行的信息抽取 CLI，面向软件开发者、CI 脚本与 AI Agent。它把 jq、XPath、正则等能力放进**同一条记录流**，由一条管道表达式组合，而不是拆成多个子命令。

- **唯一稳定契约是 CLI**。crate `uniextract` 的 `pub` 项（`cli`、`run`、`parse_pipeline`）仅用于内部分层与集成测试；不承诺稳定的库 API，也不发布库文档。
- 输入来自文件、目录、glob、结构化路径匹配或 stdin；输出默认 JSONL。
- 设计取舍是「宁可吵，不可骗」：错误必须可观测，限制触顶必须报错而非静默截断，stdout 只承载数据。

### 能力边界

| 不做 | 原因 |
|---|---|
| 网络访问 / 远程数据源 | 单进程本地工具，输入集必须可预演 |
| 并发与并行调度 | 保证输出顺序字节级稳定、诊断可定位 |
| 稳定库 API / SDK | 契约面收敛到 CLI，避免内部重构破坏调用方 |
| 完整 jq / XPath 1.0 兼容 | 只保证实测过的函数、轴与语法 |
| schema 校验、join、跨文件关联 | 单记录流模型，不做关系运算 |
| captures 参与求值 | captures 是来源元数据，不是记录字段 |
| 结果排序 / 提取提示（hint） | 排序由输入发现顺序决定，提示会引入隐式行为 |
| 跨段通配与自动 `**` 追加 | 路径匹配必须显式，避免意外遍历 |

## 2. 版本与技术基线

| 项 | 值 |
|---|---|
| crate / 二进制 | `uniextract` / `ux` |
| `Cargo.toml` version | `0.3.1`（由 `v0.3.1` tag 发版；`Cargo.lock` 与 `ux --version` 同步） |
| edition / MSRV | `2024` / Rust `1.88` |
| 依赖锁定 | `Cargo.lock` 与 `fuzz/Cargo.lock`，CI 全量 `--locked` |
| 许可 | MIT |

关键依赖与职责：

| 依赖 | 用途 |
|---|---|
| `clap` / `clap_complete` | 参数解析、校验、5 种 shell 补全 |
| `pest` / `pest_derive` + `dsl.pest` | 管道结构扫描 |
| `jaq-core` / `jaq-json` / `jaq-std` | `jq` 阶段求值 |
| `sxd-document` / `sxd-xpath` / `sxd_html` | XML / HTML 解析与 XPath |
| `regex` | `re`、`split` 阶段与路径 spec 片段 |
| `serde_json`（`arbitrary_precision`）、`serde_yaml_ng`、`toml`、`csv` | 10 种输入格式的归一化 |
| `encoding_rs` | 非 UTF-8 转码 |
| `glob` / `walkdir` | 路径参数 glob 与目录递归 |
| `nucleo-matcher` | `--fuzzy` 路径过滤 |
| `thiserror` | `AppError` 派生 |

## 3. 模块结构

`src/lib.rs` 只把 `cli` 声明为 `pub mod`，其余模块均为 crate 内私有。

| 模块 | 职责 |
|---|---|
| `main.rs` | 进程入口：`--completions` 短路、注入 TTY 状态、JSON 模式安装空 panic hook |
| `lib.rs` | `run()` / `run_inner()` 编排、`catch_unwind` 兜底、`--list` 短路、诊断收口 |
| `cli.rs` | `Cli` 参数定义、`ValueEnum`、`validate()`、长帮助文本 |
| `error.rs` | `AppError` / `ErrorCode` / `diagnostic()` 结构化字段 |
| `diagnostic.rs` | `Reporter`：stderr 单行诊断、文本/JSON 双格式、`--quiet` 压制 |
| `dsl.rs` + `dsl.pest` | 管道解析：grammar 管结构，Rust 侧管阶段名与字面量 |
| `stage/{jq,xpath,regex,mod}.rs` | 7 阶段的具体实现 |
| `engine.rs` | `Pipeline` / `Relay`：阶段间统一数据模型传递 |
| `model.rs` | `Item`、`Origin`（含 `captures`）、`ItemSink` |
| `types.rs` | `Limits`：体积与结果数上限 |
| `markup.rs` | XML / HTML 容错树、命名空间绑定 |
| `discover.rs` | 路径参数展开、默认忽略表、fuzzy / exclude 过滤、`Input` |
| `pathspec.rs` | `--path-spec` 编译与段级 frontier 遍历（见 `docs/path-spec-design.md`） |
| `source.rs` | 输入源打开、BOM / 编码探测、转码、二进制检测、体积限流 |
| `input.rs` | 按格式切成记录流，附加 `Origin` 溯源 |
| `output.rs` | `-O jsonl/text/json` 三态输出、`--max-results` 触顶即停、`abandon()` |

## 4. 执行模型

```
main → Cli::parse → run(cli, stdin, stdout, stderr)
     → Reporter::new(stderr, error_format, quiet)
     → catch_unwind(AssertUnwindSafe(run_inner))
```

`run_inner` 的顺序固定：

1. `Cli::validate()`：参数互斥、stdin 边界、`--path-spec` 预编译。非法 spec 在任何目录被打开之前返回 `Usage(2)`。
2. `--completions` 已由 `main.rs` 短路；`--list` 在此输出输入集 JSONL 后立即返回。
3. `discover()`（或 `pathspec::discover()`）得到输入列表，按归一化相对路径字典序串行处理。
4. `parse_pipeline()` 编译为 `Vec<StageSpec>`，由 `Pipeline` / `Relay` 逐阶段传递统一 `Item`。
5. 每个输入经 `InputSource` → `process_source` → `OutputSink`；`Eval` 错误携带 `Origin`（`path` / `line` / `record` / `stage`）。
6. `--with-source` 把 `Origin` 包进输出；`max_results` 触顶抛 `Limit`，`-O json` 调用 `abandon()` 丢弃未完成的聚合数组。

**7 个阶段**（`dsl.rs::StageSpec`）：`jq` / `xpath` / `re` / `text` / `attr` / `split` / `slurp`。`slurp()` 与 `-O json` 会在写第一个字节前缓存全部结果；默认 64 KiB 批量写，`--line-buffered` 或 TTY 时逐条 flush。

**panic 收敛**：`catch_unwind` 无条件把主线程 unwind panic 转成 `runtime(1)` 结构化诊断；`--error-format json` 额外安装空 panic hook 保证 stderr 纯净。库直调 `run()` 同样收敛，但不继承进程级 hook。

**限制**：`--max-input-size`（默认 256MiB，转码前按原始字节限流）、`--max-record-size`（默认 8MiB）、`--max-results`（0 = 不限）。`0` 表示对应上限关闭。

## 5. Agent 契约

| 关注点 | 契约 |
|---|---|
| stdout | 只承载数据；`-O jsonl` 每行紧凑 JSON，`-O json` 单个数组，`-O text` 原样文本；`--list` 输出输入清单 JSONL |
| stderr | 每条诊断一行；`--error-format json` 时每行都是可解析 JSON；`--quiet` 只压 warning，错误必出 |
| 退出码 | `0` 成功（含零匹配）、`1` 运行期错误、`2` 参数/编译错误 |
| 顺序 | 文件字典序串行；`--list` 顺序与实际处理顺序一致；多次调用字节级稳定 |
| 挂起防护 | 无路径默认读 stdin；stdin 是终端且未显式给 `-` 时返回 `Usage(2)`；`--no-stdin` 彻底禁用 |
| 诊断字段 | `code` / `message` / `exit_code` 恒存在；`eval` 追加 `stage` / `path` / `line` / `record`；字段只追加不重命名 |

`ErrorCode` 取值与退出码：

| `code` | 含义 | 退出码 |
|---|---|---|
| `query` | 管道语法 / 阶段编译失败 | 2 |
| `usage` | 参数非法或互斥 | 2 |
| `eval` | 求值阶段失败 | 1 |
| `runtime` | 运行期错误（含收敛后的 panic） | 1 |
| `limit_exceeded` | 结果数触顶 | 1 |
| `io` | IO 错误 | 1 |
| `warning` | 非致命告警 | 0 |

**BrokenPipe（0.3.x 当前行为）**：判定按错误变体而非 I/O 方向。`AppError::Io(BrokenPipe)` 一律视为正常终止（退出 0、不写诊断），`Runtime(String)` 包裹的读错误为 `runtime(1)`。输入侧首次 `fill_buf` 与逐行 `read_until` 产生的 BrokenPipe 同样返回 0。`ux --help` 的 ROADMAP、`README.md` 与 `CHANGELOG.md` 记录了 0.4.0 的方向区分计划。

`--list`（别名 `--dry-run`）不打开文件、不需要 `-e`；`--completions` 支持 bash / zsh / fish / powershell / elvish；`--quiet` 的别名为 `--silent`。

## 6. 输入与输出

**格式**（`InputFormat`）：`auto` / `json` / `jsonl` / `yaml` / `toml` / `xml` / `html` / `text` / `csv` / `tsv`。`-f auto` 依赖扩展名；无扩展名的 JSONL 需显式 `-f jsonl`。`TextMode` 为 `lines`（默认）或 `raw`。

**编码**（`Encoding`）：`utf-8` / `utf-16le` / `utf-16be` / `gbk` / `gb18030` / `auto`。BOM 探测与非 UTF-8 转码在 `source.rs`；转码前原始字节受 `--max-input-size` 限制，转码后不再二次判拒以避免解码膨胀误伤。二进制输入默认跳过并告警，`--force-text` 可绕过。

**输入发现**：路径参数支持文件、目录、glob 与 `-`；默认忽略表为 `.git` / `.hg` / `.svn` / `target` / `node_modules` / `.venv` / `__pycache__` / `.trae`；`--no-default-ignore` 关闭，`--exclude` 追加排除 glob，`--fuzzy` 做路径模糊过滤，`--max-depth` 限制递归深度，`--strict` 让首个文件错误即停。`--path-spec` 与位置参数互斥，且 `--max-depth` 在该模式下输出一条忽略告警。

**输出**：三态 `OutputSink`；`-O json` 遇 `max_results` 触顶时 `abandon()` 保证不吐残缺数组；`--compact` 关闭 pretty printing。

## 7. 测试与门禁

Rust 测试共 **167 项**：

| 位置 | 项数 | 覆盖 |
|---|---|---|
| `src/*` 内嵌单测 | 92 | 各模块单元行为，含 `pathspec.rs` 11 项 |
| `tests/agent.rs` | 21 | Agent 契约：诊断、quiet、list、no-stdin、completions、TTY、帮助文本 |
| `tests/cli.rs` | 30 | 端到端管道、格式、错误码、`--help` 的 PATH SPEC / ROADMAP 文案 |
| `tests/jq_compat.rs` | 3 | jq 兼容性夹具 |
| `tests/lib_run.rs` | 6 | 库直调 `run()`：stdout 纯净、BrokenPipe 两类变体、usage、panic 收敛 |
| `tests/pathspec.rs` | 7 | `--path-spec` CLI 端到端 |
| `tests/pathspec_scenario.rs` | 8 | NativeHeap 12 夹具的 8 项场景断言 |

工程门禁：

- `sample/smoke.py` 45 条冒烟；`sample/export_cases.py --check` 保证 `sample/CASES.md` 与用例同步。
- `.github/scripts/check_contract_docs.py`：断言必需文件存在、扫描过期契约文案、校验 12 个 NativeHeap 夹具。
- `.github/scripts/check_test_count.py`：以 `cargo test -- --list` 的汇总行统计测试数并锚定 167。
- `.github/scripts/check_fuzz_manifest.py`：`cargo metadata --manifest-path fuzz/Cargo.toml --locked --format-version 1`。
- `.github/workflows/ci.yml`：双 OS `verify`（fmt / clippy `-D warnings` / test / release / smoke / 三个 Python 门禁）与 Rust 1.88 `msrv` job；工具链固定为 1.88，避免 `-D warnings` 被新 lint 破坏；无 `python -c` 内联，bash 与 pwsh 行为一致。

## 8. 生效的架构决策

| # | 决定 | 理由 |
|---|---|---|
| A1 | CLI 是唯一稳定契约，库接口不公共化 | 内部模块可自由重构，调用方不被 `pub` 面绑死 |
| A2 | 7 阶段共用单记录流与统一 `Item` | 组合能力来自管道，而非子命令矩阵 |
| A3 | 结构化诊断 + 0/1/2 退出码 | Agent / CI 可基于 `code` 分支重试，不解析人类文案 |
| A4 | `catch_unwind` 无条件收敛 + JSON 模式空 panic hook | 进程级保证 stderr 纯净与退出码稳定 |
| A5 | 限制触顶即停，`-O json` 失败时 `abandon()` | 不输出误导性的部分聚合 |
| A6 | 非 UTF-8 转码前按原始字节限流 | 避免无界 `read_to_end` 与解码膨胀二次误拒 |
| A7 | `--list` 与正常运行共用 `validate()` | 预演与实跑看到同一套输入集与错误 |
| A8 | MSRV 固定 1.88，两份 lock 全 `--locked` | 可复现构建，MSRV 由 CI 独立把关 |
| A9 | 文档分散为 README / CHANGELOG / `--help` / `docs/` 两份设计，由 CI 门禁防止漂移 | 每份文档有明确读者与职责，交叉引用可机器校验 |

## 9. 文档索引

| 文档 | 读者 | 内容 |
|---|---|---|
| `README.md` | 用户 / Agent | 安装、用法、稳定契约、能力边界、0.4.0 时间表 |
| `CHANGELOG.md` | 维护者 / 升级方 | 版本变更、破坏性变更、迁移表 |
| `ux --help` | 只看命令行的用户 | STAGES、AGENT CONTRACT、PATH SPEC、ROADMAP、EXAMPLES |
| `docs/cli-design.md` | 维护者 | 本文：当前实现的设计与生效决策 |
| `docs/path-spec-design.md` | 维护者 | 结构化路径匹配的语法、编译、剪枝与夹具 |
