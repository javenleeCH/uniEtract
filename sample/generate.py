#!/usr/bin/env python3
"""Regenerate every fixture under sample/.

Run from anywhere:  python sample/generate.py

The fixtures are committed so that CLI tests never need a generator at runtime.
This script exists because some of them cannot be maintained in a text editor:

  * edge/gbk.txt          GB18030 bytes
  * edge/utf16le.txt      UTF-16LE with BOM
  * edge/utf16be.xml      UTF-16BE with BOM and encoding="UTF-16" declaration
  * edge/crlf.log         CRLF line endings
  * edge/bom-utf8.jsonl   UTF-8 BOM
  * edge/binary.bin       invalid UTF-8 bytes (encoding error, not binary skip)
  * edge/nul-bytes.bin    pure ASCII plus NUL bytes (binary detection)
  * edge/empty.jsonl      zero bytes
  * edge/long-line.txt    one 4096-byte line for --max-record-size

Do not hand-edit those with a tool that normalizes newlines or encoding.
All writes go through write_bytes(), so no platform CRLF translation happens.
"""

from pathlib import Path

ROOT = Path(__file__).resolve().parent


def put(rel, data, encoding="utf-8"):
    path = ROOT / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    if isinstance(data, str):
        data = data.encode(encoding)
    path.write_bytes(data)


# ---------------------------------------------------------------------------
# basic/: one canonical document per input format (all UTF-8, LF endings)
# ---------------------------------------------------------------------------
BASIC = {
    "basic/users.json": """{
  "team": "core",
  "users": [
    {"id": 1, "name": "alice", "msg": "user=42"},
    {"id": 2, "name": "bob", "msg": "user=7"},
    {"id": 3, "name": "carol", "msg": "user=99"}
  ]
}
""",
    "basic/events.jsonl": """{"n":1,"level":"INFO"}
{"n":2,"level":"ERROR"}
{"n":3,"level":"INFO"}
""",
    "basic/config.yaml": """database:
  host: db.internal
  port: 5432
features:
  - search
  - export
""",
    "basic/config.toml": """title = "sample"

[server]
host = "127.0.0.1"
port = 8080
""",
    "basic/books.xml": """<?xml version="1.0"?>
<root xmlns:b="urn:books">
  <b:book sku="ABC-123"><b:title>Rust in Action</b:title></b:book>
  <b:book sku="DEF-456"><b:title>Programming Rust</b:title></b:book>
</root>
""",
    "basic/page.html": """<!DOCTYPE html>
<html><body>
<a href="https://example.com/x">x</a>
<a href="https://example.org/y">y</a>
<div id="main">hello</div>
</body></html>
""",
    "basic/jobs.csv": """id,status,error
1,ok,
2,failed,disk full
3,failed,"disk, full"
""",
    "basic/jobs.tsv": """id\tname\tscore
1\talice\t90
2\tbob\t85
""",
    "basic/app.log": """INFO service started
ERROR: disk full
INFO retry queued
ERROR: connection reset
""",
    "basic/notes.txt": """第一行：中文
second line
third line
""",
}

# ---------------------------------------------------------------------------
# edge/: encoding, BOM, size limits, malformed input, HTML parser quirks
# ---------------------------------------------------------------------------
EDGE_TEXT = {
    "edge/blank-lines.jsonl": '{"n":1}\n\n{"n":2}\n\n',
    "edge/big-integer.json": '{"n":123456789012345678901234567890,"small":1}\n',
    "edge/multi-doc.yaml": """---
name: alice
role: admin
---
name: bob
role: editor
""",
    # Written as a single-quoted literal: the CSV field ends with `"""`,
    # which would terminate a Python triple-quoted string.
    "edge/csv-quotes.csv": 'id,note\n1,"contains, comma"\n2,"say ""hi"""\n',
    "edge/malformed.xml": "<root><broken></root>\n",
    "edge/default-ns.xml": '<root xmlns="urn:books"><book sku="NS-1"/></root>\n',
    "edge/prefixed.xml": '<root xmlns:x="urn:books"><x:book sku="PRE-1"/></root>\n',
    "edge/html-tbody.html": "<table><tr><td>x</td></tr></table>\n",
    "edge/html-template.html": '<template><p id="t">inside</p></template><p id="o">outside</p>\n',
    "edge/html-noscript.html": '<noscript><p id="n">inside</p></noscript><p id="o">outside</p>\n',
    "edge/html-uppercase.html": '<DIV ID="upper"><A HREF="/u">x</A></DIV>\n',
    "edge/long-line.txt": "x" * 4096 + "\n",
    "edge/no-extension-jsonl": '{"n":1}\n{"n":2}\n',
    "edge/split.txt": """id=123456
----------------
id=654321
""",
}

# ---------------------------------------------------------------------------
# discover/: deterministic ordering, default-ignore pruning, glob, fuzzy
# ---------------------------------------------------------------------------
DISCOVER = {
    "discover/alpha.json": '{"n":1}\n',
    "discover/beta.yaml": "name: beta\n",
    "discover/gamma.txt": "plain noise\n",
    "discover/configs/prodconfig.yaml": """database:
  host: db.prod
""",
    "discover/configs/dev.yaml": """database:
  host: db.dev
""",
    "discover/node_modules/noise.json": '{"n":99}\n',
    "discover/target/noise.json": '{"n":98}\n',
    "discover/reports/2026-01.csv": "id,total\n1,10\n",
    "discover/reports/2026-02.csv": "id,total\n2,20\n",
}


def main():
    for group in (BASIC, EDGE_TEXT, DISCOVER):
        for rel, text in group.items():
            put(rel, text)

    put("edge/bom-utf8.jsonl", b'\xef\xbb\xbf{"n":1}\n{"n":2}\n')
    put("edge/gbk.txt", "中文 GBK 样本\n错误: 磁盘满\n".encode("gb18030"))
    put("edge/utf16le.txt", b"\xff\xfe" + "UTF-16LE 文本\n第二行\n".encode("utf-16-le"))
    put(
        "edge/utf16be.xml",
        b"\xfe\xff"
        + '<?xml version="1.0" encoding="UTF-16"?>\n'
        '<root><item id="i1">值</item></root>\n'.encode("utf-16-be"),
    )
    put("edge/crlf.log", b"INFO ok\r\nERROR: crlf\r\n")
    put("edge/empty.jsonl", b"")
    put("edge/binary.bin", bytes([0x89, 0x50, 0x4E, 0x47, 0x00, 0x01, 0x02, 0x03,
                                  0x00, 0xFF, 0x00, 0x1A]) * 8)
    put("edge/nul-bytes.bin", b"plain\x00binary\n")

    files = sorted(p for p in ROOT.rglob("*") if p.is_file() and p.suffix != ".py")
    print(f"generated {len(files)} fixtures under {ROOT}")
    for path in files:
        print(f"  {path.relative_to(ROOT).as_posix():46s} {path.stat().st_size:6d} B")


if __name__ == "__main__":
    main()
