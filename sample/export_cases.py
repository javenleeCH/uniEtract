#!/usr/bin/env python3
"""Regenerate sample/CASES.md, the human-review checklist for sample fixtures.

The checklist is derived from `smoke.py`'s CASES list (single source of truth),
so commands, expected exit codes and expected output cannot drift apart from
the automated assertions.

Usage (from anywhere):
    python sample/export_cases.py            # rewrite sample/CASES.md
    python sample/export_cases.py --check   # fail if CASES.md is stale (CI)

Adding a case to smoke.py requires a matching entry in NOTES; the script
refuses to generate a checklist with an undocumented case.
"""

import argparse
import fnmatch
import importlib.util
import shlex
import sys
from pathlib import Path

SAMPLE = Path(__file__).resolve().parent
OUT = SAMPLE / "CASES.md"

# What a human should actually look at when spot-checking this case.
NOTES = {
    # basic/
    "basic/json-regex-jq": "JSON 数组展开 → 正则命名捕获 → jq 重组对象；三行 id 顺序应为 42、7、99，且对象里只有 id 一个键。",
    "basic/jsonl-with-source": "JSONL 每行一条记录，`--with-source` 注入 line/record/value；核对首行 line=1、record=1、value=1，末行 value=3。",
    "basic/yaml": "YAML 映射转 JSON 后 jq 取嵌套字段；`-O text` 输出字符串时不能带引号。",
    "basic/toml": "TOML 的 `[server]` 表被正确建模；数字 8080 在 text 模式下不带引号、不变成字符串字面量。",
    "basic/xml-ns-attr": "`--ns b=urn:books` 绑定前缀，`xpath` 选元素再 `attr` 取属性；默认 JSON 输出应保留双引号。",
    "basic/html-attr": "HTML 走 DOM 解析；两个 `<a>` 的 href 按文档顺序输出，不能丢第二个。",
    "basic/csv-select": "CSV 表头映射 + jq 过滤；关键是第 3 行 `\"disk, full\"` 的逗号必须留在同一字段内。",
    "basic/tsv": "TSV 用制表符分列，两行 name 依次为 alice、bob。",
    "basic/text-log-regex": "`-f text` 逐行记录 + 正则捕获 + `-O text`；只输出两条 ERROR 内容，INFO 行被零匹配自然过滤。",
    "basic/text-lines": "text 模式逐行输出且中文 UTF-8 无损；末尾只有一个换行，不能多出空行。",
    # edge/ bytes and encodings
    "edge/text-raw-split": "`--text-mode raw` 下用 `split()` 按分隔线切块再取正则；每行都应同时给出 0（整段匹配）、1（捕获组）和 id 三个键。",
    "edge/bom-utf8": "UTF-8 BOM（EF BB BF）必须被剥离，两行 JSONL 都能解析，不能把 BOM 带进第一个键名。",
    "edge/gbk-auto-fallback": "`--encoding auto` 猜到 GB18030 并在 stderr 给出 `assumed GB18030` 警告；中文不能乱码。",
    "edge/utf16le": "显式 UTF-16LE + BOM 解码；两行文本按 LF 正确切分。",
    "edge/utf16be-xml-declaration": "UTF-16BE + BOM + XML 声明三者共存时仍能解析并取属性 id。",
    "edge/crlf-stripped": "CRLF 行尾被归一化，输出两行且不含 `\\r`（否则终端会出现重叠/回车痕迹）。",
    "edge/empty-jsonl": "0 字节文件：rc=0、无输出、无警告；空输入不是错误。",
    "edge/blank-lines-origin": "空行被跳过，但 `--with-source` 的 line 保留物理行号（第 3 行仍是 record=2），不能重编号。",
    "edge/big-integer": "30 位整数必须逐位保真，不能变成浮点、科学计数法或字符串。",
    "edge/multi-document-yaml": "多文档 YAML 按 `---` 拆成两条独立记录，输出 alice、bob。",
    "edge/csv-quoted-fields": "CSV 双引号转义 `\"\"` 与含逗号字段；第二行应输出 `say \"hi\"`。",
    "edge/malformed-xml": "坏 XML 必须显式报错：rc=1、stdout 为空、stderr 含 `invalid XML`；不得静默零匹配。",
    "edge/invalid-utf8": "非法 UTF-8 是输入卫生错误：rc=1、stdout 为空、stderr 含 `not valid UTF-8`。",
    "edge/binary-stdin-skip": "stdin 含 NUL：跳过该输入并在 stderr 提示 `binary input skipped`，rc 仍为 0、stdout 空；与上一条的文件级 invalid UTF-8 区分开。",
    "edge/max-record-size": "`--max-record-size 1KiB` 拦住 4096 字节的行，rc=1 且 stderr 点明 `line exceeds --max-record-size`。",
    "edge/no-extension-auto-misdetects": "无扩展名文件按内容自动嗅探会误判为 JSON 并报 `invalid JSON`；重点是把误判显式化（rc=1），不是悄悄出零结果。",
    "edge/no-extension-explicit-jsonl": "与上一条对照：`-f jsonl` 显式声明后同一文件正常解析出 1、2。",
    # edge/ HTML tree shape
    "edge/implicit-tbody-not-visible": "html5ever 插入隐式 `<tbody>`，因此 `//table/tr` 零匹配；零匹配是正常结果（rc=0、stdout 空）。",
    "edge/explicit-tbody-visible": "与上一条对照：显式写 `//table/tbody/tr` 能取到 x，确认 DOM 树里确实存在 tbody 层。",
    "edge/template-contents-in-dom": "`<template>` 内容被展开进 XPath 可见树，因此 id 为 t 和 o 的 `<p>` 都出现。",
    "edge/noscript-is-raw-text": "`<noscript>` 内容是原始文本而非元素树，里面的 p 不可见；只输出 o。",
    "edge/html-case-folding": "大写 `<A HREF=...>` 折叠为小写后仍能被 `//a` 与 `attr(\"href\")` 命中。",
    # edge/ XML namespaces
    "edge/default-ns-with-uri": "正确 URI 绑定能匹配默认命名空间文档，输出 NS-1；说明 `--ns` 实际按 URI 匹配（与旧文档结论相反）。",
    "edge/default-ns-wrong-uri": "同一文档换错 URI（`urn:other`）后零匹配；与上一条一起证明 URI 真的参与匹配。",
    "edge/prefixed-same-uri": "文档用 x 前缀、表达式用 b 前缀，只要 URI 相同仍能匹配，输出 PRE-1。",
    # cli/
    "cli/slurp-across-files": "`slurp()` 跨两个文件聚合 3+2=5 条记录，只输出一次结果 5；顺序与文件参数一致。",
    "cli/json-pretty": "`-O json` 把流式结果收成单个数组并 pretty 打印；核对缩进、键按字母序、末尾换行。",
    "cli/json-compact": "与上一条同样的数组，`--compact` 压成一行，无多余空格。",
    "cli/max-results-is-not-silent": "`--max-results 2` 截断后必须 rc=1 且 stderr 提到 `--max-results`；stdout 仍保留前两行 1、2。",
    "cli/with-source-text-conflict": "`--with-source` 与 `-O text` 互斥，参数校验前置：rc=2、stdout 空、stderr 含 `mutually exclusive`。",
    "cli/nesting-guard": "300 层嵌套括号在编译期被 `check_nesting(128)` 拦下：rc=2、stderr 含 `nesting exceeds 128`，绝不能栈溢出（rc=101/134）。",
    # discover/
    "discover/default-ignore-prunes": "`--fuzzy noise` 递归目录时剪掉 `node_modules/`、`target/`；两个 noise.json 都不会被读到，stdout 空。",
    "discover/fuzzy": "fuzzy 命中 `configs/prodconfig.yaml` 并读出 db.prod；核对不是 dev.yaml。",
    "discover/glob-order": "glob 展开必须确定性排序（2026-01 → 2026-02），输出 10、20；不能受文件系统遍历顺序影响。",
    "discover/skip-incompatible": "`--skip-incompatible` 跳过格式不兼容的 HTML，只输出 1 且 rc=0；不因单个文件类型不匹配而整批失败。",
}

