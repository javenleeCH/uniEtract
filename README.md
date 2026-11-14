# ux — UniExtract

把 jq、XPath、正则放进同一条记录流的 Rust CLI。不依赖外部 `jq` 或 shell，运行时无需外部 C 库（pest 的传递依赖只在构建期编译少量平台栈辅助代码）。

```powershell
cargo build --release
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
- 编码：`--encoding utf-8|utf-16le|utf-16be|gbk|gb18030|auto`。BOM 会被剥离；`auto` 回退 GB18030 时会在 stderr 说明。UTF-16 不会被误判为二进制。
- 路径：文件、目录（递归）、glob、`-`（stdin）。目录默认忽略 `.git`/`target`/`node_modules` 等；`--exclude GLOB`、`--no-default-ignore`、`--max-depth` 控制范围；`--fuzzy QUERY` 是后置过滤。输出顺序按相对路径字典序，处理永远串行。
- 输出：`-O jsonl`（默认，每行紧凑 JSON）、`-O text`（原样文本）、`-O json`（聚合成数组，默认缩进 2 空格，`--compact` 关闭）。换行恒为 LF。
- `--with-source` 包成 `{value, source:{path,line,record}}`；与 `-O text` 互斥。
- 体积：`--max-input-size`（整文档格式）、`--max-record-size`（单行/单记录/切分片段）。`0` 表示不限。
- 结果：`--max-results N` 在第 N+1 条时报错退出 1，不静默截断。

退出码：`0` 成功（含零匹配）、`1` 输入/解析/求值/输出错误、`2` 参数或编译错误。`BrokenPipe` 视为正常终止。

## 流式与缓冲

默认使用 64 KiB 输出缓冲，在 `finish` 或缓冲满时刷新。stdout 是终端时自动逐条刷新；管道场景若需要在 EOF 前看到输出，显式加 `--line-buffered`。`-O json` 与 `slurp()` 本身放弃流式。

```powershell
tail -f app.log | ux --line-buffered -f text -O text -e 're(r"ERROR: (?<m>.*)") | jq(.m)'
```

## 已知限制

- `--ns` 只解析 XPath 表达式前缀，**不**按 namespace URI 匹配；默认命名空间或不同前缀的等价文档匹配不上。
- `regex` crate 无回溯引用、无 lookaround。
- CSV 字段一律字符串，数值比较前先 `jq(.n |= tonumber)`。
- XPath 1.0 不承诺完整兼容；只保证实测过的函数与轴。
- `slurp` 的 `--with-source` 只保留第一条来源。
- `-f auto` 会把无扩展名的 JSONL 误判为 JSON，需显式 `-f jsonl`。
- `--skip-incompatible` 只判断首阶段与源文件类型；阶段链中段的类型不匹配仍是错误。
- Windows Git Bash / MSYS2 会把 `-e` 里的 `//a`、`//b:book` 当 UNC 路径改写，导致 XPath 静默零匹配。执行前设 `MSYS2_ARG_CONV_EXCL='*'`，或改用 PowerShell。这是 shell 的参数转换，不是 `ux` 的解析问题。
- DSL 括号嵌套最多 128 层，超限时报错退出 2（不崩溃、不静默截断）；限制只针对嵌套深度，不影响长正则或长 jq filter。

完整契约、ADR 与实施计划见 `.trae/documents/统一信息抽取器-cli-设计.md`（v3）。变更记录见 `CHANGELOG.md`。
