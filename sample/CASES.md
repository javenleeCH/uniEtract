# sample/ 人工抽样检查清单

> 本文件由 `python sample/export_cases.py` 从 `sample/smoke.py` 自动生成，**请勿手工编辑**。
> 命令、预期退出码与预期输出直接取自自动化断言；`sample/CASES.md` 与 `sample/smoke.py` 一一对应。
> 当前共 **45 条用例**、**41 个夹具**。

## 1. 怎么用

1. 在仓库根目录构建 release 二进制：`cargo build --release`。下文命令里的 `ux` 指 `target/release/ux`（Windows 为 `ux.exe`），可写全路径或加入 PATH。
2. 从下面的总览表挑几条，勾选 `人工确认`；建议每组至少一条，错误路径用例优先。
3. 复制该用例明细里的 `bash` 命令，在仓库根目录执行，对照列出的退出码、stdout 与 stderr 要求。
4. 不想手敲命令时，用跨平台且不经 shell 的单条复现命令：`python sample/smoke.py --only <用例名>`；全量执行 `python sample/smoke.py`。

约定与坑：

- 命令按 **bash** 写法给出；Windows Git Bash 先执行 `export MSYS2_ARG_CONV_EXCL='*'`，否则 `-e` 里的 `//a`、`//b:book` 会被改写成 UNC 路径而静默零匹配。PowerShell 不做该转换，但单引号内不转义单引号，含 XPath 的表达式建议直接用 `--only` 复现。
- `stdout = 精确相等`（含尾随换行）；`stdout 包含` / `stderr 包含` 是子串断言；未列出的流不做要求。
- `rc=0` 正常结束、`rc=1` 运行期/输入错误、`rc=2` 参数或 DSL 编译期错误（P5 校验前置）。零匹配是正常结果，不算失败。
- 字节敏感夹具（BOM、CRLF、UTF-16、GB18030、二进制）在 `.gitattributes` 中标为 `binary`；重建请用 `python sample/generate.py`，不要用会改写换行/编码的编辑器保存。

## 2. 分组与抽样建议

| 分组 | 条数 | 关注点 |
|---|---:|---|
| `basic/` | 10 | 每种输入格式的标准通路，优先抽 2–3 条确认格式嗅探与输出模式。 |
| `edge/` | 25 | 输入卫生、编码、HTML 树形与命名空间；风险最高，建议重点抽样。 |
| `cli/` | 6 | 会话聚合、输出收口与参数/编译期错误契约。 |
| `discover/` | 4 | 目录发现、默认剪枝、排序与 fuzzy 过滤。 |

按风险维度建议至少抽这些编号：

- 编码与输入卫生：12–16、23–27
- 错误与边界契约：22、25、26、39、40、41（尤其 41 必须看到 rc=2 而不是进程崩溃）
- HTML 树形：28–32；命名空间：05、33–35
- 聚合与输出：36–38；文件发现确定性：42–45

## 3. 用例总览

