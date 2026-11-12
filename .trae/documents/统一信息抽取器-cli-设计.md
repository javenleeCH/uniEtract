# UniExtract 统一信息抽取 CLI：设计与实施计划（v2）

## 0. 修订说明

### 0.1 基线变更

v1 是"工作区为空、从零实现"的设计稿。但仓库当前已有 v0.1 实现（`0e6ec2a`，15 个源文件，`ux` 二进制可构建，单元测试覆盖 jq/regex/xpath/markup/output）。因此 v2 不再是空想设计，而是三件事：

1. **契约修正**：把 v1 里自相矛盾、写错、以及实现时证明不可行的条款改对；
2. **现状台账**：列出代码与 v1 契约的真实偏差（含一处"文档承诺、代码没有"的死开关）；
3. **增量计划**：只描述尚未完成的工作，已实现的部分标注为"已有"。

### 0.2 修订索引

| 编号 | 修订内容 | 章节 |
|---|---|---|
| R1 | 新增 `slurp()` / `split()` 阶段：v1 严格 per-item，导致 JSONL/CSV 多文件场景无法做任何跨记录计算 | §1.3 §1.5 |
| R2 | 明确"Node 不跨文档"硬不变量与聚合物化边界（`Item<'doc>` 保留，不引入 arena） | §1.1 §1.3 |
| R3 | `Stage` 存 `String` 与"输入前编译"互斥：拆为 `StageSpec` + `CompiledStage` | §1.2 |
| R4 | 混合格式下"类型错误即失败"与"0 输出即过滤"冲突：新增 `--skip-incompatible` | §1.4 §2.1 |
| R5 | `text()` 表格里"Data 字符串去引号"是笔误，改为"原样输出" | §1.2 |
| R6 | `jq()` 输出为 JSON 字符串时应回落为 `Text`（v1 写成"每个输出都是 Data"，与实现不符且丢失文本语义） | §1.2 |
| R7 | `re()` 的"有无捕获组决定输出类型"从隐性行为升为显式契约，并标注 `regex` crate 无回溯引用/无 lookaround | §1.5 |
| R8 | XPath 命名空间：`sxd-document` 不建模 namespace URI，`--ns` 只是表达式前缀解析，等价文档不同前缀/默认命名空间匹配不上。降级为已知限制并给出验证用例 | §3.3 |
| R9 | 依赖从"待验证"改为"已落地 + 版本锁定规则"（jaq 三件套 3.1.2/3.0.4/2.0.3，`sxd_html` 0.1.3），并新增数字精度规则 | §3.2 §3.3 |
| R10 | 新增输入卫生：BOM 剥离、`--encoding`、目录递归默认忽略表、二进制跳过、`--max-input-size` | §2.3 §2.4 |
| R11 | 输出契约：统一 LF、`-O json` 默认缩进 + `--compact`、`--with-source` 与 `-O text` 互斥报错 | §2.5 |
| R12 | 阶段划分改为按剩余缺口排序（聚合 → 发现 → 输入卫生 → 输出收口 → 兼容矩阵/加固） | §4 |

### 0.3 现状与契约的偏差台账

| 偏差 | 现状 | 目标 | 影响 |
|---|---|---|---|
| `--fuzzy` 死开关 | `cli.rs` 定义并解析，`lib.rs` 与任何模块均未使用 | 按 §2.2 生效或先删除 | 用户以为在过滤，实际全量输入，静默错误 |
| 文件发现缺失 | 无 `discover.rs`；`glob`/`walkdir`/`nucleo-matcher` 是未使用依赖；路径只走 `File::open` | §2.2 完整实现 | 目录、glob、递归全部不可用，v1 §4 阶段 4 实际未做 |
| 无聚合能力 | 无 `slurp`/`split`；`-O json` 仅聚合输出 | §1.3 | JSONL/CSV 无法求总数、去重、分组 |
| 无 BOM 处理 | `read_to_string` 直读；`lines()` 模式首行带 BOM | §2.3 | 首条记录/CSV 首列表头被污染，正则首匹配失效 |
| 仅 UTF-8 | 非 UTF-8 直接报错，无编码选项 | §2.3 | GBK 日志/CSV 不可用 |
| 无二进制防护 | 递归（未来）或显式传 `.png` 会按文本读 | §2.4 | 输出二进制垃圾或每文件一条错误 |
| `-O json` 紧凑单行 | `serde_json::to_vec` | §2.5 | 大数组人不可读 |
| `--with-source` 覆盖文本模式 | `output.rs` 中 `with_source` 优先于 `-O text`，静默改输出形态 | §2.5 | 参数组合语义不明，应为参数错误 |
| `jq` 结果经字符串回环 | `Val::to_string()` 后 `from_str` | 保留，但 §3.3 明确精度与 NaN 规则 | 大整数/NaN 需有测试兜底 |

