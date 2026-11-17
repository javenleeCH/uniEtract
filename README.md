# ux — UniExtract

把 jq、XPath、正则放进同一条记录流的 Rust CLI。不依赖外部 `jq` 或 shell，运行时无需外部 C 库（pest 的传递依赖只在构建期编译少量平台栈辅助代码）。

## 安装

```powershell
cargo build --release            # 产物：target/release/ux
cargo install --path .           # 安装到 ~/.cargo/bin/ux
ux --version                     # ux 0.3.1
ux --completions powershell      # 生成补全脚本到 stdout
```

需要 Rust 1.88+（edition 2024，MSRV 由 CI 的 1.88 job 与 `cargo build --locked` 双重把关）。MIT 许可见 `LICENSE`。Shell 补全支持 bash / zsh / fish / powershell / elvish。也可从 GitHub Releases 下载对应平台的预编译包（含 `ux`、`README.md`、`LICENSE`、`CHANGELOG.md` 与 `docs/`），校验 `SHA256SUMS` 后解压即用。

当前版本 `0.3.1`：`Cargo.toml`、`Cargo.lock`、`ux --version` 与 `CHANGELOG.md` 保持一致，由 `v0.3.1` tag 经 GitHub Actions 自动发版。变更记录见 `CHANGELOG.md`，版本路线见「0.4.0 时间表」。

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
| `captures()` | 任意 | `--path-spec` captures 对象（显式 opt-in；无 path-spec 时为 null）→ Data |

参数字符串支持 `r"..."` / `r#"..."#` 原始字符串（任意井号数）与 `\n \r \t \\ \' \"` 六种转义。管道符只在括号深度为 0 且不在字符串内时切分，所以 jq 自身的 `.a | .b` 不会被误拆。没有「字面 `|`」转义。

DSL 结构由 `pest` 的 PEG 语法（`src/dsl.pest`）描述，`src/dsl.rs` 负责阶段名与字面量解码；阶段名以 Rust 的 `StageSpec` 为唯一事实来源。

编译期会做**类型上界检查**：`slurp() | xpath(...)` 这类确定不可能的组合会在读取输入前报错（退出 2）。运行期仍保留值级类型检查，因为 `jq` 的输出类型取决于值。

## 输入与输出

- 格式：JSON、JSONL/NDJSON、YAML（多文档）、TOML、XML、HTML、text/log、CSV/TSV。`-f auto` 先按扩展名，未知扩展名只保守嗅探 JSON 与 HTML/XML，其余按文本；YAML/TOML/CSV/JSONL 请显式指定 `-f`。
- 编码：`--encoding utf-8|utf-16le|utf-16be|gbk|gb18030|auto`。BOM 会被剥离；`auto` 回退 GB18030 时会在 stderr 说明。UTF-16 不会被误判为二进制。转码前的原始字节也受 `--max-input-size` 约束，解码后不再二次套用同一上限。
- 路径：文件、目录（递归）、glob、`-`（stdin）。目录默认忽略 `.git`/`target`/`node_modules` 等；`--exclude GLOB`、`--no-default-ignore`、`--max-depth` 控制范围；`--fuzzy QUERY` 是后置过滤，不做目录剪枝。`--list`（别名 `--dry-run`）只输出待处理清单而不读文件；`--no-stdin` 显式拒绝 stdin。输出顺序按相对路径字典序，处理永远串行。`--list` 清单每行带 `type` / `path`（及 `--path-spec` 的 `captures`），并追加 `format`（仅当格式可从 `-f` 或扩展名确定；内容无关提示，未知扩展名省略该字段）。
- 输出：`-O jsonl`（默认，每行紧凑 JSON）、`-O text`（原样文本）、`-O json`（聚合成数组，默认缩进 2 空格，`--compact` 关闭）。换行恒为 LF。
- `--with-source` 包成 `{value, source:{path,line,record}}`；与 `-O text` 互斥。`source.path` 与 `--list` 的 `path` 一致使用 `/` 分隔符（Windows 上也如此），两份输出可直接对账。
- 体积：`--max-input-size`（整文档格式）、`--max-record-size`（单行/单记录/切分片段）。`0` 表示不限。
- 结果：`--max-results N` 在第 N+1 条时报错退出 1，立即终止且不输出残缺聚合，不静默截断。
- 诊断：`--error-format text|json`（默认 text）控制 stderr 编码；`--quiet`（别名 `--silent`）压制 warning，错误始终输出。

### `--path-spec`：结构化路径匹配

按**路径结构**选择输入，而不是先递归全部文件再按内容过滤。spec 以当前工作目录为锚，`/` 分段；段内只允许两种显式占位：