| # | 用例 | 检查点（一句话） | 预期 | 人工确认 |
|---:|---|---|---|:---:|
| 01 | `basic/json-regex-jq` | JSON 数组展开 → 正则命名捕获 → jq 重组对象 | rc=0 · stdout 3 行 | ☐ |
| 02 | `basic/jsonl-with-source` | JSONL 每行一条记录，`--with-source` 注入 line/record/valu… | rc=0 · 含指定片段 | ☐ |
| 03 | `basic/yaml` | YAML 映射转 JSON 后 jq 取嵌套字段 | rc=0 · stdout 1 行 | ☐ |
| 04 | `basic/toml` | TOML 的 `[server]` 表被正确建模 | rc=0 · stdout 1 行 | ☐ |
| 05 | `basic/xml-ns-attr` | `--ns b=urn:books` 绑定前缀，`xpath` 选元素再 `attr` 取属性 | rc=0 · stdout 2 行 | ☐ |
| 06 | `basic/html-attr` | HTML 走 DOM 解析 | rc=0 · stdout 2 行 | ☐ |
| 07 | `basic/csv-select` | CSV 表头映射 + jq 过滤 | rc=0 · stdout 2 行 | ☐ |
| 08 | `basic/tsv` | TSV 用制表符分列，两行 name 依次为 alice、bob。 | rc=0 · stdout 2 行 | ☐ |
| 09 | `basic/text-log-regex` | `-f text` 逐行记录 + 正则捕获 + `-O text` | rc=0 · stdout 2 行 | ☐ |
| 10 | `basic/text-lines` | text 模式逐行输出且中文 UTF-8 无损 | rc=0 · stdout 3 行 | ☐ |
| 11 | `edge/text-raw-split` | `--text-mode raw` 下用 `split()` 按分隔线切块再取正则 | rc=0 · stdout 2 行 | ☐ |
| 12 | `edge/bom-utf8` | UTF-8 BOM（EF BB BF）必须被剥离，两行 JSONL 都能解析，不能把 BOM 带… | rc=0 · stdout 2 行 | ☐ |
| 13 | `edge/gbk-auto-fallback` | `--encoding auto` 猜到 GB18030 并在 stderr 给出 `assumed GB18030`… | rc=0 · stdout 2 行 · stderr 有提示 | ☐ |
| 14 | `edge/utf16le` | 显式 UTF-16LE + BOM 解码 | rc=0 · stdout 2 行 | ☐ |
| 15 | `edge/utf16be-xml-declaration` | UTF-16BE + BOM + XML 声明三者共存时仍能解析并取属性 id。 | rc=0 · stdout 1 行 | ☐ |
| 16 | `edge/crlf-stripped` | CRLF 行尾被归一化，输出两行且不含 `\r`（否则终端会出现重叠/回车痕迹）。 | rc=0 · stdout 2 行 | ☐ |
| 17 | `edge/empty-jsonl` | 0 字节文件：rc=0、无输出、无警告 | rc=0 · stdout 空 | ☐ |
| 18 | `edge/blank-lines-origin` | 空行被跳过，但 `--with-source` 的 line 保留物理行号（第 3 行仍是 re… | rc=0 · 含指定片段 | ☐ |
| 19 | `edge/big-integer` | 30 位整数必须逐位保真，不能变成浮点、科学计数法或字符串。 | rc=0 · stdout 1 行 | ☐ |
| 20 | `edge/multi-document-yaml` | 多文档 YAML 按 `---` 拆成两条独立记录，输出 alice、bob。 | rc=0 · stdout 2 行 | ☐ |
| 21 | `edge/csv-quoted-fields` | CSV 双引号转义 `""` 与含逗号字段 | rc=0 · stdout 2 行 | ☐ |
| 22 | `edge/malformed-xml` | 坏 XML 必须显式报错：rc=1、stdout 为空、stderr 含 `invalid XML`… | rc=1 · stdout 空 · stderr 有提示 | ☐ |
| 23 | `edge/invalid-utf8` | 非法 UTF-8 是输入卫生错误：rc=1、stdout 为空、stderr 含 `not valid UTF-8`… | rc=1 · stdout 空 · stderr 有提示 | ☐ |
| 24 | `edge/binary-stdin-skip` | stdin 含 NUL：跳过该输入并在 stderr 提示 `binary input skipped`… | rc=0 · stdout 空 · stderr 有提示 · stdin | ☐ |
| 25 | `edge/max-record-size` | `--max-record-size 1KiB` 拦住 4096 字节的行，rc=1 且 std… | rc=1 · stdout 空 · stderr 有提示 | ☐ |
| 26 | `edge/no-extension-auto-misdetects` | 无扩展名文件按内容自动嗅探会误判为 JSON 并报 `invalid JSON` | rc=1 · stdout 空 · stderr 有提示 | ☐ |
| 27 | `edge/no-extension-explicit-jsonl` | 与上一条对照：`-f jsonl` 显式声明后同一文件正常解析出 1、2。 | rc=0 · stdout 2 行 | ☐ |
| 28 | `edge/implicit-tbody-not-visible` | html5ever 插入隐式 `<tbody>`，因此 `//table/tr` 零匹配 | rc=0 · stdout 空 | ☐ |
| 29 | `edge/explicit-tbody-visible` | 与上一条对照：显式写 `//table/tbody/tr` 能取到 x，确认 DOM 树里确实存… | rc=0 · stdout 1 行 | ☐ |
| 30 | `edge/template-contents-in-dom` | `<template>` 内容被展开进 XPath 可见树，因此 id 为 t 和 o 的 `<p>`… | rc=0 · stdout 2 行 | ☐ |
| 31 | `edge/noscript-is-raw-text` | `<noscript>` 内容是原始文本而非元素树，里面的 p 不可见 | rc=0 · stdout 1 行 | ☐ |
| 32 | `edge/html-case-folding` | 大写 `<A HREF=...>` 折叠为小写后仍能被 `//a` 与 `attr("href")`… | rc=0 · stdout 1 行 | ☐ |
| 33 | `edge/default-ns-with-uri` | 正确 URI 绑定能匹配默认命名空间文档，输出 NS-1 | rc=0 · stdout 1 行 | ☐ |
| 34 | `edge/default-ns-wrong-uri` | 同一文档换错 URI（`urn:other`）后零匹配 | rc=0 · stdout 空 | ☐ |
| 35 | `edge/prefixed-same-uri` | 文档用 x 前缀、表达式用 b 前缀，只要 URI 相同仍能匹配，输出 PRE-1。 | rc=0 · stdout 1 行 | ☐ |
| 36 | `cli/slurp-across-files` | `slurp()` 跨两个文件聚合 3+2=5 条记录，只输出一次结果 5 | rc=0 · stdout 1 行 | ☐ |
| 37 | `cli/json-pretty` | `-O json` 把流式结果收成单个数组并 pretty 打印 | rc=0 · stdout 14 行 | ☐ |
| 38 | `cli/json-compact` | 与上一条同样的数组，`--compact` 压成一行，无多余空格。 | rc=0 · stdout 1 行 | ☐ |
| 39 | `cli/max-results-is-not-silent` | `--max-results 2` 截断后必须 rc=1 且 stderr 提到 `--max-results`… | rc=1 · stdout 2 行 · stderr 有提示 | ☐ |
| 40 | `cli/with-source-text-conflict` | `--with-source` 与 `-O text` 互斥，参数校验前置：rc=2、stdou… | rc=2 · stdout 空 · stderr 有提示 | ☐ |
| 41 | `cli/nesting-guard` | 300 层嵌套括号在编译期被 `check_nesting(128)` 拦下：rc=2、stde… | rc=2 · stdout 空 · stderr 有提示 · stdin | ☐ |
| 42 | `discover/default-ignore-prunes` | `--fuzzy noise` 递归目录时剪掉 `node_modules/`、`target/`… | rc=0 · stdout 空 | ☐ |
| 43 | `discover/fuzzy` | fuzzy 命中 `configs/prodconfig.yaml` 并读出 db.prod | rc=0 · stdout 1 行 | ☐ |
| 44 | `discover/glob-order` | glob 展开必须确定性排序（2026-01 → 2026-02），输出 10、20 | rc=0 · stdout 2 行 | ☐ |
| 45 | `discover/skip-incompatible` | `--skip-incompatible` 跳过格式不兼容的 HTML，只输出 1 且 rc=0 | rc=0 · stdout 1 行 | ☐ |

## 4. 夹具清单

`引用` 列是用例编号，标 `—` 表示未被自动化断言直接使用，适合优先人工检查。