---

## 1. 核心设计：统一记录流

### 1.1 数据模型（保留借用模型 + 一条硬不变量）

```rust
pub struct Item<'doc> {
    pub value: Value<'doc>,
    pub origin: Origin,
}

pub enum Value<'doc> {
    Data(serde_json::Value),
    Text(String),
    Node(sxd_xpath::nodeset::Node<'doc>),
}
```

v1 曾考虑 arena + 稳定索引来消灭 lifetime 参数。**结论：不需要。** 现有实现（`markup::process` 持有 `Package`、`Item<'doc>` 只在文档作用域内流通）已经让借用检查器替你保证了正确性，引入 arena 只会增加一次回查开销和一套索引协议。

但要把它写成显式不变量，因为后续新阶段会反复踩：

> **I1（Node 不跨文档）**：`Node` 只能在同一 `Package` 的生命周期内使用。任何可能**同时持有多条来源不同记录的阶段**（聚合、缓冲、`-O json` 之外的跨文件缓冲）**必须在边界处物化**：`Node → node.string_value()`，产出 `Text` 或 `Data`。
>
> 该不变量由编译器强制（`Package` 是局部变量），不需要运行时断言；但每个新增的"收集型"阶段都必须遵守，否则编译期报错，届时不要用 `unsafe` 或 `transmute` 绕过。

`Origin` 三字段的填充规则必须列表化，否则 `--with-source` 的 golden 测试会一直漂：

| 输入 | `path` | `line` | `record` |
|---|---|---|---|
| JSON / YAML(单文档) / TOML / XML / HTML | 文件路径 | `1` | `1` |
| YAML 多文档 | 文件路径 | 该文档起始行（不可得时为 `1`） | 文档序号（1 起） |
| JSONL | 文件路径 | 该行行号 | 非空记录序号（1 起） |
| text lines | 文件路径 | 该行行号 | 同 `line` |
| text raw | 文件路径 | `1` | `1` |
| CSV/TSV | 文件路径 | 记录起始行（`csv` 的 `position().line()`） | 记录序号（1 起，不含表头） |
| stdin | `null` | 同对应格式 | 同对应格式 |

### 1.2 流水线语义

`Stage` 拆成两层：**解析产物**（`StageSpec`，持有原始字符串）与**编译产物**（`CompiledStage`，持有编译后的选择器）。v1 把 `Stage` 定义为持有 `String` 的枚举，却又要求"输入前编译"，两者互斥。

```rust
pub enum StageSpec { Jq(String), XPath(String), Regex(String), Text, Attr(String), Slurp, Split(String) }

pub enum CompiledStage {
    Jq(jaq_core::Filter<...>),
    XPath(sxd_xpath::XPath),
    Regex(regex::Regex),
    Text,
    Attr(String),
    Slurp,
    Split(regex::Regex), // 复用正则做切分，避免第二套语法
}
```

- `Pipeline::compile()` 完成全部编译；任一失败即 `AppError::Query` → 退出码 2，**早于任何输入读取**。
- 每个阶段接收一个 `Item`，同步产出 `0..N` 个 `Item`：0 表示过滤，N 表示展开。
- 除 `Slurp` 与 `-O json` 外不跨记录收集。

类型规则（修正 R5、R6）：

