# ux CLI 契约参考

## 命令形态

```
ux [FLAGS] -e 'stage(...) | stage(...)' [PATH|GLOB|- ...]
```

`-e` / `--extract` 是必填项，唯一例外是 `--list` 与 `--completions`。管道表达式必须是**一个参数**，用引号包住；`|` 在括号深度为 0 且不在字符串内时才切分阶段，因此 `jq(.a | .b)` 不会误拆。阶段参数支持 `r"..."` / `r#"..."#` 原始字符串与 `\n \r \t \\ \' \"` 六种转义。DSL 括号嵌套上限 128 层，超限报 `query`（退出 2）。

## 关键参数

| 参数 | 作用 |
|---|---|
| `-e`, `--extract` | 管道表达式 |
| `-f`, `--format` | `auto\|json\|jsonl\|yaml\|toml\|xml\|html\|text\|csv\|tsv` |
| `-O`, `--output` | `jsonl`（默认）\| `text` \| `json` |
| `--text-mode` | `lines`（默认）\| `raw` |
| `--encoding` | `utf-8\|utf-16le\|utf-16be\|gbk\|gb18030\|auto` |
| `--ns PREFIX=URI` | XPath 前缀绑定，可重复；URI 与文档命名空间 URI 匹配 |
| `--path-spec SPEC` | 结构化路径匹配，与位置参数互斥 |
| `--list`, `--dry-run` | 只输出输入集 JSONL，不读文件内容；每行带 `type`/`path`（及 path-spec 的 `captures`），并追加 `format`（仅当格式可由 `-f` 或扩展名确定；内容无关提示） |
| `--completions SHELL` | bash / zsh / fish / powershell / elvish |
| `--error-format` | `text`（默认）\| `json` |
| `--quiet`, `--silent` | 只压**非结果影响** warning，错误照常输出；结果影响 warning（跳文件/空文件/没命中/零匹配）照出，JSON 带 `"category":"result_set"` |
| `--no-stdin` | 完全拒绝 stdin（隐式或 `-`），CI / agent 必加 |
| `--with-source` | 输出 `{value, source:{path,line,record,captures}}`；与 `-O text` 互斥。`source.path` 与 `--list` 的 `path` 一致使用 `/` 分隔符，可直接对账 |
| `--fuzzy QUERY` | 对归一化相对路径做后置模糊过滤（不做目录剪枝） |
| `--exclude GLOB` | 追加排除 glob，可重复 |
| `--no-default-ignore` | 关闭默认忽略表（`.git`、`.hg`、`.svn`、`target`、`node_modules`、`.venv`、`__pycache__`、`.trae`） |
| `--max-depth N` | 目录递归深度；`--path-spec` 下被忽略并告警 |
| `--skip-incompatible` | 跳过首阶段无法处理的源文件 |
| `--strict` | 首个文件错误即停 |
| `--force-text` | 绕过二进制检测 |
| `--max-input-size` | 整文档上限，默认 256MiB，`0` 不限 |
| `--max-record-size` | 单行/记录/切分片段上限，默认 8MiB |
| `--max-results N` | 第 N+1 条时报错退出 1，立即停止 |
| `--line-buffered` | 逐条 flush；长驻管道必加 |
| `--compact` | 关闭 `-O json` 缩进 |

## 8 个阶段

| 阶段 | 接受 | 输出 |
|---|---|---|
| `jq(FILTER)` | Data / Text | JSON 字符串回落 Text，其余 Data |
| `xpath(EXPR)` | Node | nodeset 展开为 Node；标量结果为 Text/Data |
| `re(PATTERN)` | Text | 无捕获组输出完整匹配 Text；有捕获组输出含 `0`/`1`/命名键的 Data 对象 |
| `text()` | 任意 | string-value / 原样 / 紧凑 JSON → Text |
| `attr(NAME)` | 元素或属性 Node | 属性值 Text，缺失则 0 条 |
| `split(PATTERN)` | Text | 按正则切分的 Text 片段，空片段丢弃、不 trim |
| `slurp()` | 任意 | 全部输入收敛为一个 Data 数组，至多一个，跨文件全局 |
| `captures()` | 任意 | `--path-spec` captures 对象（显式 opt-in；无 path-spec 时为 null）→ Data |
| `check(FILTER)` | Data / Text | 断言每条记录满足 FILTER（jq 真值）；通过则原样传递，失败为 `eval(1)` |

编译期做类型上界检查（`slurp() | xpath(...)` 这类组合在读取输入前报错退出 2）；运行期仍有值级类型检查。

## 输入与输出

