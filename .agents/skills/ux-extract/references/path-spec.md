# `--path-spec` 参考

## 何时使用

路径本身携带语义时使用（时间戳目录、服务名、固定子路径、文件类型）。`--path-spec` 与位置参数 `PATH|GLOB` 互斥，以当前工作目录为锚，采用**段级剪枝**：不匹配的中间目录根本不会被进入，而不是先递归整棵树再过滤。

**不要**用它做跨段通配、提取提示（hint）。排序可用 `--sort-by CAPTURE[:num]`；captures 默认只读，需 `captures()` 阶段显式注入才能参与求值。

## 语法

spec 是 `/` 分隔的段序列，每段由字面片段与占位符组成。占位符只有两种显式形式：

| 写法 | 语义 |
|---|---|
| `{name:re:...}` | Rust 正则片段，拼进 `(?P<name>...)` |
| `{name:glob:*.txt}` | 单段 glob：`*` → `[^/]*`、`?` → `[^/]` |

```
data/{ts:re:\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}}_SRRG/render_service/{mins:re:\d+}min/hidumper/txt/{file:glob:*.txt}
```

- 捕获名必须是合法正则组名且全局唯一；重名（含嵌套组与显式占位跨段撞名）在编译期拒绝。
- 段数 ≤ 64，captures ≤ 32（嵌套命名组计入捕获预算）。
- 转义只支持 `\{` / `\}` / `\\`；其他 `\X` 保留为字面两字符。
- 目录结尾的 spec **不会**隐式追加 `**`；末段按文件段匹配。必须显式写出 `/{file:glob:*.txt}`。

## 编译期规则

以下全部在**打开任何目录之前**返回 `Usage(2)`，因此 `--list` 也能安全试错：

| 输入 | 结果 |
|---|---|
| `time:` / `lit:` / 简写占位 / 未知 kind | 拒绝。0.3.1 只接受 `re:` 与 `glob:` |
| `{%Y-%m-%d}`、`{\d+}` 等简写 | 拒绝 |
| `glob` 占位含字符类 `[...]` 或字面 `]` | 拒绝，改用 `re:` |
| `re:` 片段含顶层 `\|` 或不配平括号 | 拒绝，避免片段逃逸捕获组、令剪枝静默失效 |
| 空段、重复捕获名、段数/捕获数超限 | 拒绝 |
| `--path-spec` 与位置参数并存 | clap `conflicts_with` → `Usage(2)` |

`re:` 片段中嵌套在 `(?:...)` 内的 alternation 合法；`\d{4}` 量词不参与括号计数；字符类以 `]` 开头（`[]a]`）按字面成员处理。**不做日历或时钟合法性校验**：`2026-02-30` 是否匹配完全取决于调用方写的正则片段。

## 遍历与剪枝

- 按层维护 frontier，每个节点携带当前目录、已匹配段序号、前缀 parts 与已收集 captures。
- 只把当前段正则命中的条目推进到下一层；中间段要求 `is_dir()`，末段要求 `is_file()`。
- 结果默认按归一化相对路径字典序排序，与 `--list` 及实际处理顺序一致；`--sort-by CAPTURE[:num]` 可改为按命名捕获排序（`CAPTURE` 字符串，`CAPTURE:num` 数值，修复 `10min` 排在 `2min` 前的错序）。
- 可与 `--exclude`、`--fuzzy`（后置过滤）、`--no-default-ignore`、`--sort-by` 组合。
- `--max-depth` 在该模式下被忽略并输出一条 warning；深度由 spec 段数决定。
- frontier 按层展开、不设节点数上限；用窄片段（具体目录名、受约束的 `re:`）控制扇出。

## captures

`capture_json()` 返回纯字符串 `BTreeMap`：
- 0.4.0 起，`re:` 片段内的嵌套命名组也是捕获：`{file:re:.*(?<ts>\d{8}_\d{6})_(?<seq>\d+)_hidumper\.txt}` 产出 `file` / `ts` / `seq` 三个键。0.3.1 会静默丢弃内层组——升级前写的 spec 可能突然多出键，消费端按名字取值不受影响。
- 键按名字**字典序**输出，不是 spec 中的出现顺序。
- 只出现在 `--list` 的输入清单与 `--with-source` 的 `source.captures` 里。
- **默认不参与流水线求值**：DSL、`jq`、`xpath` 都拿不到；不生成 `ts_fields` 之类派生字段。需要时用 `captures()` 阶段显式注入（如 `... | captures() | jq(.ts)`）。
- 需要时间语义过滤时，把有效性写进 `re:` 片段，或把 `--with-source` 的 JSONL 交给外部工具解析 `source.captures.ts` 字符串。

## 完整示例：NativeHeap 场景

```powershell
# 1. 预演：确认命中哪些文件，并查看 captures
ux --path-spec 'data/{ts:re:\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}}_SRRG/render_service/{mins:re:\d+}min/hidumper/txt/{file:glob:*.txt}' --list

# 2. 正式抽取：从命中的 dump 里取 NativeHeap pss，并带溯源
ux --error-format json --path-spec 'data/{ts:re:\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}}_SRRG/render_service/{mins:re:\d+}min/hidumper/txt/{file:glob:*.txt}' -f text -O jsonl --with-source -e 're(r"NativeHeap pss:\s*(?<pss>\d+)") | jq({pss})'
```

`--with-source` 的输出形如 `{"source":{"captures":{...},"line":5,"path":"...","record":5},"value":{"pss":"12345"}}`：captures 在 `source` 里，按名字字典序，值一律字符串。

上面的 `ts` 片段是宽松的 `\d{4}-\d{2}-\d{2}`，会命中 `2026-02-30` 这类写法。排除非法日期时把有效性写进片段，例如用 `(?:0[13578]|1[02])` 之类的分组枚举，**不要**依赖内置的日历校验（不存在）。

## 诊断

三类遍历异常统一经 `Reporter` 输出 warning，格式 `kind=<kind> path=...: <error>`：

| kind | 触发点 |
|---|---|
| `read_dir` | `fs::read_dir` 打开目录失败，跳过该目录 |
| `read_dir_entry` | `ReadDir` 迭代单条目失败 |
| `file_type` | `entry.file_type()` 失败 |

文本模式为 `ux: warning: ...`；`--error-format json` 为 `{"code":"warning","message":...,"exit_code":0,"category":"result_set"}`；这三类告警是结果影响型，**不受 `--quiet` 压制**（丢文件不可静默）。

零匹配时退出 0，stderr 一行 `no input files matched`。

## 陷阱清单

- 忘记写末段文件占位 → 目录结尾 spec 不会自动递归，结果恒为零匹配。
- 用 `time:` / `{%Y}` 等旧语法 → `Usage(2)`；用 `re:` 显式表达。
- 期望 `source.captures` 能在 `jq` 里直接引用 → 默认不行，captures 是只读元数据；需显式 `captures()` 阶段注入后才能引用。
- 在 `glob` 占位里写 `[0-9]` → 字符类被拒绝，改用 `re:`。
- 在 `re:` 片段里写顶层 `a|b` → 被拒绝；改写成 `(?:a|b)`。
- 加 `--max-depth` 以为能限制遍历 → 只会得到一条忽略告警。
- 依赖捕获键顺序 → 实际按字典序。
