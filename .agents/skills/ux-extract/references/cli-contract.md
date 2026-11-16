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
| `--list`, `--dry-run` | 只输出输入集 JSONL，不读文件内容 |
| `--completions SHELL` | bash / zsh / fish / powershell / elvish |
| `--error-format` | `text`（默认）\| `json` |
| `--quiet`, `--silent` | 只压 warning，错误照常输出 |
| `--no-stdin` | 完全拒绝 stdin（隐式或 `-`），CI / agent 必加 |
| `--with-source` | 输出 `{value, source:{path,line,record,captures}}`；与 `-O text` 互斥 |
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

## 7 个阶段

| 阶段 | 接受 | 输出 |
|---|---|---|
| `jq(FILTER)` | Data / Text | JSON 字符串回落 Text，其余 Data |
| `xpath(EXPR)` | Node | nodeset 展开为 Node；标量结果为 Text/Data |
| `re(PATTERN)` | Text | 无捕获组输出完整匹配 Text；有捕获组输出含 `0`/`1`/命名键的 Data 对象 |
| `text()` | 任意 | string-value / 原样 / 紧凑 JSON → Text |
| `attr(NAME)` | 元素或属性 Node | 属性值 Text，缺失则 0 条 |
| `split(PATTERN)` | Text | 按正则切分的 Text 片段，空片段丢弃、不 trim |
| `slurp()` | 任意 | 全部输入收敛为一个 Data 数组，至多一个，跨文件全局 |

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

跨版本稳定：stdout 只承载数据；stderr 每条诊断一行；`--quiet` 只压 warning；错误必出；0/1/2 退出码；字典序串行；`--no-stdin` 与 TTY 保护；`catch_unwind` 无条件把主线程 unwind panic 收敛为 `runtime(1)`，JSON 模式额外安装空 panic hook 保证 stderr 纯净。

0.3.x 当前行为（0.4.0 会变更）：`AppError::Io(BrokenPipe)` 一律退出 0 且不写诊断，`Runtime(String)` 包裹的读错误为 `runtime(1)`。**输入侧**首次 `fill_buf` 与逐行 `read_until` 产生的 BrokenPipe 同样返回 0，因此 `rc=0` + stdout 空 + stderr 空**不能证明输入被完整读取**；需要强保证时改用文件输入。

## `rc=0` 且 stdout 为空的决策表

| stderr | 含义 | 动作 |
|---|---|---|
| 空 | 真的零匹配，或输入侧管道破裂被静默判为成功 | 复核表达式与 `-f`；若输入来自管道，改用文件输入重跑 |
| `no input files matched` | glob / 过滤没命中 | `ux --list` 复核输入集 |
| `binary input skipped` / `incompatible` | 文件被跳过 | 加 `--force-text` 或调整首阶段 |
| `assumed GB18030` | 编码回退 | 显式 `--encoding` |

`--quiet` 会隐藏这些 warning。需要区分「零匹配」与「文件被跳过」时不要加；管道输入场景不要依赖 `--quiet` + 退出码判断完整性。

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

不做网络访问、并发、稳定库 API、完整 jq / XPath 1.0 兼容、schema 校验、join、跨文件关联、结果排序、提取提示（hint）、captures 求值注入。库直调 `run()` 仅用于内部分层与测试，不承诺 API 稳定性。
