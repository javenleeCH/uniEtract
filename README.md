# ux — UniExtract

把 jq、XPath、正则放进同一条记录流的 Rust CLI。不依赖外部 `jq` 或 shell，运行时无需外部 C 库（pest 的传递依赖只在构建期编译少量平台栈辅助代码）。

## 安装

```powershell
cargo build --release            # 产物：target/release/ux
cargo install --path .           # 安装到 ~/.cargo/bin/ux
ux --version                     # ux 0.3.0
ux --completions powershell      # 生成补全脚本到 stdout
```

需要 Rust 1.85+（edition 2024）。MIT 许可见 `LICENSE`。Shell 补全支持 bash / zsh / fish / powershell / elvish。

## 30 秒上手

```powershell
'{"id":1}', '{"id":2}' | ux -f jsonl -O text -e 'jq(.id)'   # 1 / 2，逐行输出
ux --list .\sample\basic\                                    # 先看清将处理哪些文件
ux --help                                                    # 阶段速查 + Agent 契约 + 示例
```

## 快速开始

```powershell
# JSON → 正则命名捕获 → 重整形
ux -e 'jq(.items[].msg) | re(r"user=(?<id>\d+)") | jq({id})' a.json

# HTML 链接 → 属性 → 正则 → 取值
ux -e 'xpath("//a") | attr("href") | re(r"^https://(?<host>[^/]+)") | jq(.host)' page.html

# 日志逐行流式抽取
Get-Content app.log -Encoding utf8 | ux -f text -O text -e 're(r"ERROR: (?<message>.*)") | jq(.message)'

# JSONL 全局聚合（放弃流式，内存与记录数成正比）
ux -f jsonl -O text -e 'slurp() | jq(length)' jobs.jsonl more.jsonl

# raw 文本切分
ux -f text --text-mode raw -e 'split(r"\r?\n----------------\r?\n") | re(r"(?<id>\d{6})")' report.txt

# 目录递归 + 模糊过滤 + 跳过不兼容文件
ux --fuzzy prodconfig --skip-incompatible -e 'jq(.database.host)' .

# Agent：先确认输入集，再静默抽取并拿结构化错误
ux --list .\logs\
ux --quiet --error-format json -f jsonl -e 'jq(select(.level == "ERROR"))' .\logs\
```

## 流水线阶段

| 阶段 | 接受 | 输出 |
|---|---|---|
| `jq(FILTER)` | Data / Text | JSON 字符串回落 Text，其余 Data |
| `xpath(EXPR)` | Node | nodeset 展开为 Node；字符串/数字/布尔为 Text/Data |
| `re(PATTERN)` | Text | 无捕获组：完整匹配 Text；有捕获组：含 `"0"`/`"1"`/命名键的 Data 对象 |
| `text()` | 任意 | string-value / 原样 / 紧凑 JSON → Text |
| `attr(NAME)` | 元素或属性 Node | 属性值 Text（缺失则 0 条） |
| `split(PATTERN)` | Text | 按正则切分的 Text 片段，空片段丢弃、不 trim |
| `slurp()` | 任意 | 所有输入收敛为一个 Data 数组；至多一个，跨文件全局 |

参数字符串支持 `r"..."` / `r#"..."#` 原始字符串（任意井号数）与 `\n \r \t \\ \' \"` 六种转义。管道符只在括号深度为 0 且不在字符串内时切分，所以 jq 自身的 `.a | .b` 不会被误拆。没有「字面 `|`」转义。

DSL 结构由 `pest` 的 PEG 语法（`src/dsl.pest`）描述，`src/dsl.rs` 负责阶段名与字面量解码；阶段名以 Rust 的 `StageSpec` 为唯一事实来源。

编译期会做**类型上界检查**：`slurp() | xpath(...)` 这类确定不可能的组合会在读取输入前报错（退出 2）。运行期仍保留值级类型检查，因为 `jq` 的输出类型取决于值。

## 输入与输出