# Commands that are impractical to print verbatim (very long generated DSL).
CMD_OVERRIDE = {
    "cli/nesting-guard": (
        'E=$(python -c "print(\'jq(\' + \'(\'*300 + \'.a\' + \')\'*300 + \')\')")\n'
        'echo null | ux -f json -e "$E"'
    ),
}

GROUP_INFO = {
    "basic": "每种输入格式的标准通路，优先抽 2–3 条确认格式嗅探与输出模式。",
    "edge": "输入卫生、编码、HTML 树形与命名空间；风险最高，建议重点抽样。",
    "cli": "会话聚合、输出收口与参数/编译期错误契约。",
    "discover": "目录发现、默认剪枝、排序与 fuzzy 过滤。",
}

FIXTURE_NOTES = {
    "basic/users.json": "3 个 user，msg 形如 `user=42`；单文档 JSON。",
    "basic/events.jsonl": "3 条记录，n=1..3、level 交替；用于输出与 slurp 聚合。",
    "basic/config.yaml": "嵌套映射 `database.host`。",
    "basic/config.toml": "`[server]` 表，port=8080。",
    "basic/books.xml": "显式 `b` 前缀命名空间，两本 book 各带 sku 属性。",
    "basic/page.html": "两个 `<a>`、一个 `<div>`；UTF-8/LF。",
    "basic/jobs.csv": "3 行；第 3 行 error 含逗号并用双引号包裹。",
    "basic/jobs.tsv": "tab 分隔，3 列。",
    "basic/app.log": "4 行日志，其中 2 行以 `ERROR: ` 开头。",
    "basic/notes.txt": "中英混合 3 行，含全角冒号。",
    "edge/split.txt": "两段 id= 记录，中间由 16 个连字符单独成行分隔。",
    "edge/blank-lines.jsonl": "`{\"n\":1}` 与 `{\"n\":2}` 之间各夹一个空行。",
    "edge/big-integer.json": "30 位十进制整数 + 一个小整数。",
    "edge/multi-doc.yaml": "用 `---` 分隔的两份文档（alice/admin、bob/editor）。",
    "edge/csv-quotes.csv": "含逗号字段与 `\"\"` 转义的引号字段。",
    "edge/malformed.xml": "标签不闭合，用于触发解析错误。",
    "edge/default-ns.xml": "默认命名空间 `urn:books`，book 无前缀。",
    "edge/prefixed.xml": "同一 URI 但文档前缀为 `x`。",
    "edge/html-tbody.html": "未写 `<tbody>` 的单行表格。",
    "edge/html-template.html": "`<template>` 内一个 p，外部一个 p。",
    "edge/html-noscript.html": "`<noscript>` 内一个 p，外部一个 p。",
    "edge/html-uppercase.html": "全部标签/属性名大写。",
    "edge/long-line.txt": "单个 4096 字节的 `x` 行，用于 `--max-record-size`。",
    "edge/no-extension-jsonl": "内容是 JSONL 但没有扩展名。",
    "discover/alpha.json": "目录发现中的普通 JSON。",
    "discover/beta.yaml": "目录发现中的其他格式文件。",
    "discover/gamma.txt": "目录发现中的纯文本背景文件。",
    "discover/configs/prodconfig.yaml": "fuzzy 目标，host=db.prod。",
    "discover/configs/dev.yaml": "fuzzy 干扰项，host=db.dev。",
    "discover/node_modules/noise.json": "应被默认忽略规则剪枝。",
    "discover/target/noise.json": "应被默认忽略规则剪枝。",
    "discover/reports/2026-01.csv": "glob 排序用例的第一份，total=10。",
    "discover/reports/2026-02.csv": "glob 排序用例的第二份，total=20。",
    # byte-sensitive fixtures
    "edge/bom-utf8.jsonl": "UTF-8 BOM（EF BB BF）+ 两行 JSONL；`.gitattributes` 标记 binary 防止被规范化。",
    "edge/gbk.txt": "GB18030 字节、无 BOM；`--encoding auto` 的猜测对象。",
    "edge/utf16le.txt": "UTF-16LE + FF FE BOM，无 XML 声明。",
    "edge/utf16be.xml": "UTF-16BE + FE FF BOM，且内容里带 `encoding=\"UTF-16\"` 声明。",
    "edge/crlf.log": "CRLF 行尾（0D 0A）；`.gitattributes` 标记 binary 保留原始字节。",
    "edge/empty.jsonl": "0 字节文件。",
    "edge/binary.bin": "含 0xFF/0x00 等非法 UTF-8 字节，触发编码错误而非二进制跳过。",
    "edge/nul-bytes.bin": "纯 ASCII + NUL；二进制检测素材，未被 smoke.py 直接引用（对应用例通过 stdin 构造同样字节）。",
}