| 写法 | 含义 |
|---|---|
| `{name:re:...}` | Rust regex 片段，产生命名捕获 `name`；片段内层的命名组 `(?<sub>...)` 也是合法捕获，会一并进 `captures` |
| `{name:glob:*.txt}` | `*` / `?` 段内通配（不跨 `/`），产生命名捕获 `name` |

- 0.3.1 起**不接受** `time:` / `lit:` / 简写 `{%Y-%m-%d}` / `{\d+}`，也**不做日历或时钟合法性校验**；有效性由调用方写进 `re:` 片段。非法 spec 在打开任何目录之前返回 `Usage(2)`。
- 命名 captures 是**只读字符串元数据**：出现在 `--list` 与 `--with-source` 的 `source.captures` 里，默认**不参与流水线求值**，也不生成 `ts_fields` 之类派生字段。0.4.0 起可用 `captures()` 阶段显式把它们注入记录流（opt-in）。
- 目录结尾的 spec 不会隐式追加 `**`；必须显式写出文件段，例如 `/{file:glob:*.txt}`。
- 与位置参数 `PATH|GLOB` 互斥；可与 `--exclude`、`--fuzzy`（后置过滤）、`--no-default-ignore` 组合。段级剪枝：不匹配的中间目录根本不会被进入。
- 发现阶段三类异常（`read_dir` 打开失败、`ReadDir` 单条目失败、`file_type` 失败）会输出 warning 而不是静默跳过；`--quiet` 可压制，因此**需要区分零匹配与「丢文件」时不要加 `--quiet`**。
- `glob` 占位不支持字符类（`[...]`，含字面 `]`），需要时改用 `{name:re:...}`；`re:` 片段禁止顶层 `|` 与不配平括号，避免悄悄逃逸捕获组、让段级剪枝静默失效。
- captures 在 JSON 对象里按名字**字典序**输出（不是 spec 中的出现顺序）；同名捕获（含嵌套组与显式占位跨段重名）在编译期拒绝；嵌套命名组计入 32 个捕获的预算。
- `re:` 片段里可以写嵌套命名组拆文件名字段：`{file:re:.*(?<ts>\d{8}_\d{6})_(?<seq>\d+)_hidumper\.txt}` 会产出 `file` / `ts` / `seq` 三个捕获；片段内的冒号（如 `\d{2}:\d{2}`）合法。
- 转义只支持 `\{` / `\}` / `\\` 三种；其他 `\X` 保留为字面两字符。

### 输入选择：选型决策表

四种文件选择方式并行存在，按需求取一种：

| 入口 | 适用场景 | 剪枝 | 命名捕获 | 备注 |
|---|---|---|---|---|
| 位置参数 `PATH` / 目录 / `GLOB` | 名称通配、递归整目录 | 目录递归时默认忽略表 | 无 | 最通用；`--fuzzy`/`--exclude` 做后置过滤 |
| `--path-spec` | 路径**结构**带语义（时间戳目录/服务名离散），要段级剪枝 | 段级（不匹配目录不进入） | 有（`re:`/`glob:`） | 与位置参数互斥；目录尾不追加 `**` |
| `--path-regex REGEX` | 需要**跨段/全路径**正则过滤，不要结构化捕获 | 无（后置过滤） | 无 | 可重复，全匹配 AND；与 `--exclude`/`--fuzzy` 组合 |
| `--root DIR` + 相对位置参数 | 把发现/报告锚定到非当前目录 | 随位置参数语义 | 无 | 绝对位置参数不受锚定影响 |

`--path-spec` 优先于 `--path-regex`：做结构化段匹配时用前者（有剪枝与 captures）；做轻量正则后置过滤时用后者。

```powershell
ux --path-spec 'data/{ts:re:\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}}_SRRG/render_service/{mins:re:\d+}min/hidumper/txt/{file:glob:*.txt}' --list
```

退出码：`0` 成功（含零匹配）、`1` 输入/解析/求值/输出错误、`2` 参数或编译错误。判定按 **I/O 方向**而非错误变体：**输出侧** `AppError::Io(BrokenPipe)`（例如 `| head` 提前关闭）视为正常终止并退出 0（不写诊断）；**输入侧**读失败——含上游管道破裂——一律 `runtime(1)` 并输出诊断。0.4.0 起输入侧不再被静默判成功。详细字段见下节。

## 面向开发者与 Agent 的稳定契约

CLI 会被脚本、CI 与 LLM 生成的命令直接调用。下列行为是**跨版本稳定保证**：