- 格式：JSON、JSONL/NDJSON、YAML（多文档）、TOML、XML、HTML、text/log、CSV/TSV。`-f auto` 先按扩展名，未知扩展名只保守嗅探 JSON 与 HTML/XML，其余按文本；YAML/TOML/CSV/JSONL 请显式指定 `-f`。
- 编码：`--encoding utf-8|utf-16le|utf-16be|gbk|gb18030|auto`。BOM 会被剥离；`auto` 回退 GB18030 时会在 stderr 说明。UTF-16 不会被误判为二进制。转码前的原始字节也受 `--max-input-size` 约束，解码后不再二次套用同一上限。
- 路径：文件、目录（递归）、glob、`-`（stdin）。目录默认忽略 `.git`/`target`/`node_modules` 等；`--exclude GLOB`、`--no-default-ignore`、`--max-depth` 控制范围；`--fuzzy QUERY` 是后置过滤，不做目录剪枝。`--list`（别名 `--dry-run`）只输出待处理清单而不读文件；`--no-stdin` 显式拒绝 stdin。输出顺序按相对路径字典序，处理永远串行。
- 输出：`-O jsonl`（默认，每行紧凑 JSON）、`-O text`（原样文本）、`-O json`（聚合成数组，默认缩进 2 空格，`--compact` 关闭）。换行恒为 LF。
- `--with-source` 包成 `{value, source:{path,line,record}}`；与 `-O text` 互斥。
- 体积：`--max-input-size`（整文档格式）、`--max-record-size`（单行/单记录/切分片段）。`0` 表示不限。
- 结果：`--max-results N` 在第 N+1 条时报错退出 1，立即终止且不输出残缺聚合，不静默截断。
- 诊断：`--error-format text|json`（默认 text）控制 stderr 编码；`--quiet`（别名 `--silent`）压制 warning，错误始终输出。

退出码：`0` 成功（含零匹配）、`1` 输入/解析/求值/输出错误、`2` 参数或编译错误。`BrokenPipe` 视为正常终止。详细字段见下节。

## 面向开发者与 Agent 的稳定契约

CLI 会被脚本、CI 与 LLM 生成的命令直接调用，因此下列行为是**稳定契约**：

| 关注点 | 保证 |
|---|---|
| stdout | 只承载数据；`-O jsonl` 每行一个紧凑 JSON，`-O json` 单个数组，`-O text` 原样文本；`--list` 输出输入清单 JSONL |
| stderr | 每条诊断一行；`--error-format json` 时每行都是可解析的 JSON 对象；`--quiet` 压制 warning 但错误必出 |
| 退出码 | `0` 成功（含零匹配）、`1` 运行期错误、`2` 参数/编译错误；`BrokenPipe` 为 0；意外 panic 也收敛为 `1` 的结构化 `runtime` 诊断，不会出现 rc=101 |
| 顺序 | 文件按归一化相对路径字典序串行处理；`--list` 顺序与实际处理顺序一致，多次调用字节级稳定 |
| 输入 | 无路径默认读 stdin；stdin 是终端且未显式给 `-` 时返回 `Usage(2)` 并提示，避免自动化进程挂起；`--no-stdin` 可彻底禁用 |

结构化诊断示例（stderr，每行一个对象）：

```json
{"code":"usage","exit_code":2,"message":"--with-source and -O text are mutually exclusive; ..."}
{"code":"eval","exit_code":1,"message":"<stdin> (record 1): stage `jq`: boom","line":null,"path":null,"record":1,"stage":"jq"}
{"code":"limit_exceeded","exit_code":1,"message":"--max-results (1) exceeded: refusing to silently truncate output"}
{"code":"warning","exit_code":0,"message":"no input files matched"}
```

`code` 取值：`query` / `usage` / `eval` / `runtime` / `limit_exceeded` / `io` / `warning`。前六类对应失败，`warning` 不表示失败；`eval` 额外提供 `stage` / `path` / `line` / `record`。字段只会追加，不会重命名。文本模式保持历史的 `ux: ...` 单行格式，脚本可按需切换。

推荐的 Agent 调用姿势：

```powershell
# 1. 确认输入集：不需要 -e，不打开文件内容
ux --list .\logs\

# 2. 静默 warning + 结构化错误，stdout 可直接按 JSONL 解析
ux --quiet --error-format json -f jsonl -e 'jq(select(.level == "ERROR"))' .\logs\

# 3. 依据退出码与 stderr 的 code 决定重试（query/usage 改参数，runtime/io 查数据）
```

`rc=0` 且 stdout 为空**不等于抽取失败**，按 stderr 与上下文判断：

| stderr | 含义 | 建议动作 |
|---|---|---|
| 空 | 真的零匹配 | 复核表达式与输入格式（显式 `-f jsonl`、HTML 隐式 `<tbody>` 等） |
| `no input files matched` | glob / 过滤没命中 | 用 `ux --list` 复核输入集 |
| `binary input skipped` / `incompatible` | 文件被跳过 | 按需加 `--force-text` 或调整首阶段 |
| `assumed GB18030` | 编码回退 | 显式 `--encoding` 更稳 |

`--quiet` 会隐藏这些 warning：只关心数据且依赖退出码时可加；需要区分「零匹配」与「文件被跳过」时不要加。

容易误判的点：