| 阶段 | 接受类型 | 输出与规则 |
|---|---|---|
| `jq(filter)` | `Data`、`Text` | `Text` 作为 JSON 字符串输入；输出为 JSON 字符串时回落为 `Text`，其余为 `Data` |
| `xpath(expr)` | `Node` | nodeset 按文档顺序展开为 `Node`；字符串→`Text`；数字/布尔→`Data` |
| `re(pattern)` | `Text` | 无捕获组：每个非重叠匹配产出完整匹配 `Text`；有捕获组：每次匹配产出一个 `Data` 对象 |
| `text()` | 所有类型 | `Node`→string-value；`Text`→原样；`Data` 字符串→原样；其余→紧凑 JSON |
| `attr(name)` | 元素 `Node` | 属性值 `Text`；属性缺失产出 0 个结果；非元素报类型错误 |
| `slurp()` | 任意（物化） | 上游全部结果收敛为一个 `Data` 数组（见 §1.3） |
| `split(pattern)` | `Text` | 按正则切分成多个 `Text` |

不做隐式 Node→JSON、JSON 文本自动解析。需要时用 `jq` 显式处理。

### 1.3 slurp 与聚合边界（新增，R1/R2）

没有 `slurp` 时，"统一流水线"在统计类用例上是残的：`-f jsonl`、`-f csv`、多文件输入下永远算不出总数、去重、分组、最大值——因为 jq 用户一半的价值来自 `jq -s`，而 `-O json` 只聚合**输出**，不参与计算。

契约：

```
slurp()                    # 上游所有 Item → 单个 Data 数组
slurp() | jq(length)       # 例：统计条数
```

- **物化**：`slurp` 是类型边界。`Node` 在此转 `node.string_value()`（`Text`），`Text` 转 JSON 字符串，`Data` 原样；结果为一个 `Value::Data(Array)`。
- **来源**：slurp 后 `Origin` 折叠为上游第一条的 `Origin`；`--with-source` 只包外层，不保留每条来源（如需逐条来源，应先用 `--with-source` 之外的路径，或在 jq 里自行拼）。
- **流式终止**：`slurp()` 之后流水线不再逐条输出，内存与上游总记录数成正比；必须在 `--help` 与 README 明确。
- **`split(pattern)`** 与 slurp 对称：`--text-mode raw` 读完整个文件后，除跨行正则外没有别的切分手段，raw 模式价值被削掉一半。`split` 用 `regex::Regex` 切分，空片段丢弃。

### 1.4 交叉格式的类型策略（新增，R4）

v1 规定"类型错误 = 失败"，同时又规定"0 输出 = 过滤"，两者在混合输入时打架：

```powershell
ux -e 'xpath("//a") | attr("href")' *.json *.xml    # JSON 文件必然类型错误 → 整轮退出 1
```

契约：

- **默认**：类型错误仍是错误（不静默），退出码 1。宁可吵，不可骗。
- **`--skip-incompatible`**：若某文件的**源类型**与该流水线的首个阶段不兼容（`xpath`/`attr` 需要 `Node`，`re` 需要 `Text`），整文件跳过，向 stderr 记一条 warning，**不计入** `had_errors`。阶段链中段才发生的类型错误不算"不兼容"，仍按失败处理。
- 实现方式：`CompiledStage::accepts_source(InputFormat) -> bool` 做静态判定，不靠试运行。

### 1.5 DSL MVP（修正 R7）

```text
pipeline := stage ("|" stage)*
stage    := jq(FILTER)
          | xpath(STRING)
          | re(STRING)
          | split(STRING)
          | text()
          | attr(STRING)
          | slurp()
```

解析器只在括号深度为 0 且不在字符串内时识别管道符，因此 jq 自身的 `.a | .b` 不会被误拆（已有实现，`dsl.rs` 的单测覆盖了嵌套括号与原始字符串）。支持普通转义字符串（`\n \r \t \\ \' \"` 六种，其余报错并提示用原始字符串）与 `r"..."` / `r#"..."#`。

**`re()` 的类型多态要显式写在契约里**（R7）：

- 同一条 `re(...) | jq(...)`，加一个捕获组就从"`Text` 流"变成"对象流"，`jq(.x)` 从类型错误变为可用。这是**有意为之**，不是 bug，必须写进文档与 `--help`。
- 输出对象同时含数字键与命名键（`{"0":..., "1":..., "id":...}`），便于 `jq(.id)` 与 `jq(."1")` 两种访问。副作用是 `jq(keys)` 会多出数字键，需在文档提示。
- `regex` crate **无回溯引用、无 lookaround**。日志抽取会频繁撞上，文档给一句指引："需要断言/回溯时，先用 `re` 粗切再用 `jq` 二次处理"。