BYTE_SENSITIVE = {
    "edge/bom-utf8.jsonl", "edge/gbk.txt", "edge/utf16le.txt",
    "edge/utf16be.xml", "edge/crlf.log", "edge/empty.jsonl",
    "edge/binary.bin", "edge/nul-bytes.bin", "edge/long-line.txt",
}


def load_cases():
    spec = importlib.util.spec_from_file_location("sample_smoke", SAMPLE / "smoke.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.CASES


def inline(text):
    """Wrap in inline code, tolerating backticks in the payload."""
    text = str(text).replace("\n", "\\n")
    fence = "`" * (max(1, text.count("`") + 1))
    return f"{fence} {text} {fence}" if "`" in text else f"`{text}`"


def command_for(case):
    name = case["name"]
    if name in CMD_OVERRIDE:
        return CMD_OVERRIDE[name]
    cmd = "ux " + shlex.join(case["args"])
    stdin = case.get("stdin")
    if stdin is not None:
        data = stdin.encode("utf-8") if isinstance(stdin, str) else stdin
        cmd = f'python -c "import sys; sys.stdout.buffer.write({data!r})" | ' + cmd
    if len(cmd) > 300:
        cmd = cmd[:160] + " …（参数过长已截断，请用 --only 复现；完整定义见 smoke.py）"
    return cmd


def inputs_for(case):
    return [a for a in case["args"] if a.startswith("sample/")]


def referenced_numbers(rel, cases):
    hits = []
    for i, case in enumerate(cases, 1):
        for arg in inputs_for(case):
            p = arg[len("sample/"):]
            if p == rel or fnmatch.fnmatch(rel, p):
                hits.append(i)
                break
            # directory argument, e.g. sample/discover
            if not Path(p).suffix and rel.startswith(p.rstrip("/") + "/"):
                hits.append(i)
                break
    return hits


def shorten(note, limit=48):
    """First clause of a note, truncated without breaking an inline code span."""
    text = note.split("；")[0]
    if len(text) <= limit:
        return text
    cut = text[:limit]
    if cut.count("`") % 2:
        closing = text.find("`", limit)
        cut = text[:closing + 1] if closing != -1 else cut[:cut.rfind("`")]
    return cut + "…"


def fixture_preview(rel, path):
    data = path.read_bytes()
    lines = []
    if rel in BYTE_SENSITIVE:
        head = " ".join(f"{b:02X}" for b in data[:16])
        lines.append(f"- 前 16 字节：`{head}`")
    if rel == "edge/gbk.txt":
        text = data.decode("gb18030")
    elif rel == "edge/utf16le.txt":
        text = data[2:].decode("utf-16-le")
    elif rel == "edge/utf16be.xml":
        text = data[2:].decode("utf-16-be")
    elif rel == "edge/bom-utf8.jsonl":
        text = data[3:].decode("utf-8")
    elif rel in ("edge/binary.bin", "edge/nul-bytes.bin"):
        text = None
    else:
        try:
            text = data.decode("utf-8")
        except UnicodeDecodeError:
            text = None
    if text is None:
        lines.append("- 内容：二进制（无 UTF-8 文本表示），按上方字节核对即可。")
    elif not text:
        lines.append("- 内容：（空文件）")
    elif len(text) > 300:
        lines.append(f"- 内容：{inline(repr(text[:40]))} … 共 {len(text)} 字符（单个超长行）。")
    else:
        shown = text.replace("\r", "\\r").replace("\t", "\\t")
        lines.append("- 内容（`\\r`/`\\t` 为便于核对做了转义显示）：")
        lines.append("")
        lines.append("```text")
        lines.append(shown.rstrip("\n"))
        lines.append("```")
    return "\n".join(lines)


def render(cases):
    missing = [c["name"] for c in cases if c["name"] not in NOTES]
    if missing:
        raise SystemExit("missing NOTES entries: " + ", ".join(missing))

    fixtures = sorted(
        (p for p in SAMPLE.rglob("*")
         if p.is_file()
         and p.suffix not in (".py", ".md", ".pyc", ".pyo")
         and "__pycache__" not in p.relative_to(SAMPLE).parts),
        key=lambda p: p.relative_to(SAMPLE).as_posix(),
    )
    out = []
    add = out.append

    add("# sample/ 人工抽样检查清单")
    add("")
    add("> 本文件由 `python sample/export_cases.py` 从 `sample/smoke.py` 自动生成，**请勿手工编辑**。")
    add("> 命令、预期退出码与预期输出直接取自自动化断言；`sample/CASES.md` 与 `sample/smoke.py` 一一对应。")
    add(f"> 当前共 **{len(cases)} 条用例**、**{len(fixtures)} 个夹具**。")
    add("")
    add("## 1. 怎么用")
    add("")
    add("1. 在仓库根目录构建 release 二进制：`cargo build --release`。下文命令里的 `ux` 指 `target/release/ux`（Windows 为 `ux.exe`），可写全路径或加入 PATH。")
    add("2. 从下面的总览表挑几条，勾选 `人工确认`；建议每组至少一条，错误路径用例优先。")
    add("3. 复制该用例明细里的 `bash` 命令，在仓库根目录执行，对照列出的退出码、stdout 与 stderr 要求。")
    add("4. 不想手敲命令时，用跨平台且不经 shell 的单条复现命令：`python sample/smoke.py --only <用例名>`；全量执行 `python sample/smoke.py`。")
    add("")
    add("约定与坑：")
    add("")
    add("- 命令按 **bash** 写法给出；Windows Git Bash 先执行 `export MSYS2_ARG_CONV_EXCL='*'`，否则 `-e` 里的 `//a`、`//b:book` 会被改写成 UNC 路径而静默零匹配。PowerShell 不做该转换，但单引号内不转义单引号，含 XPath 的表达式建议直接用 `--only` 复现。")
    add("- `stdout = 精确相等`（含尾随换行）；`stdout 包含` / `stderr 包含` 是子串断言；未列出的流不做要求。")
    add("- `rc=0` 正常结束、`rc=1` 运行期/输入错误、`rc=2` 参数或 DSL 编译期错误（P5 校验前置）。零匹配是正常结果，不算失败。")
    add("- 字节敏感夹具（BOM、CRLF、UTF-16、GB18030、二进制）在 `.gitattributes` 中标为 `binary`；重建请用 `python sample/generate.py`，不要用会改写换行/编码的编辑器保存。")
    add("")
    add("## 2. 分组与抽样建议")
    add("")
    add("| 分组 | 条数 | 关注点 |")
    add("|---|---:|---|")
    for group in ("basic", "edge", "cli", "discover"):
        count = sum(1 for c in cases if c["name"].startswith(group + "/"))
        add(f"| `{group}/` | {count} | {GROUP_INFO[group]} |")
    add("")
    add("按风险维度建议至少抽这些编号：")
    add("")
    add("- 编码与输入卫生：12–16、23–27")
    add("- 错误与边界契约：22、25、26、39、40、41（尤其 41 必须看到 rc=2 而不是进程崩溃）")
    add("- HTML 树形：28–32；命名空间：05、33–35")
    add("- 聚合与输出：36–38；文件发现确定性：42–45")
    add("")
    add("## 3. 用例总览")
    add("")
    add("| # | 用例 | 检查点（一句话） | 预期 | 人工确认 |")
    add("|---:|---|---|---|:---:|")
    for i, case in enumerate(cases, 1):
        short = shorten(NOTES[case["name"]]).replace("|", "\\|")
        parts = [f"rc={case['rc']}"]
        stdout = case.get("stdout")
        if stdout is not None:
            parts.append("stdout 空" if stdout == "" else f"stdout {len(stdout.splitlines())} 行")
        if case.get("stdout_contains"):
            parts.append("含指定片段")
        if case.get("stderr_contains"):
            parts.append("stderr 有提示")
        if case.get("stdin") is not None:
            parts.append("stdin")
        expected = " · ".join(parts).replace("|", "\\|")
        add(f"| {i:02d} | `{case['name']}` | {short} | {expected} | ☐ |")
    add("")
    add("## 4. 夹具清单")
    add("")
    add("`引用` 列是用例编号，标 `—` 表示未被自动化断言直接使用，适合优先人工检查。")
    add("")
    add("| 夹具（相对 `sample/`） | 大小 | 特征 / 用途 | 引用 |")
    add("|---|---:|---|---|")
    for path in fixtures:
        rel = path.relative_to(SAMPLE).as_posix()
        note = FIXTURE_NOTES.get(rel, "—").replace("|", "\\|")
        hits = referenced_numbers(rel, cases)
        refs = ", ".join(f"{n:02d}" for n in hits) if hits else "—"
        mark = " ⚠" if rel in BYTE_SENSITIVE else ""
        add(f"| `{rel}`{mark} | {path.stat().st_size} B | {note} | {refs} |")
    add("")
    add("⚠ = 字节敏感夹具，勿用普通编辑器改。原始内容展开如下：")
    add("")
    add("<details>")
    add("<summary>展开全部夹具原文（供逐字节核对）</summary>")
    add("")
    for path in fixtures:
        rel = path.relative_to(SAMPLE).as_posix()
        add(f"**`{rel}` · {path.stat().st_size} B**")
        add("")
        add(FIXTURE_NOTES.get(rel, "—"))
        add("")
        add(fixture_preview(rel, path))
        add("")
    add("</details>")
    add("")
    add("## 5. 逐条明细")
    add("")

    group = None
    for i, case in enumerate(cases, 1):
        this_group = case["name"].split("/")[0]
        if this_group != group:
            group = this_group
            add(f"### 分组 `{group}/`")
            add("")
        add(f"#### {i:02d} · `{case['name']}`")
        add("")
        add(f"检查点：{NOTES[case['name']]}")
        add("")
        add("```bash")
        add(command_for(case))
        add("```")
        add("")
        add(f"- 单条复现：`python sample/smoke.py --only {case['name']}`")
        inputs = inputs_for(case)
        add(f"- 输入：{', '.join(inline(p) for p in inputs) if inputs else '无（仅 stdin）'}")
        if any("*" in p for p in inputs):
            add("- 注：命令里的 glob 刻意加引号，由 `ux` 自己展开；不要让 shell 抢先展开，否则顺序断言失去意义。")
        stdin = case.get("stdin")
        if stdin is not None:
            data = stdin.encode("utf-8") if isinstance(stdin, str) else stdin
            how = "由上方命令的 `echo null` 提供" if case["name"] in CMD_OVERRIDE \
                else "命令里的 `python -c` 即用于写入这些字节"
            add(f"- stdin：{inline(repr(data))}（{how}）")
        add(f"- 预期退出码：`{case['rc']}`")
        stdout = case.get("stdout")
        if stdout is None:
            add("- 预期 stdout：不做精确断言")
        elif stdout == "":
            add("- 预期 stdout：**空字符串**")
        else:
            add(f"- 预期 stdout（精确相等，repr = {inline(repr(stdout))}）：")
            add("")
            add("```text")
            add(stdout.rstrip("\n"))
            add("```")
            if case.get("stdout_contains") or case.get("stderr_contains"):
                add("")
        for needle in case.get("stdout_contains", []):
            add(f"- stdout 必须包含：{inline(needle)}")
        stderr = case.get("stderr_contains")
        if stderr:
            add(f"- stderr 必须包含：{inline(stderr)}")
        add("")

    add("## 6. 已修复事项与清单边界")
    add("")
    add("- **未注册前缀已在 0.3.0 修复**：`xpath(\"//x:book\")` 现在于编译期返回 `Query(2)` 并提示 `--ns PREFIX=URI`，不再触发 `sxd-xpath` panic（rc=101）；由 `tests/agent.rs` 覆盖，本清单不重复该输入。")
    add("- **命名空间文档已在 0.3.0 纠正**：README 与 `--help` 明确 URI 参与匹配；用例 33–35 证明正确 URI 能匹配默认命名空间及不同前缀的等价文档、错 URI 零匹配。当前契约以 `README.md` 与 `docs/cli-design.md` 为准。")
    add("- 本清单只覆盖 `sample/smoke.py` 的 CLI 黑盒用例；Rust 单测/集成测试与 fuzz 不在此列。")
    add("- 校验清单是否过期：`python sample/export_cases.py --check`（CI 已接入）。")
    add("")
    return "\n".join(out) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail if CASES.md is stale")
    options = parser.parse_args()

    cases = load_cases()
    text = render(cases)
    if options.check:
        current = OUT.read_text(encoding="utf-8") if OUT.exists() else None
        if current != text:
            print("sample/CASES.md is stale; run `python sample/export_cases.py`", file=sys.stderr)
            return 1
        print(f"sample/CASES.md is up to date ({len(cases)} cases)")
        return 0
    with OUT.open("w", encoding="utf-8", newline="\n") as fh:
        fh.write(text)
    print(f"wrote {OUT} ({len(cases)} cases, {len(text.splitlines())} lines)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