| 关注点 | 保证 |
|---|---|
| stdout | 只承载数据；`-O jsonl` 每行一个紧凑 JSON，`-O json` 单个数组，`-O text` 原样文本；`--list` 输出输入清单 JSONL |
| stderr | 每条诊断一行；`--error-format json` 时每行都是可解析的 JSON 对象；`--quiet` 压制 warning 但错误必出 |
| 退出码 | `0` 成功（含零匹配）、`1` 运行期错误、`2` 参数/编译错误；`Runtime(String)` 包裹的读错误为 1。`catch_unwind` **无条件**把主线程 unwind panic 收敛为 `1` 的结构化 `runtime` 诊断；`--error-format json` 模式额外安装空 panic hook，保证 stderr 只有结构化诊断。库直调 `run()` 同样收敛，但不继承该进程级 hook，调用方需自行管理 |
| 顺序 | 文件按归一化相对路径字典序串行处理；`--list` 顺序与实际处理顺序一致，多次调用字节级稳定 |
| 输入 | 无路径默认读 stdin；stdin 是终端且未显式给 `-` 时返回 `Usage(2)` 并提示，避免自动化进程挂起；`--no-stdin` 可彻底禁用 |

以下条目是**当前版本行为（0.4.0 已定型）**：

| 关注点 | 行为 |
|---|---|
| BrokenPipe | 按 I/O 方向区分：输出侧 `AppError::Io(BrokenPipe)` 为 0 且不写诊断（正常终止）；输入侧读失败为 `runtime(1)` 并输出诊断 |

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
| `empty file (no records)` | 文件被打开但零字节，不产生任何记录（空 stdin 不警告） | 与 `--list` 清单对账时把该文件记为「空」而非「缺失」 |
| `assumed GB18030` | 编码回退 | 显式 `--encoding` 更稳 |
| `processed N file(s), 0 results` | 打开了 ≥1 个文件（无错误）但产物 0 条 | 与「真零匹配」区分：复核表达式 / `-f`；这本身就是零匹配的信号 |

`--quiet` 会隐藏这些 warning：只关心数据且依赖退出码时可加；需要区分「零匹配」与「文件被跳过」时不要加。输入侧管道破裂现在已是 `runtime(1)` 诊断（见「稳定契约」表），不再以「空 stderr + 空 stdout」的形态出现。

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

CI 固定 Rust 1.88（含 rustfmt / clippy）。本机若是更新的 stable，某些 lint 只在 1.88 报（例如 `clippy::uninlined_format_args`），因此提交前用同一工具链复现门禁：

```powershell
cargo +1.88.0 fmt --check
cargo +1.88.0 clippy --all-targets --locked -- -D warnings
cargo +1.88.0 test --all-targets --locked
```

## 发布流程

版本由 tag 驱动的 GitHub Actions（`.github/workflows/release.yml`）发布，不需要本地上传二进制：

```powershell
python .github/scripts/check_release_version.py   # tag / manifest / 文档 / 二进制一致性
git tag v0.3.1
git push origin v0.3.1
git push github v0.3.1                            # 触发 Release workflow
```

推送 `v*` tag 时（也可用 `workflow_dispatch` 指定 tag 重跑），workflow 用 Rust 1.88 在 Linux x86_64、Windows x86_64、macOS x86_64 / arm64 四个 runner 上执行 `cargo build --release --locked --target ...`；`.github/scripts/check_release_version.py` 先校验 `Cargo.toml`、`Cargo.lock`、`fuzz/Cargo.lock`、`CHANGELOG.md`、`README.md`、`docs/cli-design.md` 与 `ux --version` 全部对齐，随后 `.github/scripts/package_release.py` 打包为 `ux-<version>-<target>.tar.gz` / `.zip`。包内含 `ux`（Windows 为 `ux.exe`）、`README.md`、`LICENSE`、`CHANGELOG.md` 与 `docs/`。发布 job 会重算每个包的哈希、生成 `SHA256SUMS`、核对四个平台产物齐全，再把 CHANGELOG 中该版本的说明作为 Release 正文一并发布。

下载后先对照 `SHA256SUMS` 中的同名条目校验压缩包哈希（Linux `sha256sum <file>`、macOS `shasum -a 256 <file>`、Windows `Get-FileHash -Algorithm SHA256 <file>`），再解压使用。同一脚本也可在本地打包：`python .github/scripts/package_release.py --version 0.3.1 --target x86_64-pc-windows-msvc --binary target/release/ux.exe --format zip`。

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
- `--path-spec` 只做段级结构匹配：不做跨段通配、不做基于 captures 的排序，也不提供 `--sort` / `--max-files` 或提取提示（hint）。`re:` 片段内嵌套命名组拆文件名字段自 0.4.0 起可用（见上文）。manpage 未随发行提供，可用 `--help` 与 `--completions` 替代。

完整契约与 ADR 见 `docs/cli-design.md`；路径匹配的段级剪枝与夹具证据见 `docs/path-spec-design.md`；变更记录与版本路线见 `CHANGELOG.md`。

