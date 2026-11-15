# ux 抽取配方

## 约定

- `-e` 始终作为**一个带引号的参数**；PowerShell 与 bash 都用单引号最省事。
- 示例中的文件名按实际输入替换。先跑 `ux --list` 确认输入集。
- Agent / CI 场景统一加 `--error-format json`；无人值守再加 `--no-stdin`（此时必须给出文件路径）。
- Windows Git Bash / MSYS2 执行含 `//` 的 XPath 前设 `MSYS2_ARG_CONV_EXCL='*'`，或直接用 PowerShell。

## JSON / JSONL

```powershell
# 取字段
ux --error-format json -O text -e 'jq(.id)' data.json

# 嵌套数组展开 + 重整形
ux --error-format json -e 'jq(.items[].msg) | re(r"user=(?<id>\d+)") | jq({id})' data.json

# JSONL 过滤（显式 -f，避免 auto 误判）
ux --error-format json -f jsonl -e 'jq(select(.level == "ERROR"))' app.jsonl

# 多文档 YAML / TOML
ux -f yaml -e 'jq(.name)' config.yaml
ux -f toml -e 'jq(.database.host)' config.toml
```

## 文本 / 日志 / 正则

```powershell
# 逐行正则命名捕获
Get-Content app.log -Encoding utf8 | ux -f text -O text -e 're(r"ERROR: (?<message>.*)") | jq(.message)'

# 长驻管道：逐条 flush，避免误判无输出
tail -f app.log | ux --line-buffered -f text -O text -e 're(r"ERROR: (?<m>.*)") | jq(.m)'

# raw 文本按分隔符切分再匹配
ux -f text --text-mode raw -e 'split(r"\r?\n----------------\r?\n") | re(r"(?<id>\d{6})")' report.txt
```

## CSV / TSV

```powershell
# CSV 字段一律字符串；数值比较前先转换（把 id 换成实际列名）
ux -f csv -e 'jq(.id |= tonumber) | jq(select(.id > 1))' rows.csv
ux -f tsv -O text -e 'jq(.name)' rows.tsv
```

## XML / HTML

```powershell
# 命名空间的 URI 参与匹配，表达式前缀可与文档前缀不同
ux --ns b=urn:books -f xml -O text -e 'xpath("//b:book") | attr("sku")' books.xml

# HTML 链接 → 属性 → 正则 → 取值
ux -e 'xpath("//a") | attr("href") | re(r"^https://(?<host>[^/]+)") | jq(.host)' page.html
```

HTML 容错树会插入隐式 `<tbody>` 并把标签/属性名折叠为小写；`//table/tr`、`//A` 可能静默零匹配，按实际 DOM 写表达式。

## 目录 / 多文件 / 聚合

```powershell
# 先预演
ux --list .\logs\ --exclude '*.tmp'

# 递归 + 模糊过滤 + 跳过不兼容文件
ux --fuzzy prodconfig --skip-incompatible -e 'jq(.database.host)' .

# 全局聚合（放弃流式，内存与记录数成正比）
ux -f jsonl -O text -e 'slurp() | jq(length)' jobs.jsonl more.jsonl

# 限制结果数：触顶是错误而非截断
ux --max-results 100 -e 'jq(.id)' big.jsonl
```

## 溯源

```powershell
# --with-source 不能与 -O text 组合
ux -f jsonl --with-source -e 'jq(.id)' events.jsonl
```

输出为 `{value, source:{path,line,record}}`；用 `--path-spec` 时 `source` 还会带 `captures`（只读字符串，DSL 不可引用）。

## 结构化路径筛选

```powershell
ux --path-spec 'data/{ts:re:\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}}_SRRG/render_service/{mins:re:\d+}min/hidumper/txt/{file:glob:*.txt}' --list
```

完整语法与陷阱见 `references/path-spec.md`。

## 编码与非 UTF-8

```powershell
ux --encoding gb18030 -f text -O text -e 're(r"ERROR: (?<m>.*)") | jq(.m)' legacy.log
ux --encoding utf-16le -f text -e 'jq(.)' utf16.txt
```

不指定时 `auto` 回退会在 stderr 输出 `assumed GB18030` warning；要求确定性时显式传 `--encoding`。

## stdin 与无人值守

```powershell
# 显式 - 声明 stdin，避免终端挂起
ux -f jsonl -O text -e 'jq(.id)' -

# 无人值守：完全拒绝 stdin，必须给路径
ux --no-stdin --error-format json --quiet -e 'jq(.id)' .\data\

# 结构化错误驱动的调用骨架
ux --list .\data\
ux --quiet --error-format json -f jsonl -e 'jq(select(.level == "ERROR"))' .\data\
```

按 `code` 分支重试：`usage` / `query` 改参数或语法，`runtime` / `io` / `eval` 查数据，`limit_exceeded` 提高上限或收窄输入。

## 组合建议

| 目标 | 组合 |
|---|---|
| 先确认再执行 | `ux --list` → `ux --error-format json ...` |
| 大输入 | `-O jsonl` + 显式 `-f` + `--line-buffered`（若为管道） |
| 需要来源 | `--with-source` + `-O jsonl` / `-O json` |
| 路径带语义 | `--path-spec` + `--list` 预演 |
| 完整性优先 | 文件输入，不用管道；不用 `--quiet` 掩盖 warning |
| 不允许部分结果 | 加 `--strict` |