- `-f auto` 依赖扩展名；无扩展名的 JSONL 会被误判为 JSON，必须显式 `-f jsonl`。YAML/TOML/CSV/TSV 也建议显式指定。
- BOM 会被剥离；`auto` 回退 GB18030 时 stderr 有 `assumed GB18030` warning。转码前原始字节受 `--max-input-size` 约束，解码后不再二次套用。
- 输出换行恒为 LF。`-O jsonl` 每行紧凑 JSON；`-O json` 聚合成数组（默认缩进 2）；`-O text` 原样文本。
- `-O json` 与 `slurp()` 会在写第一个字节前缓存全部结果；`--with-source` 叠加 `slurp()` 只保留第一条来源。
- 文件按归一化相对路径字典序串行处理；`--list` 顺序与实际处理顺序一致，多次调用字节级稳定。

## 诊断与退出码

`--error-format json` 时 stderr 每行一个对象：

```json
{"code":"usage","message":"...","exit_code":2}
{"code":"eval","message":"...","exit_code":1,"stage":"jq","path":"a.json","line":7,"record":3}
```

`code` / `message` / `exit_code` 恒存在；`eval` 追加 `stage` / `path` / `line` / `record`，缺失的 `path` / `line` 显式为 `null` 而非缺键。字段只追加不重命名。文本模式保持 `ux: ...` 单行。

| `code` | 含义 | 退出码 |
|---|---|---|
| `query` | 管道语法 / 阶段编译失败（含未绑定 XPath 前缀） | 2 |
| `usage` | 参数非法或互斥 | 2 |
| `eval` | 求值阶段失败 | 1 |
| `runtime` | 运行期错误（含收敛后的 panic） | 1 |
| `limit_exceeded` | `--max-results` 触顶 | 1 |
| `io` | IO 错误 | 1 |
| `warning` | 非致命告警 | 0 |

退出码：`0` 成功（含零匹配）、`1` 运行期错误、`2` 参数/编译错误。`--max-results` 触顶时 `-O json` 不会吐残缺数组。

## 稳定保证与当前行为

跨版本稳定：stdout 只承载数据；stderr 每条诊断一行；`--quiet` 只压**非结果影响** warning；错误必出；0/1/2 退出码；字典序串行；`--no-stdin` 与 TTY 保护；`catch_unwind` 无条件把主线程 unwind panic 收敛为 `runtime(1)`，JSON 模式额外安装空 panic hook 保证 stderr 纯净。

0.4.0 行为：BrokenPipe 按 **I/O 方向**区分——**输出侧** `AppError::Io(BrokenPipe)` 退出 0 且不写诊断（正常终止）；**输入侧**读失败（含上游管道破裂）为 `runtime(1)` 诊断。`rc=0` 且 stdout 空且 stderr 空现在**只表示「真的零匹配」**，不再可能是「输入侧管道破裂被静默判成功」。

## `rc=0` 且 stdout 为空的决策表

| stderr | 含义 | 动作 |
|---|---|---|
| 空 | 真的零匹配 | 复核表达式与 `-f` |
| `no input files matched` | glob / 过滤没命中 | `ux --list` 复核输入集 |
| `binary input skipped` / `incompatible` | 文件被跳过 | 加 `--force-text` 或调整首阶段 |
| `empty file (no records)` | 0 字节文件已打开但不产生记录（空 stdin 不警告） | 对账时记为「空」而非「缺失」 |
| `assumed GB18030` | 编码回退 | 显式 `--encoding` |
| `processed N file(s), 0 results` | 打开 ≥1 个文件但产物 0 条（无错误） | 这是零匹配的可见信号；复核表达式 / `-f` |

结果影响型 warning（`no input files matched` / `binary input skipped` / `incompatible` / `empty file` / `processed N file(s), 0 results`）**不受 `--quiet` 压制**，JSON 带 `"category":"result_set"`——依赖退出码时也可放心加 `--quiet`。

## 常见陷阱

- `--help` / `--version` / `--completions` 的 stdout 是给人或 shell 看的，不是数据。
- 非 strict 模式下单文件出错仍会继续处理其余文件，`rc=1` 且已产出的 jsonl/text 结果保留；`-O json` 失败时不输出部分聚合。
- HTML 容错树会插入隐式 `<tbody>` 并折叠标签/属性名为小写，`//table/tr`、`//A` 可能静默零匹配。
- CSV 字段一律字符串，数值比较前用 `jq(.n |= tonumber)`。
- `regex` crate 无回溯引用、无 lookaround。
- Windows Git Bash / MSYS2 会把 `-e` 里的 `//a`、`//b:book` 当 UNC 路径改写。执行前设 `MSYS2_ARG_CONV_EXCL='*'` 或改用 PowerShell。
- `--skip-incompatible` 只判断首阶段与源文件类型，阶段链中段的类型不匹配仍是错误。
- 默认 64 KiB 输出缓冲；长驻管道（如 `tail -f`）务必加 `--line-buffered`。

## 能力边界

不做网络访问、并发、稳定库 API、完整 jq / XPath 1.0 兼容、schema 校验、join、跨文件关联、结果排序、提取提示（hint）。captures 只读元数据，需 `captures()` 阶段显式注入。库直调 `run()` 仅用于内部分层与测试，不承诺 API 稳定性。