| 夹具（相对 `sample/`） | 大小 | 特征 / 用途 | 引用 |
|---|---:|---|---|
| `basic/app.log` | 80 B | 4 行日志，其中 2 行以 `ERROR: ` 开头。 | 09 |
| `basic/books.xml` | 193 B | 显式 `b` 前缀命名空间，两本 book 各带 sku 属性。 | 05 |
| `basic/config.toml` | 58 B | `[server]` 表，port=8080。 | 04 |
| `basic/config.yaml` | 75 B | 嵌套映射 `database.host`。 | 03 |
| `basic/events.jsonl` | 70 B | 3 条记录，n=1..3、level 交替；用于输出与 slurp 聚合。 | 02, 36, 37, 38, 39 |
| `basic/jobs.csv` | 63 B | 3 行；第 3 行 error 含逗号并用双引号包裹。 | 07 |
| `basic/jobs.tsv` | 34 B | tab 分隔，3 列。 | 08 |
| `basic/notes.txt` | 42 B | 中英混合 3 行，含全角冒号。 | 10, 40 |
| `basic/page.html` | 147 B | 两个 `<a>`、一个 `<div>`；UTF-8/LF。 | 06, 45 |
| `basic/users.json` | 185 B | 3 个 user，msg 形如 `user=42`；单文档 JSON。 | 01 |
| `discover/alpha.json` | 8 B | 目录发现中的普通 JSON。 | 42, 43, 45 |
| `discover/beta.yaml` | 11 B | 目录发现中的其他格式文件。 | 42, 43 |
| `discover/configs/dev.yaml` | 25 B | fuzzy 干扰项，host=db.dev。 | 42, 43 |
| `discover/configs/prodconfig.yaml` | 26 B | fuzzy 目标，host=db.prod。 | 42, 43 |
| `discover/gamma.txt` | 12 B | 目录发现中的纯文本背景文件。 | 42, 43 |
| `discover/node_modules/noise.json` | 9 B | 应被默认忽略规则剪枝。 | 42, 43 |
| `discover/reports/2026-01.csv` | 14 B | glob 排序用例的第一份，total=10。 | 42, 43, 44 |
| `discover/reports/2026-02.csv` | 14 B | glob 排序用例的第二份，total=20。 | 42, 43, 44 |
| `discover/target/noise.json` | 9 B | 应被默认忽略规则剪枝。 | 42, 43 |
| `edge/big-integer.json` | 47 B | 30 位十进制整数 + 一个小整数。 | 19 |
| `edge/binary.bin` ⚠ | 96 B | 含 0xFF/0x00 等非法 UTF-8 字节，触发编码错误而非二进制跳过。 | 23 |
| `edge/blank-lines.jsonl` | 18 B | `{"n":1}` 与 `{"n":2}` 之间各夹一个空行。 | 18, 36 |
| `edge/bom-utf8.jsonl` ⚠ | 19 B | UTF-8 BOM（EF BB BF）+ 两行 JSONL；`.gitattributes` 标记 binary 防止被规范化。 | 12 |
| `edge/crlf.log` ⚠ | 22 B | CRLF 行尾（0D 0A）；`.gitattributes` 标记 binary 保留原始字节。 | 16 |
| `edge/csv-quotes.csv` | 43 B | 含逗号字段与 `""` 转义的引号字段。 | 21 |
| `edge/default-ns.xml` | 50 B | 默认命名空间 `urn:books`，book 无前缀。 | 33, 34 |
| `edge/empty.jsonl` ⚠ | 0 B | 0 字节文件。 | 17 |
| `edge/gbk.txt` ⚠ | 27 B | GB18030 字节、无 BOM；`--encoding auto` 的猜测对象。 | 13 |
| `edge/html-noscript.html` | 63 B | `<noscript>` 内一个 p，外部一个 p。 | 31 |
| `edge/html-tbody.html` | 35 B | 未写 `<tbody>` 的单行表格。 | 28, 29 |
| `edge/html-template.html` | 63 B | `<template>` 内一个 p，外部一个 p。 | 30 |
| `edge/html-uppercase.html` | 41 B | 全部标签/属性名大写。 | 32 |
| `edge/long-line.txt` ⚠ | 4097 B | 单个 4096 字节的 `x` 行，用于 `--max-record-size`。 | 25 |
| `edge/malformed.xml` | 22 B | 标签不闭合，用于触发解析错误。 | 22 |
| `edge/multi-doc.yaml` | 55 B | 用 `---` 分隔的两份文档（alice/admin、bob/editor）。 | 20 |
| `edge/no-extension-jsonl` | 16 B | 内容是 JSONL 但没有扩展名。 | 26, 27 |
| `edge/nul-bytes.bin` ⚠ | 13 B | 纯 ASCII + NUL；二进制检测素材，未被 smoke.py 直接引用（对应用例通过 stdin 构造同样字节）。 | — |
| `edge/prefixed.xml` | 55 B | 同一 URI 但文档前缀为 `x`。 | 35 |
| `edge/split.txt` | 37 B | 两段 id= 记录，中间由 16 个连字符单独成行分隔。 | 11 |
| `edge/utf16be.xml` ⚠ | 154 B | UTF-16BE + FE FF BOM，且内容里带 `encoding="UTF-16"` 声明。 | 15 |
| `edge/utf16le.txt` ⚠ | 34 B | UTF-16LE + FF FE BOM，无 XML 声明。 | 14 |

⚠ = 字节敏感夹具，勿用普通编辑器改。原始内容展开如下：

<details>
<summary>展开全部夹具原文（供逐字节核对）</summary>

**`basic/app.log` · 80 B**

4 行日志，其中 2 行以 `ERROR: ` 开头。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
INFO service started
ERROR: disk full
INFO retry queued
ERROR: connection reset
```

**`basic/books.xml` · 193 B**

显式 `b` 前缀命名空间，两本 book 各带 sku 属性。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
<?xml version="1.0"?>
<root xmlns:b="urn:books">
  <b:book sku="ABC-123"><b:title>Rust in Action</b:title></b:book>
  <b:book sku="DEF-456"><b:title>Programming Rust</b:title></b:book>
</root>
```

**`basic/config.toml` · 58 B**

`[server]` 表，port=8080。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
title = "sample"

[server]
host = "127.0.0.1"
port = 8080
```

**`basic/config.yaml` · 75 B**

嵌套映射 `database.host`。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
database:
  host: db.internal
  port: 5432
features:
  - search
  - export
```

**`basic/events.jsonl` · 70 B**

3 条记录，n=1..3、level 交替；用于输出与 slurp 聚合。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
{"n":1,"level":"INFO"}
{"n":2,"level":"ERROR"}
{"n":3,"level":"INFO"}
```

**`basic/jobs.csv` · 63 B**

3 行；第 3 行 error 含逗号并用双引号包裹。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
id,status,error
1,ok,
2,failed,disk full
3,failed,"disk, full"
```

**`basic/jobs.tsv` · 34 B**

tab 分隔，3 列。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
id\tname\tscore
1\talice\t90
2\tbob\t85
```

**`basic/notes.txt` · 42 B**

中英混合 3 行，含全角冒号。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
第一行：中文
second line
third line
```

**`basic/page.html` · 147 B**