### 1.6 示例

```powershell
# JSON 内抽取 → 正则命名捕获 → 重整形
ux -e 'jq(.items[].msg) | re(r"user=(?<id>\d+)") | jq({id})' a.json

# HTML 链接 → 属性 → 正则 → 取值
ux -e 'xpath("//a") | attr("href") | re(r"^https://(?<host>[^/]+)") | jq(.host)' page.html

# XML 命名空间（见 §3.3 限制：前缀需与文档一致）
ux --ns b=urn:books -e 'xpath("//b:book") | text() | re(r"(?<sku>[A-Z]+-\d+)")' books.xml

# 日志逐行流式抽取
Get-Content app.log -Encoding utf8 | ux -f text -O text -e 're(r"ERROR: (?<message>.*)") | jq(.message)'

# 新增：JSONL 聚合统计（slurp）
ux -f jsonl -O text -e 'slurp() | jq(length)' jobs.jsonl

# 新增：raw 文本先切分再抽字段
ux -f text --text-mode raw -e 'split(r"\r?\n----------------\r?\n") | re(r"(?<id>\d{6})")' report.txt

# CSV 过滤投影
ux -f csv -e 'jq(select(.status == "failed") | {id, error})' jobs.csv

# 目录递归 + 模糊过滤 + 跳过不兼容文件
ux --fuzzy prodconfig --skip-incompatible -e 'jq(.database.host)' .
```

---

## 2. CLI 契约

### 2.1 参数

```text
ux -e PIPELINE
   [-f auto|json|jsonl|yaml|toml|xml|html|text|csv|tsv]     # 默认 auto
   [-O jsonl|text|json]                                     # 默认 jsonl
   [--compact]                                              # -O json 关闭缩进
   [--text-mode lines|raw]                                  # 默认 lines
   [--ns PREFIX=URI]...                                     # 可重复
   [--fuzzy QUERY] [--max-depth N]
   [--exclude GLOB]...                                      # 可重复
   [--skip-incompatible]
   [--encoding utf-8|gbk|gb18030|auto]                      # 默认 utf-8
   [--max-input-size SIZE]                                  # 默认 256MiB，0 = 不限
   [--max-results N]                                        # 默认 0 = 不限
   [--with-source] [--strict]
   [PATH|GLOB|-]...
```

参数校验（全部在读取输入前完成，失败退出 2）：

- `--with-source` 与 `-O text` 互斥（v0.1 是静默改输出形态，改为明确报错）。
- `--encoding` 与 `-f` 不矛盾，但与 `--text-mode raw` 之外的模式无关，无需报错。
- `--compact` 仅对 `-O json` 生效，其余模式忽略。
- `-` 最多出现一次（已有实现）。

### 2.2 输入与文件发现（v0.1 未实现，本版为待实现契约）

- 无路径：读 stdin；`-` 显式表示 stdin。
- 显式文件：直接输入。
- 目录：`walkdir` 递归，`filter_entry` 剪枝；默认忽略 `.git`、`.hg`、`.svn`、`target`、`node_modules`、`.venv`、`__pycache__`、`.trae`。隐藏文件**不**默认忽略（`.env`、`.npmrc` 是常见抽取目标），由 `--exclude` 控制。
- glob：`*`、`?`、字符类、`**`；展开后按规范化绝对路径去重，输出顺序按**相对路径字典序**稳定排序。
- `--fuzzy QUERY`：对规范化相对路径用 `nucleo-matcher` 打分过滤，同分按字典序，保证可复现。必须明确它是**后置过滤器**：先全量枚举再过滤，不做目录剪枝；`--max-depth` 提供逃逸阀。评分对版本敏感，版本在 `Cargo.toml` 锁定。
- 打开文件用**原始 `Path`**，绝不用模糊匹配后的字符串反推路径（避免跨平台分隔符与大小写问题）。

### 2.3 编码与 BOM（新增，R10）