## 0.4.0 时间表（已定稿）

| 项 | 决定 |
|---|---|
| `--path-regex` / `--root` | **已实现（本轮）**，与 `--path-spec` **并存**，不替换；选型决策表见下 |
| captures 求值注入 | **已实现（本轮）**：新增 `captures()` 阶段，显式 opt-in；默认仍只读 |
| `slurp()` / `-O json` | **保留，不废弃** |
| 嵌套命名组进 captures | **已实现（本轮）**：`re:` 片段内的 `(?<sub>...)` 一并产出捕获；跨段同名编译期拒绝。原计划的「子捕获 + basename 阶段」由本项覆盖 |
| 提取提示（hint） | **不做** |
| 库接口 | **不承诺稳定库 API、不发布库文档**：`pub` 项（`run` / `parse_pipeline` / `cli`）仅用于内部分层与测试，稳定契约只有 CLI |
| BrokenPipe 按输入/输出侧区分 | **已实现（本轮）**：输入侧读错输出 `runtime(1)` 诊断，不再静默返回 0；输出侧 `BrokenPipe` 退出 0 且不写诊断 |

## Agent Skill

随仓库提供 `ux-extract` skill（`.agents/skills/ux-extract/`），教 Agent 用 `ux` 做本地信息抽取：预检、`--list` 先定输入集、八阶段流水线组装、结构化诊断与退出码解读。`.agents/skills/` 是 [Agent Skills 开放规范](https://agentskills.io)约定的**跨客户端通用目录**：遵循规范的 Agent（Claude Code、Codex、Amp、CodeBuddy 等）都会扫描 `~/.agents/skills/`（用户级）与 `<project>/.agents/skills/`（项目级）。

### 下载

`ux-extract.zip` 随每个版本发布到 GitHub Releases（资产名固定不变，`SHA256SUMS` 中有对应校验和条目）。固定取最新版用 `releases/latest/download`（GitHub 自动跳到最新 release）；需要钉住版本时把 `latest/download` 换成 `download/v<版本>`：

```powershell
# PowerShell：固定取最新版
Invoke-WebRequest -Uri "https://github.com/javenleeCH/uniEtract/releases/latest/download/ux-extract.zip" -OutFile ux-extract.zip
```

```bash
# bash / macOS / Linux；或直接在浏览器打开同一 URL
curl -LO https://github.com/javenleeCH/uniEtract/releases/latest/download/ux-extract.zip

# 钉住具体版本，例如 v0.3.1
curl -LO https://github.com/javenleeCH/uniEtract/releases/download/v0.3.1/ux-extract.zip
```

下载后可按该版本 `SHA256SUMS` 中的 `ux-extract.zip` 条目校验哈希（Linux `sha256sum`、macOS `shasum -a 256`、Windows `Get-FileHash -Algorithm SHA256`）。已 clone 仓库时无需下载：直接用仓库内的 `dist/ux-extract.zip` 或 `.agents/skills/ux-extract/`。

### 安装

一键安装（需要 Python 3.8+，只用标准库、不联网、不改配置）：

```powershell
# 情况 A：拿到了 ux-extract.zip
Expand-Archive -Path .\ux-extract.zip -DestinationPath .
python ux-extract/scripts/install.py

# 情况 B：clone 了本仓库
python .agents/skills/ux-extract/scripts/install.py
```

| 用法 | 结果 |
|---|---|
| 不带参数（等价 `--user`） | `~/.agents/skills/ux-extract/`，全项目可用 |
| `--project` | `<当前目录>/.agents/skills/ux-extract/`，随仓库共享 |
| `--target DIR` | 装进任意识别 `SKILL.md` 的 Agent 原生根目录 |
| `--force` / `--uninstall` | 覆盖已装副本 / 卸载 |

Agent 只认自己原生路径时用 `--target`，例如 Claude Code：

```bash
python3 ux-extract/scripts/install.py --target ~/.claude/skills
```

无 Python 时手动安装：把 `ux-extract/` 整个目录复制到 Agent 的 skills 根目录，目录名保持 `ux-extract`。

### 验证与触发

```powershell
python ~/.agents/skills/ux-extract/scripts/ux_probe.py   # 期望 {"ok": true, ...}
```

预检确认 `ux` 在 PATH 或 `target/release/` 上、长帮助齐全、stdout/stderr 分离。`ux` 不是 skill 的安装前置，但执行抽取任务前需要它。重启或刷新 Agent 后，对它说「用 ux 从这个日志目录里抽出所有 ERROR 消息」即可触发。

完整说明见 `.agents/skills/ux-extract/INSTALL.md`（随 zip 分发）。