两个 `<a>`、一个 `<div>`；UTF-8/LF。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
<!DOCTYPE html>
<html><body>
<a href="https://example.com/x">x</a>
<a href="https://example.org/y">y</a>
<div id="main">hello</div>
</body></html>
```

**`basic/users.json` · 185 B**

3 个 user，msg 形如 `user=42`；单文档 JSON。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
{
  "team": "core",
  "users": [
    {"id": 1, "name": "alice", "msg": "user=42"},
    {"id": 2, "name": "bob", "msg": "user=7"},
    {"id": 3, "name": "carol", "msg": "user=99"}
  ]
}
```

**`discover/alpha.json` · 8 B**

目录发现中的普通 JSON。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
{"n":1}
```

**`discover/beta.yaml` · 11 B**

目录发现中的其他格式文件。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
name: beta
```

**`discover/configs/dev.yaml` · 25 B**

fuzzy 干扰项，host=db.dev。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
database:
  host: db.dev
```

**`discover/configs/prodconfig.yaml` · 26 B**

fuzzy 目标，host=db.prod。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
database:
  host: db.prod
```

**`discover/gamma.txt` · 12 B**

目录发现中的纯文本背景文件。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
plain noise
```

**`discover/node_modules/noise.json` · 9 B**

应被默认忽略规则剪枝。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
{"n":99}
```

**`discover/reports/2026-01.csv` · 14 B**

glob 排序用例的第一份，total=10。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
id,total
1,10
```

**`discover/reports/2026-02.csv` · 14 B**

glob 排序用例的第二份，total=20。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
id,total
2,20
```

**`discover/target/noise.json` · 9 B**

应被默认忽略规则剪枝。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
{"n":98}
```

**`edge/big-integer.json` · 47 B**

30 位十进制整数 + 一个小整数。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
{"n":123456789012345678901234567890,"small":1}
```

**`edge/binary.bin` · 96 B**

含 0xFF/0x00 等非法 UTF-8 字节，触发编码错误而非二进制跳过。

- 前 16 字节：`89 50 4E 47 00 01 02 03 00 FF 00 1A 89 50 4E 47`
- 内容：二进制（无 UTF-8 文本表示），按上方字节核对即可。

**`edge/blank-lines.jsonl` · 18 B**

`{"n":1}` 与 `{"n":2}` 之间各夹一个空行。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
{"n":1}

{"n":2}
```

**`edge/bom-utf8.jsonl` · 19 B**

UTF-8 BOM（EF BB BF）+ 两行 JSONL；`.gitattributes` 标记 binary 防止被规范化。

- 前 16 字节：`EF BB BF 7B 22 6E 22 3A 31 7D 0A 7B 22 6E 22 3A`
- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
{"n":1}
{"n":2}
```

**`edge/crlf.log` · 22 B**

CRLF 行尾（0D 0A）；`.gitattributes` 标记 binary 保留原始字节。

- 前 16 字节：`49 4E 46 4F 20 6F 6B 0D 0A 45 52 52 4F 52 3A 20`
- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
INFO ok\r
ERROR: crlf\r
```

**`edge/csv-quotes.csv` · 43 B**

含逗号字段与 `""` 转义的引号字段。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
id,note
1,"contains, comma"
2,"say ""hi"""
```

**`edge/default-ns.xml` · 50 B**

默认命名空间 `urn:books`，book 无前缀。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
<root xmlns="urn:books"><book sku="NS-1"/></root>
```

**`edge/empty.jsonl` · 0 B**

0 字节文件。

- 前 16 字节：``
- 内容：（空文件）

**`edge/gbk.txt` · 27 B**

GB18030 字节、无 BOM；`--encoding auto` 的猜测对象。

- 前 16 字节：`D6 D0 CE C4 20 47 42 4B 20 D1 F9 B1 BE 0A B4 ED`
- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
中文 GBK 样本
错误: 磁盘满
```

**`edge/html-noscript.html` · 63 B**

`<noscript>` 内一个 p，外部一个 p。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
<noscript><p id="n">inside</p></noscript><p id="o">outside</p>
```

**`edge/html-tbody.html` · 35 B**

未写 `<tbody>` 的单行表格。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
<table><tr><td>x</td></tr></table>
```

**`edge/html-template.html` · 63 B**

`<template>` 内一个 p，外部一个 p。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
<template><p id="t">inside</p></template><p id="o">outside</p>
```

**`edge/html-uppercase.html` · 41 B**

全部标签/属性名大写。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
<DIV ID="upper"><A HREF="/u">x</A></DIV>
```

**`edge/long-line.txt` · 4097 B**

单个 4096 字节的 `x` 行，用于 `--max-record-size`。

- 前 16 字节：`78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78`
- 内容：`'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'` … 共 4097 字符（单个超长行）。

**`edge/malformed.xml` · 22 B**

标签不闭合，用于触发解析错误。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
<root><broken></root>
```

**`edge/multi-doc.yaml` · 55 B**

用 `---` 分隔的两份文档（alice/admin、bob/editor）。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
---
name: alice
role: admin
---
name: bob
role: editor
```

**`edge/no-extension-jsonl` · 16 B**

内容是 JSONL 但没有扩展名。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
{"n":1}
{"n":2}
```

**`edge/nul-bytes.bin` · 13 B**

纯 ASCII + NUL；二进制检测素材，未被 smoke.py 直接引用（对应用例通过 stdin 构造同样字节）。

- 前 16 字节：`70 6C 61 69 6E 00 62 69 6E 61 72 79 0A`
- 内容：二进制（无 UTF-8 文本表示），按上方字节核对即可。

**`edge/prefixed.xml` · 55 B**

同一 URI 但文档前缀为 `x`。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
<root xmlns:x="urn:books"><x:book sku="PRE-1"/></root>
```

**`edge/split.txt` · 37 B**

两段 id= 记录，中间由 16 个连字符单独成行分隔。

- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
id=123456
----------------
id=654321
```

**`edge/utf16be.xml` · 154 B**

UTF-16BE + FE FF BOM，且内容里带 `encoding="UTF-16"` 声明。

- 前 16 字节：`FE FF 00 3C 00 3F 00 78 00 6D 00 6C 00 20 00 76`
- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
<?xml version="1.0" encoding="UTF-16"?>
<root><item id="i1">值</item></root>
```

**`edge/utf16le.txt` · 34 B**

UTF-16LE + FF FE BOM，无 XML 声明。