- 默认 `--encoding utf-8`：严格 UTF-8，非法字节 → 该文件失败并给出字节偏移。
- `--encoding auto`：按 BOM 判定（UTF-8/UTF-16LE/BE）→ 严格 UTF-8 → 回退 GB18030；成功回退时向 stderr 打印"`path`: assumed GB18030（用于可复现）"。
- `--encoding gbk|gb18030`：显式指定，用 `encoding_rs`（纯 Rust）解码。
- **BOM 必须剥离**：`lines` 模式下 BOM 会污染首条记录（`re(r"^\d+")` 直接全灭），CSV 会污染首列表头。剥离在读取层统一做，对 `raw` 模式同样剥离（BOM 不是内容）。
- 行终止符：`lines` 模式剥离行尾 `\r`（兼容 CRLF）；`raw` 模式保留原始内容。

### 2.4 二进制与体积防护（新增，R10）

- 目录递归或显式路径遇到二进制：前 4KiB 含 NUL 字节，或非可打印字节占比 > 30%，判定为二进制 → 跳过并记 stderr warning，**不计入**退出码；`--force-text` 可强行按文本处理。
- `--max-input-size` 仅对**整文档解析**的格式生效（json/yaml/toml/xml/html/raw text）；JSONL/text lines/CSV 逐条流式，不受该限制。超限 → 该文件失败，提示当前限制与如何调整。

### 2.5 输出与错误（修正 R11）

- 默认 `-O jsonl`：每结果一行紧凑 JSON；`Text`/`Node` 输出 JSON 字符串（`Node` 取 string-value）。
- `-O text`：`Text` 原样；`Node` 取 string-value；`Data` 字符串原样、其余紧凑 JSON；每结果以 `\n` 结束。
- `-O json`：聚合为一个 JSON 数组，完成后一次输出；**默认缩进 2 空格**，`--compact` 关闭。
- **换行统一 LF**，不做任何平台相关 CR 转换（否则管道到 `grep`/`jq`/`Select-String` 才暴露）。`raw` 模式内容内的原始 `\r` 不处理。
- `--with-source`：每项包为 `{value, source:{path,line,record}}`，缺失字段为 `null`。
- stdout 只写数据；诊断写 stderr，含文件、记录/行号、失败阶段名。
- 错误策略：默认"当前文件首错即停、继续其他文件，最终有任何错误退出 1"；`--strict` 全局首错即停。
- 退出码：`0` 成功（含零匹配）；`1` 输入/解析/求值/输出错误；`2` 参数或 DSL 编译错误。`BrokenPipe` 视为正常终止（已有实现）。
- JSONL/逐行文本/CSV 每产出一个结果即 flush，保证持续输入时 EOF 前可见（已有实现）。

### 2.6 各格式输入语义

| 格式 | 输入项 | 流式边界 |
|---|---|---|
| JSON | 整个文档一个 `Data` | 完整解析，受 `--max-input-size` |
| JSONL/NDJSON | 每个非空行一个 `Data`，保留行号 | 逐行解析并立即输出 |
| YAML | 每个 YAML 文档一个 `Data`；非字符串映射键报错 | 全文读入后逐文档解析 |
| TOML | 根表一个 `Data`；日期时间转字符串；NaN/Inf 报错 | 完整解析 |
| XML | 根节点一个 `Node` | 完整解析，格式错误报错 |
| HTML | 根节点一个 `Node` | `sxd_html` 容错解析，完整构建 DOM |
| text/log | 每行一个 `Text`，或 raw 全文一个 `Text` | lines 逐行 |
| CSV/TSV | 首行表头，每条记录一个字符串字段对象；空/重复表头报错 | 逐记录；字段不做数值推断 |

`-f auto`：先扩展名；stdin/未知扩展名只保守嗅探 JSON（`{`/`[`）与 HTML/XML（`<`），否则按文本。YAML/TOML/CSV/JSONL 在未知扩展名下必须显式指定——格式一旦确定，解析失败即报错，**不降级重试**。

CSV 字段一律字符串：`jq(select(.n > 5))` 会因字符串比较失效，文档必须写明"先 `jq(.n |= tonumber)`"。

---

## 3. 模块与依赖

### 3.1 项目结构（现状；缺 `discover`）

