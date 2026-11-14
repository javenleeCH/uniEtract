# Changelog

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