- 前 16 字节：`FF FE 55 00 54 00 46 00 2D 00 31 00 36 00 4C 00`
- 内容（`\r`/`\t` 为便于核对做了转义显示）：

```text
UTF-16LE 文本
第二行
```

</details>

## 5. 逐条明细

### 分组 `basic/`

#### 01 · `basic/json-regex-jq`

检查点：JSON 数组展开 → 正则命名捕获 → jq 重组对象；三行 id 顺序应为 42、7、99，且对象里只有 id 一个键。

```bash
ux -e 'jq(.users[].msg) | re(r"user=(?<id>\d+)") | jq({id})' sample/basic/users.json
```

- 单条复现：`python sample/smoke.py --only basic/json-regex-jq`
- 输入：`sample/basic/users.json`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'{"id":"42"}\n{"id":"7"}\n{"id":"99"}\n'`）：

```text
{"id":"42"}
{"id":"7"}
{"id":"99"}
```

#### 02 · `basic/jsonl-with-source`

检查点：JSONL 每行一条记录，`--with-source` 注入 line/record/value；核对首行 line=1、record=1、value=1，末行 value=3。

```bash
ux --with-source -e 'jq(.n)' sample/basic/events.jsonl
```

- 单条复现：`python sample/smoke.py --only basic/jsonl-with-source`
- 输入：`sample/basic/events.jsonl`
- 预期退出码：`0`
- 预期 stdout：不做精确断言
- stdout 必须包含：`"line":1`
- stdout 必须包含：`"record":1`
- stdout 必须包含：`"value":1`
- stdout 必须包含：`"value":3`

#### 03 · `basic/yaml`

检查点：YAML 映射转 JSON 后 jq 取嵌套字段；`-O text` 输出字符串时不能带引号。

```bash
ux -O text -e 'jq(.database.host)' sample/basic/config.yaml
```

- 单条复现：`python sample/smoke.py --only basic/yaml`
- 输入：`sample/basic/config.yaml`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'db.internal\n'`）：

```text
db.internal
```

#### 04 · `basic/toml`

检查点：TOML 的 `[server]` 表被正确建模；数字 8080 在 text 模式下不带引号、不变成字符串字面量。

```bash
ux -O text -e 'jq(.server.port)' sample/basic/config.toml
```

- 单条复现：`python sample/smoke.py --only basic/toml`
- 输入：`sample/basic/config.toml`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'8080\n'`）：

```text
8080
```

#### 05 · `basic/xml-ns-attr`

检查点：`--ns b=urn:books` 绑定前缀，`xpath` 选元素再 `attr` 取属性；默认 JSON 输出应保留双引号。

```bash
ux --ns b=urn:books -e 'xpath("//b:book") | attr("sku")' sample/basic/books.xml
```

- 单条复现：`python sample/smoke.py --only basic/xml-ns-attr`
- 输入：`sample/basic/books.xml`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'"ABC-123"\n"DEF-456"\n'`）：

```text
"ABC-123"
"DEF-456"
```

#### 06 · `basic/html-attr`

检查点：HTML 走 DOM 解析；两个 `<a>` 的 href 按文档顺序输出，不能丢第二个。

```bash
ux -O text -e 'xpath("//a") | attr("href")' sample/basic/page.html
```

- 单条复现：`python sample/smoke.py --only basic/html-attr`
- 输入：`sample/basic/page.html`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'https://example.com/x\nhttps://example.org/y\n'`）：

```text
https://example.com/x
https://example.org/y
```

#### 07 · `basic/csv-select`

检查点：CSV 表头映射 + jq 过滤；关键是第 3 行 `"disk, full"` 的逗号必须留在同一字段内。

```bash
ux -e 'jq(select(.status == "failed") | {id, error})' sample/basic/jobs.csv
```

- 单条复现：`python sample/smoke.py --only basic/csv-select`
- 输入：`sample/basic/jobs.csv`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'{"error":"disk full","id":"2"}\n{"error":"disk, full","id":"3"}\n'`）：

```text
{"error":"disk full","id":"2"}
{"error":"disk, full","id":"3"}
```

#### 08 · `basic/tsv`

检查点：TSV 用制表符分列，两行 name 依次为 alice、bob。

```bash
ux -O text -e 'jq(.name)' sample/basic/jobs.tsv
```

- 单条复现：`python sample/smoke.py --only basic/tsv`
- 输入：`sample/basic/jobs.tsv`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'alice\nbob\n'`）：

```text
alice
bob
```

#### 09 · `basic/text-log-regex`

检查点：`-f text` 逐行记录 + 正则捕获 + `-O text`；只输出两条 ERROR 内容，INFO 行被零匹配自然过滤。

```bash
ux -f text -O text -e 're(r"ERROR: (?<m>.*)") | jq(.m)' sample/basic/app.log
```

- 单条复现：`python sample/smoke.py --only basic/text-log-regex`
- 输入：`sample/basic/app.log`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'disk full\nconnection reset\n'`）：

```text
disk full
connection reset
```

#### 10 · `basic/text-lines`

检查点：text 模式逐行输出且中文 UTF-8 无损；末尾只有一个换行，不能多出空行。

```bash
ux -f text -O text -e 'text()' sample/basic/notes.txt
```

- 单条复现：`python sample/smoke.py --only basic/text-lines`
- 输入：`sample/basic/notes.txt`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'第一行：中文\nsecond line\nthird line\n'`）：

```text
第一行：中文
second line
third line
```

### 分组 `edge/`

#### 11 · `edge/text-raw-split`

检查点：`--text-mode raw` 下用 `split()` 按分隔线切块再取正则；每行都应同时给出 0（整段匹配）、1（捕获组）和 id 三个键。

```bash
ux -f text --text-mode raw -e 'split(r"\r?\n----------------\r?\n") | re(r"id=(?<id>\d{6})")' sample/edge/split.txt
```

- 单条复现：`python sample/smoke.py --only edge/text-raw-split`
- 输入：`sample/edge/split.txt`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'{"0":"id=123456","1":"123456","id":"123456"}\n{"0":"id=654321","1":"654321","id":"654321"}\n'`）：

```text
{"0":"id=123456","1":"123456","id":"123456"}
{"0":"id=654321","1":"654321","id":"654321"}
```

#### 12 · `edge/bom-utf8`

检查点：UTF-8 BOM（EF BB BF）必须被剥离，两行 JSONL 都能解析，不能把 BOM 带进第一个键名。