```text
src/main.rs      参数解析、退出码映射
src/lib.rs       编排一次运行（多文件循环、strict、broken pipe）
src/cli.rs       clap 定义与参数校验      ← 待补 §2.1 新增项
src/dsl.rs       StageSpec 解析           ← 待补 slurp/split
src/engine.rs    Pipeline::compile/execute
src/model.rs     Item/Value/Origin
src/stage/       已编译阶段：jq / regex / xpath 适配器
src/input.rs     格式探测、逐格式读取、emit
src/markup.rs    Package 生命周期、XML/HTML 构建
src/output.rs    jsonl/text/json、source 包装、flush
src/discover.rs  ← 完全缺失，本版新增
src/error.rs     带路径/记录/阶段的错误
tests/           ← 目录不存在，端到端测试未落地
```

### 3.2 依赖与锁定（现状）

```toml
jaq-core = "3.1.2"; jaq-std = "3.0.4"; jaq-json = { version = "2.0.3", features = ["serde"] }
serde_json = { version = "1", features = ["arbitrary_precision"] }
sxd-document = "0.3"; sxd-xpath = "0.4"; sxd_html = "0.1.3"
regex = "1"; serde_yaml_ng = "0.10"; toml = "0.9"; csv = "1.3"
winnow = "0.7"; clap = "4.5"; thiserror = "2"
glob = "0.3"; walkdir = "2"; nucleo-matcher = "0.3"   # 已声明但未使用
```

规则：

1. **jaq 三件套跨版本线**（core 3.x / std 3.0.4 / json 2.0.3）由 `Cargo.lock` 锁死；任一方升级必须先跑 §5 的 jq 兼容集，不允许仅 `cargo update` 后提交。
2. `serde_json` 开了 `arbitrary_precision`，数字以原始字面保存。进出 jaq 时的规则必须显式：
   - 进入：超出 `isize` 范围的整数 → **报错**（`cannot be represented`），禁止静默降为 f64；
   - 输出：jaq 打印后重新 `from_str`，保留字面精度；
   - `NaN`/`±Inf` 在 jaq 侧输出为 `null`（与 jq 一致），需有测试固定该行为。
3. `sxd_html = 0.1.3` 在当前 `Cargo.lock` 中解析到 `html5ever 0.38.0`（实证，非 0.1.1 时代的 `^0.26` 线），因此**不需要自研 TreeSink**。残余风险只是"上游 crate 体量小、维护活跃度低"：处理方式是 `Cargo.lock` 固定版本 + 自有容错测试集管住行为，任何升级都必须重跑 §5 的 HTML 用例。

### 3.3 已证实的依赖边界（不再是"待验证"）

v1 把 HTML/XPath 当作最大不确定项。现在已有真实测试跑通，结论写死：

- **HTML 可用**：`//a`、`//script | text()`（`a < b` 不被当标签）、`//svg:circle` 均通过（`markup.rs` 单测）。未覆盖但必须补测：`<template>`、`<noscript>`、隐式 `<tbody>` 对 `//table/tr` 的影响、HTML 元素名大小写折叠、属性名大小写。
- **XPath 命名空间是残的（R8）**：`sxd-document` 的节点名是"含前缀的字符串"，不建模 namespace URI。`Context::set_namespace` 只做表达式前缀解析，因此：
  - 文档用 `xmlns="urn:books"`（默认命名空间）→ `//b:book` 匹配不上；
  - 文档用等价但不同前缀（`xmlns:c="urn:books"`）→ `//b:book` 也匹配不上；
  - 只有文档前缀与 `--ns` 前缀一致时才"看起来能用"。
  → 契约写明"`--ns` 仅解析表达式前缀，不实现 namespace-URI 匹配"，并在 `--help` 给出绕过办法（先按文档实际前缀写表达式）。若未来必须做真正的 URI 匹配，候选替换是 `roxmltree`（正确建模 ns）+ 自研/桥接 XPath，属于 v3 议题，不在本版动。
- **XPath 1.0 完整性**：`sxd-xpath` 的 nodeset/string/number/boolean 四类结果已正确映射；`id()`、`lang()` 等函数未实测，**不要在文档里承诺 XPath 1.0 完整兼容**。

---

## 4. 剩余实施计划

### 阶段 A：聚合能力（R1/R2）

`dsl.rs` 加 `slurp()` / `split(pattern)` 语法；`CompiledStage` 加两个变体；`engine.rs` 需要一条"收集上游再下发"的执行路径（当前 `execute_from` 是纯递归逐条，需引入 `collect` 分支）。