- `--help` / `--version` / `--completions` 的 stdout 是给人或 shell 看的，不是数据。
- `-O json` 与 `slurp()` 会在写第一个字节前缓存全部结果；大输入用 `-O jsonl`。`-O json` 叠加 `slurp()` 时，数组元素本身也是数组（双层）。
- `--max-results` 触顶是错误（退出 1），不是静默截断；此时 `-O json` 不会输出残缺数组。
- 非 strict 模式下单文件出错仍会继续处理其余文件，`rc=1` 且前面已产出的 jsonl/text 结果保留；`-O json` 失败时不输出部分聚合数组。
- XPath 表达式里的前缀必须用 `--ns PREFIX=URI` 绑定；**URI 会与文档命名空间 URI 匹配**，表达式前缀可与文档前缀不同。未绑定前缀现在是编译期 `Query(2)`，不会崩溃。
- HTML 由 `html5ever` 容错解析：会插入隐式 `<tbody>`，并把元素/属性名折叠为小写，`//table/tr`、`//A` 可能静默零匹配。
- Windows Git Bash / MSYS2 会把 `//a` 改写成 UNC 路径，见「已知限制」。

## 流式与缓冲

默认使用 64 KiB 输出缓冲，在 `finish` 或缓冲满时刷新。stdout 是终端时自动逐条刷新；管道场景若需要在 EOF 前看到输出，显式加 `--line-buffered`。`-O json` 与 `slurp()` 本身放弃流式。长驻管道（如 `tail -f`）务必显式加 `--line-buffered`，否则会被误判为「无输出」。

```powershell
tail -f app.log | ux --line-buffered -f text -O text -e 're(r"ERROR: (?<m>.*)") | jq(.m)'
```

## 样本与冒烟测试

`sample/` 是可直接喂给 CLI 的固定夹具，不依赖网络或临时目录：

| 目录 | 内容 |
|---|---|
| `basic/` | 9 种输入格式各一份标准样本（JSON、JSONL、YAML、TOML、XML、HTML、CSV、TSV、text/log） |
| `edge/` | BOM、GB18030、UTF-16LE/BE、CRLF、空文件、空行 Origin、大整数、多文档 YAML、CSV 引号、坏 XML、二进制、超长行、无扩展名、`split()`、HTML 容错树形、XML 命名空间 |
| `discover/` | 目录排序、默认忽略目录剪枝（`node_modules/`、`target/`）、glob、fuzzy |

```powershell
python sample/generate.py            # 重建全部夹具（含非 UTF-8 与二进制文件）
python sample/smoke.py               # 对 release 二进制跑 45 条断言
python sample/smoke.py --only encoding
python sample/smoke.py --ux .\target\debug\ux.exe
python sample/export_cases.py        # 重建人工抽样检查清单 sample/CASES.md
python sample/export_cases.py --check
```

`sample/CASES.md` 是同源自 `smoke.py` 生成的**人工抽样检查清单**：45 条用例的可复制命令、检查点、预期退出码 / stdout / stderr，以及 41 个夹具的原文与字节特征。人工抽样时按分组勾选即可；CI 用 `--check` 保证清单不与断言漂移。

`smoke.py` 通过 subprocess 直接调用 `ux`（不经过 shell），因此不受 MSYS2 的 `//a` 路径转换影响；CI 在 `cargo build --release` 之后自动运行它。`CASES.md` 里的 bash 命令在 Git Bash 下执行前需 `export MSYS2_ARG_CONV_EXCL='*'`，清单内已注明。

## 已知限制

- `regex` crate 无回溯引用、无 lookaround。
- CSV 字段一律字符串，数值比较前先 `jq(.n |= tonumber)`。
- XPath 1.0 不承诺完整兼容；只保证实测过的函数与轴。
- `slurp` 的 `--with-source` 只保留第一条来源。
- `-f auto` 会把无扩展名的 JSONL 误判为 JSON，需显式 `-f jsonl`。
- `--skip-incompatible` 只判断首阶段与源文件类型；阶段链中段的类型不匹配仍是错误。
- HTML 容错树会插入隐式 `<tbody>` 并折叠标签/属性名为小写，表达式需按实际 DOM 写。
- Windows Git Bash / MSYS2 会把 `-e` 里的 `//a`、`//b:book` 当 UNC 路径改写，导致 XPath 静默零匹配。执行前设 `MSYS2_ARG_CONV_EXCL='*'`，或改用 PowerShell。这是 shell 的参数转换，不是 `ux` 的解析问题。
- DSL 括号嵌套最多 128 层，超限时报错退出 2（不崩溃、不静默截断）；限制只针对嵌套深度，不影响长正则或长 jq filter。
- `--list` 已覆盖「先确认输入集」；更细的 `--path-spec` / `--sort` / `--max-files` 仍属未实现提案。manpage 未随发行提供，可用 `--help` 与 `--completions` 替代。

完整契约、ADR 与实施计划见 `.trae/documents/统一信息抽取器-cli-设计.md`（v3）；其中的 `--ns` 结论以本 README 与 `--help` 为准（URI 参与匹配）。模块拆分（`locate` / `extract`）与路径匹配能力的设计提案见 `.trae/documents/路径匹配模块-设计.md`（v1 提案，**未实现**）。变更记录见 `CHANGELOG.md`。