```bash
ux -O text -e 'jq(.n)' sample/edge/bom-utf8.jsonl
```

- 单条复现：`python sample/smoke.py --only edge/bom-utf8`
- 输入：`sample/edge/bom-utf8.jsonl`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'1\n2\n'`）：

```text
1
2
```

#### 13 · `edge/gbk-auto-fallback`

检查点：`--encoding auto` 猜到 GB18030 并在 stderr 给出 `assumed GB18030` 警告；中文不能乱码。

```bash
ux --encoding auto -f text -O text -e 'text()' sample/edge/gbk.txt
```

- 单条复现：`python sample/smoke.py --only edge/gbk-auto-fallback`
- 输入：`sample/edge/gbk.txt`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'中文 GBK 样本\n错误: 磁盘满\n'`）：

```text
中文 GBK 样本
错误: 磁盘满
```

- stderr 必须包含：`assumed GB18030`

#### 14 · `edge/utf16le`

检查点：显式 UTF-16LE + BOM 解码；两行文本按 LF 正确切分。

```bash
ux --encoding utf-16le -f text -O text -e 'text()' sample/edge/utf16le.txt
```

- 单条复现：`python sample/smoke.py --only edge/utf16le`
- 输入：`sample/edge/utf16le.txt`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'UTF-16LE 文本\n第二行\n'`）：

```text
UTF-16LE 文本
第二行
```

#### 15 · `edge/utf16be-xml-declaration`

检查点：UTF-16BE + BOM + XML 声明三者共存时仍能解析并取属性 id。

```bash
ux --encoding utf-16be -O text -e 'xpath("//item") | attr("id")' sample/edge/utf16be.xml
```

- 单条复现：`python sample/smoke.py --only edge/utf16be-xml-declaration`
- 输入：`sample/edge/utf16be.xml`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'i1\n'`）：

```text
i1
```

#### 16 · `edge/crlf-stripped`

检查点：CRLF 行尾被归一化，输出两行且不含 `\r`（否则终端会出现重叠/回车痕迹）。

```bash
ux -f text -O text -e 'text()' sample/edge/crlf.log
```

- 单条复现：`python sample/smoke.py --only edge/crlf-stripped`
- 输入：`sample/edge/crlf.log`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'INFO ok\nERROR: crlf\n'`）：

```text
INFO ok
ERROR: crlf
```

#### 17 · `edge/empty-jsonl`

检查点：0 字节文件：rc=0、无输出、无警告；空输入不是错误。

```bash
ux -e 'jq(.)' sample/edge/empty.jsonl
```

- 单条复现：`python sample/smoke.py --only edge/empty-jsonl`
- 输入：`sample/edge/empty.jsonl`
- 预期退出码：`0`
- 预期 stdout：**空字符串**

#### 18 · `edge/blank-lines-origin`

检查点：空行被跳过，但 `--with-source` 的 line 保留物理行号（第 3 行仍是 record=2），不能重编号。

```bash
ux --with-source -e 'jq(.n)' sample/edge/blank-lines.jsonl
```

- 单条复现：`python sample/smoke.py --only edge/blank-lines-origin`
- 输入：`sample/edge/blank-lines.jsonl`
- 预期退出码：`0`
- 预期 stdout：不做精确断言
- stdout 必须包含：`"line":1`
- stdout 必须包含：`"record":1`
- stdout 必须包含：`"line":3`
- stdout 必须包含：`"record":2`

#### 19 · `edge/big-integer`

检查点：30 位整数必须逐位保真，不能变成浮点、科学计数法或字符串。

```bash
ux -e 'jq(.n)' sample/edge/big-integer.json
```

- 单条复现：`python sample/smoke.py --only edge/big-integer`
- 输入：`sample/edge/big-integer.json`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'123456789012345678901234567890\n'`）：

```text
123456789012345678901234567890
```

#### 20 · `edge/multi-document-yaml`

检查点：多文档 YAML 按 `---` 拆成两条独立记录，输出 alice、bob。

```bash
ux -O text -e 'jq(.name)' sample/edge/multi-doc.yaml
```

- 单条复现：`python sample/smoke.py --only edge/multi-document-yaml`
- 输入：`sample/edge/multi-doc.yaml`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'alice\nbob\n'`）：

```text
alice
bob
```

#### 21 · `edge/csv-quoted-fields`

检查点：CSV 双引号转义 `""` 与含逗号字段；第二行应输出 `say "hi"`。

```bash
ux -O text -e 'jq(.note)' sample/edge/csv-quotes.csv
```

- 单条复现：`python sample/smoke.py --only edge/csv-quoted-fields`
- 输入：`sample/edge/csv-quotes.csv`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'contains, comma\nsay "hi"\n'`）：

```text
contains, comma
say "hi"
```

#### 22 · `edge/malformed-xml`

检查点：坏 XML 必须显式报错：rc=1、stdout 为空、stderr 含 `invalid XML`；不得静默零匹配。

```bash
ux -e 'text()' sample/edge/malformed.xml
```

- 单条复现：`python sample/smoke.py --only edge/malformed-xml`
- 输入：`sample/edge/malformed.xml`
- 预期退出码：`1`
- 预期 stdout：**空字符串**
- stderr 必须包含：`invalid XML`

#### 23 · `edge/invalid-utf8`

检查点：非法 UTF-8 是输入卫生错误：rc=1、stdout 为空、stderr 含 `not valid UTF-8`。

```bash
ux -e 'text()' sample/edge/binary.bin
```

- 单条复现：`python sample/smoke.py --only edge/invalid-utf8`
- 输入：`sample/edge/binary.bin`
- 预期退出码：`1`
- 预期 stdout：**空字符串**
- stderr 必须包含：`not valid UTF-8`

#### 24 · `edge/binary-stdin-skip`

检查点：stdin 含 NUL：跳过该输入并在 stderr 提示 `binary input skipped`，rc 仍为 0、stdout 空；与上一条的文件级 invalid UTF-8 区分开。

```bash
python -c "import sys; sys.stdout.buffer.write(b'plain\x00binary\n')" | ux -e 'text()'
```

- 单条复现：`python sample/smoke.py --only edge/binary-stdin-skip`
- 输入：无（仅 stdin）
- stdin：`b'plain\x00binary\n'`（命令里的 `python -c` 即用于写入这些字节）
- 预期退出码：`0`
- 预期 stdout：**空字符串**
- stderr 必须包含：`binary input skipped`

