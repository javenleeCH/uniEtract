# Changelog

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
- 增加 `catch_unwind` 兜底：任何意外 panic 转为 `runtime` 诊断（退出 1），JSON 模式下 stderr 保持纯净，不再出现 rc=101。
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