验收：`ux -f jsonl -e 'slurp() | jq(length)'` 在多文件输入下给出总条数；`split` 在 raw 文本上切出正确段数；`slurp` 后接 `xpath` 必须是编译期可判定的类型错误而非运行期 panic。

### 阶段 B：文件发现（R4 一并落地）

新增 `src/discover.rs`：目录递归（`walkdir` + `filter_entry` 剪枝）、glob（`glob`）、fuzzy（`nucleo-matcher`）、去重与稳定排序、`--max-depth`、`--exclude`；`cli.rs` 接上 `--fuzzy`（当前是死开关）；`lib.rs` 改为"发现 → 逐个处理"。

验收：`ux --fuzzy prodconfig .` 只处理命中的 3 个文件；同分排序在两次运行中一致；`.git`/`target` 不被进入（用一个百万级/深度用例验证剪枝生效，而不是先走完再过滤）。

### 阶段 C：输入卫生（R10）

BOM 剥离、`--encoding`（`encoding_rs`）、二进制探测与跳过、`--max-input-size`、CRLF 处理。

验收：带 BOM 的 UTF-8 JSONL 首行可正常抽取；GBK 的日志文件经 `--encoding gbk` 正确输出中文；`ux .` 在含 PNG 的目录里不输出二进制、退出码 0；超限文件给出明确错误而非 OOM。

### 阶段 D：输出与 CLI 收口（R11）

`-O json` 默认缩进 + `--compact`；`--with-source` 与 `-O text` 互斥报错；`--max-results`；LF 不变量测试。

验收：`-O json` 输出人可读；`--with-source -O text` 退出码 2 并提示互斥；`--max-results 10` 在第 11 条时以错误终止而非截断。

### 阶段 E：测试与兼容矩阵

`tests/` 目录落地端到端测试；jq 兼容集从 `jaq-std` 内建函数表**自动生成** `COMPAT.md`（已支持/未支持两栏），不靠人肉维护；`dsl.rs` 的 `split_pipeline` 用 `cargo-fuzz` 打（未闭合原始字符串、深度失衡是首要 crash 面）。

最终命令：

```powershell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

再用 release 二进制逐条跑本文所有示例，断言 stdout、退出码与流式行为。

---

## 5. 验证方案

### 单元测试（已有部分，需补）

- DSL：嵌套括号、jq 内部 `|`、原始字符串、转义白名单、空阶段、未知阶段（**已有**）；`slurp`/`split` 语法与参数错误（待补）。
- 流水线：0..N 过滤/展开、`Origin` 保留、逐阶段类型错误、`slurp` 物化（待补）。
- 正则：完整匹配、数字/命名捕获、未参与捕获、多次匹配、无效表达式（**部分已有**）。
- XPath：文档顺序、原子类型、缺失属性、命名空间（**部分已有**，需补默认命名空间失败用例作为文档化限制的证明）。
- 输出：`Text`/`Data`/`Node`、source 包装、JSON 聚合、LF 不变量（待补）。

### 集成与 CLI golden

覆盖所有输入格式及至少一条跨引擎组合流水线；断言 stdout、stderr、退出码。边界：空输入、零匹配、非法 UTF-8、BOM 输入、坏 JSONL 行、坏 XML、容错 HTML、CSV 空/重复表头、GBK 输入、二进制文件、多文件部分失败、`--skip-incompatible`、stdin 与文件混用、未知扩展名、显式格式冲突、`--max-input-size`/`--max-results` 触发。

### 流式回归

启动真实 `ux` 子进程，向 stdin 写入一条 JSONL/文本/CSV 记录后**保持 stdin 打开**，用带超时的读取通道断言 EOF 前收到输出；关闭 stdin 校验最终结果。`-O json` 与 `slurp` 则断言"输入关闭前无输出"——这是它们放弃流式的可观测证据。

### 兼容性矩阵

- jq：字段/数组迭代、`select`、对象构造、内部管道、多输出、运行时错误、大整数、`NaN`；未支持语法必须真实报错并进 `COMPAT.md`。
- XPath：仅列实际通过的函数与轴。
- HTML：§3.3 列出的未覆盖容错点各一条用例。