#### 25 · `edge/max-record-size`

检查点：`--max-record-size 1KiB` 拦住 4096 字节的行，rc=1 且 stderr 点明 `line exceeds --max-record-size`。

```bash
ux --max-record-size 1KiB -f text -e 'text()' sample/edge/long-line.txt
```

- 单条复现：`python sample/smoke.py --only edge/max-record-size`
- 输入：`sample/edge/long-line.txt`
- 预期退出码：`1`
- 预期 stdout：**空字符串**
- stderr 必须包含：`line exceeds --max-record-size`

#### 26 · `edge/no-extension-auto-misdetects`

检查点：无扩展名文件按内容自动嗅探会误判为 JSON 并报 `invalid JSON`；重点是把误判显式化（rc=1），不是悄悄出零结果。

```bash
ux -e 'jq(.)' sample/edge/no-extension-jsonl
```

- 单条复现：`python sample/smoke.py --only edge/no-extension-auto-misdetects`
- 输入：`sample/edge/no-extension-jsonl`
- 预期退出码：`1`
- 预期 stdout：**空字符串**
- stderr 必须包含：`invalid JSON`

#### 27 · `edge/no-extension-explicit-jsonl`

检查点：与上一条对照：`-f jsonl` 显式声明后同一文件正常解析出 1、2。

```bash
ux -f jsonl -O text -e 'jq(.n)' sample/edge/no-extension-jsonl
```

- 单条复现：`python sample/smoke.py --only edge/no-extension-explicit-jsonl`
- 输入：`sample/edge/no-extension-jsonl`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'1\n2\n'`）：

```text
1
2
```

#### 28 · `edge/implicit-tbody-not-visible`

检查点：html5ever 插入隐式 `<tbody>`，因此 `//table/tr` 零匹配；零匹配是正常结果（rc=0、stdout 空）。

```bash
ux -f html -O text -e 'xpath("//table/tr") | text()' sample/edge/html-tbody.html
```

- 单条复现：`python sample/smoke.py --only edge/implicit-tbody-not-visible`
- 输入：`sample/edge/html-tbody.html`
- 预期退出码：`0`
- 预期 stdout：**空字符串**

#### 29 · `edge/explicit-tbody-visible`

检查点：与上一条对照：显式写 `//table/tbody/tr` 能取到 x，确认 DOM 树里确实存在 tbody 层。

```bash
ux -f html -O text -e 'xpath("//table/tbody/tr") | text()' sample/edge/html-tbody.html
```

- 单条复现：`python sample/smoke.py --only edge/explicit-tbody-visible`
- 输入：`sample/edge/html-tbody.html`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'x\n'`）：

```text
x
```

#### 30 · `edge/template-contents-in-dom`

检查点：`<template>` 内容被展开进 XPath 可见树，因此 id 为 t 和 o 的 `<p>` 都出现。

```bash
ux -f html -O text -e 'xpath("//p") | attr("id")' sample/edge/html-template.html
```

- 单条复现：`python sample/smoke.py --only edge/template-contents-in-dom`
- 输入：`sample/edge/html-template.html`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'t\no\n'`）：

```text
t
o
```

#### 31 · `edge/noscript-is-raw-text`

检查点：`<noscript>` 内容是原始文本而非元素树，里面的 p 不可见；只输出 o。

```bash
ux -f html -O text -e 'xpath("//p") | attr("id")' sample/edge/html-noscript.html
```

- 单条复现：`python sample/smoke.py --only edge/noscript-is-raw-text`
- 输入：`sample/edge/html-noscript.html`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'o\n'`）：

```text
o
```

#### 32 · `edge/html-case-folding`

检查点：大写 `<A HREF=...>` 折叠为小写后仍能被 `//a` 与 `attr("href")` 命中。

```bash
ux -f html -O text -e 'xpath("//a") | attr("href")' sample/edge/html-uppercase.html
```

- 单条复现：`python sample/smoke.py --only edge/html-case-folding`
- 输入：`sample/edge/html-uppercase.html`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'/u\n'`）：

```text
/u
```

#### 33 · `edge/default-ns-with-uri`

检查点：正确 URI 绑定能匹配默认命名空间文档，输出 NS-1；说明 `--ns` 实际按 URI 匹配（与旧文档结论相反）。

```bash
ux --ns b=urn:books -f xml -O text -e 'xpath("//b:book") | attr("sku")' sample/edge/default-ns.xml
```

- 单条复现：`python sample/smoke.py --only edge/default-ns-with-uri`
- 输入：`sample/edge/default-ns.xml`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'NS-1\n'`）：

```text
NS-1
```

#### 34 · `edge/default-ns-wrong-uri`

检查点：同一文档换错 URI（`urn:other`）后零匹配；与上一条一起证明 URI 真的参与匹配。

```bash
ux --ns b=urn:other -f xml -O text -e 'xpath("//b:book") | attr("sku")' sample/edge/default-ns.xml
```

- 单条复现：`python sample/smoke.py --only edge/default-ns-wrong-uri`
- 输入：`sample/edge/default-ns.xml`
- 预期退出码：`0`
- 预期 stdout：**空字符串**

#### 35 · `edge/prefixed-same-uri`

检查点：文档用 x 前缀、表达式用 b 前缀，只要 URI 相同仍能匹配，输出 PRE-1。

```bash
ux --ns b=urn:books -f xml -O text -e 'xpath("//b:book") | attr("sku")' sample/edge/prefixed.xml
```

- 单条复现：`python sample/smoke.py --only edge/prefixed-same-uri`
- 输入：`sample/edge/prefixed.xml`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'PRE-1\n'`）：

```text
PRE-1
```

### 分组 `cli/`

#### 36 · `cli/slurp-across-files`

检查点：`slurp()` 跨两个文件聚合 3+2=5 条记录，只输出一次结果 5；顺序与文件参数一致。

```bash
ux -f jsonl -O text -e 'slurp() | jq(length)' sample/basic/events.jsonl sample/edge/blank-lines.jsonl
```

- 单条复现：`python sample/smoke.py --only cli/slurp-across-files`
- 输入：`sample/basic/events.jsonl`, `sample/edge/blank-lines.jsonl`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'5\n'`）：

