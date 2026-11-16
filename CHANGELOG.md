# Changelog

## 0.4.0（未发布）

### 变更

- Agent skill 从 `.codebuddy/skills/ux-extract/` 迁移到 `.agents/skills/ux-extract/`：`.agents/skills/` 是 [Agent Skills 开放规范](https://agentskills.io)的跨客户端通用目录（用户级 `~/.agents/skills/`、项目级 `<project>/.agents/skills/`），不再绑定单一 Agent。`install.py` 的默认 `--user` / `--project` 目标随之改为 `.agents/skills`；`--target` 仍可装进任意 Agent 原生根目录（如 `~/.claude/skills`）。`README.md` 新增「Agent Skill」安装、验证与卸载说明；`dist/ux-extract.zip` 重新打包。
- `ux-extract.zip` 成为 GitHub Release 固定资产：`package_release.py --collect` 新增 `--extra-file`，Release workflow 发布时把 `dist/ux-extract.zip` 连同校验和一并上传（`SHA256SUMS` 含其条目）。下载地址：`https://github.com/javenleeCH/uniEtract/releases/latest/download/ux-extract.zip`（或 `releases/download/v<版本>/ux-extract.zip` 钉住版本）。
- **修复（Windows）**：`--with-source` 的 `source.path` 改为与 `--list` 一致的正斜杠输出。此前 Windows 上 `--list` 输出 `D:/OH/...` 而 `source.path` 输出 `D:\OH\...`，两个稳定契约输出无法直接对账（`comm` 级联全失配）。新增 `model.rs` 回归测试；Unix 行为不变（文件名含字面 `\` 时与 `--list` 的既有归一化同样处理）。
- **新增（`--path-spec`）**：`re:` 片段内的嵌套命名组（如 `.*(?<ts>\d{8}_\d{6})`）现在与显式占位一并产出捕获。0.3.1 会静默丢弃它们——字段从文件名里拆出来却拿不到，是实测反馈的 P1 问题。跨段同名（含嵌套组与显式占位重名）在打开目录前 `Usage(2)`；嵌套组计入 32 捕获预算；片段内冒号（`\d{2}:\d{2}`）行为不变（本就合法）。
- **新增**：空文件（0 字节、有路径）在打开时输出一条 warning `empty file (no records)`；空 stdin 不警告。此前 35,999 清单 vs 35,998 结果的差 1 只能靠人工对账定位。`--quiet` 可压制；`--error-format json` 下为标准 warning 对象。

## 0.3.1 — 2026-11-15

契约收敛轮。`Cargo.toml` / `Cargo.lock` / `ux --version` 均已对齐 `0.3.1`；由 `v0.3.1` tag 经 GitHub Actions 自动构建并发布 Linux / Windows / macOS 预编译包。抽取语义、默认输出格式与 0/1/2 退出码保持不变。

### 破坏性变更与迁移

| 旧行为（0.3.0） | 新行为（0.3.1） | 迁移 |
|---|---|---|
| MSRV Rust 1.85 | MSRV Rust 1.88，CI 增加 1.88 `--locked` 构建门禁 | 升级工具链；`cargo build --locked` 可复现 |
| `--path-spec` 支持 `time:` / `lit:` / 简写 `{%Y-%m-%d}` / `{\d+}`，并做日历与时钟校验 | 只接受显式 `{name:re:...}` / `{name:glob:...}`；**不做日历或时间校验**，未知 kind 与简写在打开目录前返回 `Usage(2)` | 把日期与时间约束写进 `re:` 片段；例如 `time:%Y-%m-%d` 改为 `re:\d{4}-\d{2}-\d{2}`，需排除 02-30 时自行写更严格的片段 |
| `time:` 捕获附带派生 `ts_fields` 数组 | 不再生成任何派生字段；captures 一律为字符串 | 从 `--with-source` 的 `source.captures.ts` 字符串自行切分，例如 `ux --path-spec ... --with-source -e 'text()' \| jq -r '.source.captures.ts \| split("-") \| .[0]'` 取年份；`ux` 不再生成数组，也不校验日历 |
| 发现阶段 `read_dir` 打开失败、`ReadDir` 单条目失败、`file_type` 失败静默跳过 | 三类异常各输出一条 warning（`kind=read_dir` / `kind=read_dir_entry` / `kind=file_type`），`--quiet` 可压制 | 静默吞错不再成立；按 warning 定位权限或竞态问题 |

### 明确的能力边界

- captures 是**只读字符串元数据**，只出现在 `--list` 与 `--with-source` 的 `source.captures`，**不参与流水线求值**；DSL 无法引用它们。
- `--path-spec` 与位置参数互斥；目录结尾的 spec 不隐式追加 `**`；段级剪枝保证不匹配目录不被进入。
- `slurp()` 与 `-O json` **保留，不废弃**。
- 库接口**不承诺稳定 API、不发布库文档**：`pub` 项（`run` / `parse_pipeline` / `cli`）仅用于内部分层与测试，稳定契约只有 CLI。新增 `tests/lib_run.rs` 用 `uniextract::run()` 直调验证退出码与诊断，不代表对外承诺 API。
- 判定按**错误变体**而非 I/O 方向：`AppError::Io(BrokenPipe)` 一律为 0（含输入侧首次 `fill_buf` 与逐行 `read_until` 的裸 IO 错误），`Runtime(String)` 包裹的读错误为 `runtime(1)`。本轮**不改** `source.rs` / `output.rs` / `lib.rs` / `main.rs` 代码，按输入/输出侧区分不属于 0.3.1 契约。
- `catch_unwind` **无条件**收敛主线程 unwind panic；`--error-format json` 额外保证 stderr 只有结构化诊断。库直调 `run()` 同样收敛，但不继承进程级 panic hook。

### 测试与工程

- `tests/pathspec.rs` 迁移到显式 `re:` / `glob:`；简写、`time:`、`lit:` 改为负向 `Usage(2)` 断言。
- `src/pathspec.rs` 新增 `validate_re_fragment`：拒绝 `re:` 片段里的顶层 `|` 与不配平括号（否则会逃逸捕获组、让段级剪枝静默失效）；补转义 / 冒号正则 / 嵌套量词单测。三类发现告警抽成 `warn_io` 并补结构化 JSON 与 `--quiet` 压制单测；`--max-depth` 与 `--path-spec` 组合时改为显式 warning。
- 新增 `tests/pathspec_scenario.rs`（8 项）与 `tests/fixtures/pathmatch/data`（12 个 NativeHeap 夹具）：夹具字节、非法 spec、目录尾不递归、4/12 精确命中、02-30 排除、段级剪枝、3 条 pss 抽取与 `--with-source`、captures 只读与 `--quiet` 对照。
- 新增 `tests/lib_run.rs`（6 项）：库直调 stdout 纯净、输出侧 BrokenPipe=0、`Runtime` 包裹读失败=1、**输入侧裸 IO BrokenPipe=0 的如实负向用例**、usage 结构化、panic 收敛为 runtime；用全局 Mutex 串行化 panic hook 安装。
- 旧 Python 校验原型下线，夹具与断言以 Rust 测试为单一真相；`scenarios/pathmatch/generate.py` 改为写入 `tests/fixtures/pathmatch/data`。
- 设计文档按当前实现重写到 `docs/cli-design.md` 与 `docs/path-spec-design.md`；`.trae/` 目录删除，README 与 CI 门禁的引用同步指向 `docs/`。
- CI：需要解析依赖的 cargo 步骤一律 `--locked`（`cargo fmt` 不适用）；Rust 1.88 MSRV job 含 `cargo test --all-targets --locked`；工具链固定为 1.88，避免 `-D warnings` 受新 lint 影响；三个独立门禁脚本 `.github/scripts/check_contract_docs.py`（必需文件断言、分组扫描过期文案、校验 12 个夹具）、`.github/scripts/check_test_count.py`（`-- --list` 汇总行统计）、`.github/scripts/check_fuzz_manifest.py`（`cargo metadata --locked`，去掉 `--no-deps`）；全部 `python -c` 内联移除，bash 与 pwsh 行为一致；删除与 `--all-targets` 重复的 agent 测试步骤。
- `tests/cli.rs` 新增 `--help` 的 PATH SPEC / ROADMAP 文案正向回归断言。
- 修复 GitHub CI：Rust 1.88 clippy 的 `uninlined_format_args` 会让 `tests/pathspec.rs` / `tests/pathspec_scenario.rs` 的 `assert_eq!` 在 `-D warnings` 下失败；两处改用内联格式参数。README 补上 `cargo +1.88.0 ...` 的本地同工具链复现命令。
- 新增 tag 驱动的发布流水线 `.github/workflows/release.yml`：`v*` tag 触发四个平台（Linux x86_64、Windows x86_64、macOS x86_64 / arm64）的 `--locked` release 构建与打包，由 `check_release_version.py` 统一校验版本、`package_release.py` 生成压缩包与 `SHA256SUMS`，发布 job 校验全部产物后写入 GitHub Release；CI verify 增加版本一致性步骤。
- 当前 Rust 测试共 **167 项**（92 单测 + 21 Agent 契约 + 30 CLI + 3 兼容 + 6 库直调 + 7 pathspec + 8 场景），在 Rust 1.88 下全部通过。

### 0.4.0 时间表（已定稿）

| 项 | 决定 |
|---|---|
| `--path-regex` / `--root` | 做，与 `--path-spec` **并存** |
| captures 求值注入 | 做，显式 opt-in，默认仍只读 |
| `slurp()` / `-O json` | **保留，不废弃** |
| 提取提示（hint） | **不做** |
| 库接口 | **不承诺稳定库 API、不发布库文档** |
| BrokenPipe 按输入/输出侧区分 | **做（甲方技术复核增列）**：输入侧读错输出 `runtime(1)` 诊断，不再静默返回 0；验收点为输入侧管道破裂时 `rc=1` 且 stderr 有 `runtime` 诊断 |

> 最后一项由甲方技术第二轮复核要求增列：0.3.1 只做如实文档化与负向测试，未改 I/O 代码，因此把「输入侧读错被静默当成功」留到 0.4.0 闭环，避免把当前行为固化为长期契约。

## 0.3.0

面向开发者与 AI Agent 的接口面打磨。抽取语义、默认输出格式与既有退出码保持不变。

### 新增

- `--error-format text|json`：stderr 每行一条诊断；JSON 模式输出稳定字段 `code` / `message` / `exit_code`，`eval` 额外带 `stage` / `path` / `line` / `record`，warning 也结构化。
- `--quiet`（别名 `--silent`）：压制非致命 warning，错误始终输出。
- `--list`（别名 `--dry-run`）：只做文件发现并在 stdout 输出 JSONL 清单（`type` / `path`），不打开文件、不需要 `-e`，供 Agent 先确认输入集。
- `--no-stdin`：显式拒绝隐式 stdin 与 `-`，CI/Agent 不会因漏传路径而阻塞。
- `--completions bash|zsh|fish|powershell|elvish`：生成 shell 补全脚本（新增 `clap_complete`）。
- 完整 `--help` / `-h`：阶段速查、输入输出说明、Agent 契约、示例、退出码与内存代价提示。
- `tests/agent.rs`：21 项面向自动化调用者的契约测试（stdout 纯净、结构化 stderr、退出码、清单、补全、限制行为、稳定性）。
- 分发元数据：`rust-version`、keywords / categories、`LICENSE`；CI 增加 Agent 契约测试与分发元数据检查。

### 修复

- XPath 未绑定前缀不再触发 `sxd-xpath` panic（rc=101）：编译期返回 `Query(2)` 并提示 `--ns PREFIX=URI`。
- 增加 `catch_unwind` 兜底：主线程 unwind panic 转为 `runtime` 诊断（退出 1），JSON 模式下 stderr 保持纯净（0.3.1 已把该承诺收窄为「无条件收敛 + JSON 模式额外保证 stderr 纯净」，不再作全局 rc=101 断言）。
- `--max-results` 触顶立即终止：不再继续遍历文件、不再重复报错，`-O json` 不输出残缺聚合数组。
- 失败运行（`-O json`）清空待写聚合，避免输出看似完整统计的部分数组；jsonl / text 已产出的记录保留。
- 非 UTF-8 转码路径纳入 `--max-input-size`：原始字节读取带上限，避免转码前无界 `read_to_end` 的 OOM 风险；解码后不再二次套用同一上限（GB18030 会膨胀，曾误拒合法文件）。
- 修正 `--ns` 文档：URI 实际参与文档命名空间匹配，等价 URI 的不同前缀可命中。
- 无路径且 stdin 为终端时返回 `Usage(2)` 并提示显式输入，避免 Agent 挂起；显式 `-` 仍可交互读取。
- `sample/CASES.md` 的「已知待修项」更新为已修复说明，与 0.3.0 行为一致。
- `--help` 固定 `Usage: ux ...`（`bin_name`），Windows 不再显示 `ux.exe`；`--list` 在交互终端且无路径时不再被 stdin 保护误拒（补回归单测）。

### 内部变更

- 新增 `src/diagnostic.rs` 统一收口 stderr；`AppError` 增加 `Limit` 变体、`ErrorCode` 与 `diagnostic()`；`discover` / `InputSource::open` 改由 `Reporter` 输出。
- `Cargo.toml` 版本升至 0.3.0；`-e` 改为 `Option<String>`，仅 `--list` / `--completions` 时可省略。

## 0.2.0

按 v3 设计完成阶段 0–6：类型静态化、文件发现、slurp/split 会话聚合、输入卫生、输出收口与端到端测试。

### 新增

- DSL：`slurp()`（跨文件全局聚合）、`split(PATTERN)`；编译期 `TypeSet` 类型上界检查。
- `attr()` 可直接读取 `xpath("//@href")` 产生的属性节点。
- 文件发现：目录递归剪枝、glob、`--fuzzy`（nucleo-matcher）、`--max-depth`、`--exclude`、`--no-default-ignore`、稳定排序与去重。
- 输入卫生：`--encoding`、`--force-text`、BOM 剥离、UTF-16 豁免的二进制探测、`--max-input-size`、`--max-record-size`、CRLF 剥离、`--skip-incompatible`。
- 输出：`-O json` 默认缩进 + `--compact`、`--max-results`、`--line-buffered`（终端自动启用）、64 KiB 批量刷新。
- jq 数值契约：显式 `Val → JsonValue` 转换，大整数字面量保真，`NaN`/`±Inf` 映射为 `null`。
- 端到端测试：`tests/cli.rs`（26 项，含 HTML 隐式 `<tbody>` / `<template>` / `<noscript>` / 大小写折叠专项）、`tests/jq_compat.rs`（3 项）、CI 双平台门禁、`fuzz/` DSL 解析骨架。
- 样本夹具：`sample/` 下 41 个文件覆盖全部输入格式与边界；`sample/generate.py` 可重建非 UTF-8/二进制夹具，`sample/smoke.py` 提供 45 条可执行断言并已接入 CI。

### 破坏性变更与迁移

| 旧行为 | 新行为 | 迁移 |
|---|---|---|
| `--with-source -O text` 静默输出 JSON 行 | `Usage(2)`，提示互斥 | 改用 `-O jsonl` 或 `-O json` |
| `--fuzzy` 静默忽略、全量输入 | 正常过滤 | 无需改动；如需旧行为，去掉该参数 |
| `-` 多次：`Query(2)` | `Usage(2)` | 无 |
| `jq(nan)` / `jq(infinite)` 运行期错误 | 输出 `null` | 无 |
| 每结果 flush（慢但管道即时可见） | 默认批量 flush | 管道需 EOF 前可见时加 `--line-buffered` |
| `-O json` 紧凑单行 | 默认缩进 2 空格 | 需紧凑时加 `--compact` |
| 空 glob → 打开失败、退出 1 | 无输出 + warning、退出 0 | 依赖退出码 1 的脚本需调整 |
| 显式二进制文件按文本读 | 报错退出 1 | 确需按文本处理时加 `--force-text` |
| 目录内二进制按文本读 | warning 跳过、退出 0 | 同上 |

### 修复

- v0.1 的 `jq` 结果经 `to_string → from_str` 回环：`NaN`/`Infinity` 打印为非法 JSON 导致解析失败；改为枚举遍历转换。
- v0.1 的 `--fuzzy` 死开关静默全量输入。
- 深层嵌套的 DSL 括号会递归触发 pest 的 `group` 规则，约 500 层时进程栈溢出终止；现在由 `check_nesting(128)` 前置拦截并返回明确的 `Query(2)`。

### 内部变更

- DSL 结构解析从「手写字节状态机 + winnow」换成 **pest 2 PEG**（新增 `src/dsl.pest`）：顶层管道切分、字符串/括号配平、任意井号原始字符串统一由语法描述；`src/dsl.rs` 只保留阶段名分派、转义解码与深度守卫。移除直接依赖 `winnow`（`Cargo.lock` 中仍作为 `toml`/`clap` 的传递依赖）。
- 未启用 pest 的 `stack-guard` feature（嵌套已限深，动态栈扩展无收益）；也未使用 `pest::set_call_limit`，因为它按输入长度计数，会误拒长正则。注：`pest 2.9.6` 的默认 `std` feature 本身会带入 `pest_stack_guard`/`stacker`/`psm`/`cc` 构建期依赖，关掉 `stack-guard` 不会移除它们。
- 新增 10 项 DSL 单测：任意井号 raw string、井号数不匹配、字符串内含 `)`/`|`、多字节字符、空白容忍、深度边界、2 万字符长参数、2000 条确定性乱码输入不 panic。

## 0.1.0

初始实现：jq / XPath / 正则组合流水线，JSON、JSONL、YAML、TOML、XML、HTML、text、CSV/TSV 输入，jsonl/text/json 输出。