```text
5
```

#### 37 · `cli/json-pretty`

检查点：`-O json` 把流式结果收成单个数组并 pretty 打印；核对缩进、键按字母序、末尾换行。

```bash
ux -O json -e 'jq(.)' sample/basic/events.jsonl
```

- 单条复现：`python sample/smoke.py --only cli/json-pretty`
- 输入：`sample/basic/events.jsonl`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'[\n  {\n    "level": "INFO",\n    "n": 1\n  },\n  {\n    "level": "ERROR",\n    "n": 2\n  },\n  {\n    "level": "INFO",\n    "n": 3\n  }\n]\n'`）：

```text
[
  {
    "level": "INFO",
    "n": 1
  },
  {
    "level": "ERROR",
    "n": 2
  },
  {
    "level": "INFO",
    "n": 3
  }
]
```

#### 38 · `cli/json-compact`

检查点：与上一条同样的数组，`--compact` 压成一行，无多余空格。

```bash
ux -O json --compact -e 'jq(.)' sample/basic/events.jsonl
```

- 单条复现：`python sample/smoke.py --only cli/json-compact`
- 输入：`sample/basic/events.jsonl`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'[{"level":"INFO","n":1},{"level":"ERROR","n":2},{"level":"INFO","n":3}]\n'`）：

```text
[{"level":"INFO","n":1},{"level":"ERROR","n":2},{"level":"INFO","n":3}]
```

#### 39 · `cli/max-results-is-not-silent`

检查点：`--max-results 2` 截断后必须 rc=1 且 stderr 提到 `--max-results`；stdout 仍保留前两行 1、2。

```bash
ux --max-results 2 -e 'jq(.n)' sample/basic/events.jsonl
```

- 单条复现：`python sample/smoke.py --only cli/max-results-is-not-silent`
- 输入：`sample/basic/events.jsonl`
- 预期退出码：`1`
- 预期 stdout（精确相等，repr = `'1\n2\n'`）：

```text
1
2
```

- stderr 必须包含：`--max-results`

#### 40 · `cli/with-source-text-conflict`

检查点：`--with-source` 与 `-O text` 互斥，参数校验前置：rc=2、stdout 空、stderr 含 `mutually exclusive`。

```bash
ux -O text --with-source -e 'text()' sample/basic/notes.txt
```

- 单条复现：`python sample/smoke.py --only cli/with-source-text-conflict`
- 输入：`sample/basic/notes.txt`
- 预期退出码：`2`
- 预期 stdout：**空字符串**
- stderr 必须包含：`mutually exclusive`

#### 41 · `cli/nesting-guard`

检查点：300 层嵌套括号在编译期被 `check_nesting(128)` 拦下：rc=2、stderr 含 `nesting exceeds 128`，绝不能栈溢出（rc=101/134）。

```bash
E=$(python -c "print('jq(' + '('*300 + '.a' + ')'*300 + ')')")
echo null | ux -f json -e "$E"
```

- 单条复现：`python sample/smoke.py --only cli/nesting-guard`
- 输入：无（仅 stdin）
- stdin：`b'null\n'`（由上方命令的 `echo null` 提供）
- 预期退出码：`2`
- 预期 stdout：**空字符串**
- stderr 必须包含：`nesting exceeds 128`

### 分组 `discover/`

#### 42 · `discover/default-ignore-prunes`

检查点：`--fuzzy noise` 递归目录时剪掉 `node_modules/`、`target/`；两个 noise.json 都不会被读到，stdout 空。

```bash
ux --fuzzy noise -e 'text()' sample/discover
```

- 单条复现：`python sample/smoke.py --only discover/default-ignore-prunes`
- 输入：`sample/discover`
- 预期退出码：`0`
- 预期 stdout：**空字符串**

#### 43 · `discover/fuzzy`

检查点：fuzzy 命中 `configs/prodconfig.yaml` 并读出 db.prod；核对不是 dev.yaml。

```bash
ux --fuzzy prodconfig -O text -e 'jq(.database.host)' sample/discover
```

- 单条复现：`python sample/smoke.py --only discover/fuzzy`
- 输入：`sample/discover`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'db.prod\n'`）：

```text
db.prod
```

#### 44 · `discover/glob-order`

检查点：glob 展开必须确定性排序（2026-01 → 2026-02），输出 10、20；不能受文件系统遍历顺序影响。

```bash
ux -f csv -O text -e 'jq(.total)' 'sample/discover/reports/*.csv'
```

- 单条复现：`python sample/smoke.py --only discover/glob-order`
- 输入：`sample/discover/reports/*.csv`
- 注：命令里的 glob 刻意加引号，由 `ux` 自己展开；不要让 shell 抢先展开，否则顺序断言失去意义。
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'10\n20\n'`）：

```text
10
20
```

#### 45 · `discover/skip-incompatible`

检查点：`--skip-incompatible` 跳过格式不兼容的 HTML，只输出 1 且 rc=0；不因单个文件类型不匹配而整批失败。

```bash
ux --skip-incompatible -O text -e 'jq(.n)' sample/discover/alpha.json sample/basic/page.html
```

- 单条复现：`python sample/smoke.py --only discover/skip-incompatible`
- 输入：`sample/discover/alpha.json`, `sample/basic/page.html`
- 预期退出码：`0`
- 预期 stdout（精确相等，repr = `'1\n'`）：

```text
1
```

## 6. 已知待修项 / 不在本清单覆盖范围

- **未注册前缀会 panic**：`xpath("//x:book")` 在 `sxd-xpath 0.4.2` 内部 panic（rc=101），按 P2 应为 `Query(2)`。刻意没有把 panic 写成预期结果，因此本清单不覆盖该输入。
- **文档与实测相反**：README「已知限制」、设计文档 §6.4/§10 与 `--ns` 帮助文案仍称「只解析前缀、不按 URI 匹配」，但用例 33–35 证明正确 URI 能匹配默认命名空间及不同前缀的等价文档、错 URI 零匹配（用例 05 只是常规同名前缀通路）。文档待单独一轮纠正。
- 本清单只覆盖 `sample/smoke.py` 的 CLI 黑盒用例；Rust 单测/集成测试、fuzz、以及设计文档 §8.3 的两条流式黑盒断言不在此列。
- 校验清单是否过期：`python sample/export_cases.py --check`（CI 已接入）。

