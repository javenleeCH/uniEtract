# UniExtract 统一信息抽取 CLI：设计与实施计划

## Context

当前工作区为空，需要从零实现一个 Rust 单文件 CLI，把 jq、XPath、正则的抽取能力放进同一条数据处理流水线，而不是仅提供三个互不关联的子命令。

已确认的产品决策：

- 使用 Rust，优先纯 Rust 依赖，运行时不调用外部 jq、shell 或本地 C 库。
- 使用组合式管道 DSL；各阶段保留原语法能力，阶段间采用统一数据模型。
- 首版支持 JSON、JSONL、YAML、TOML、XML、HTML、纯文本/日志、CSV/TSV。
- 文件路径支持 glob 和 fzf 风格模糊匹配。
- JSONL、逐行文本、CSV/TSV 真实流式处理；普通 JSON、YAML、TOML、XML、HTML 首版完整解析。聚合 JSON 输出会主动放弃流式输出。

首版明确不承诺 XPath 2.0/3.0 或 libjq 100% 兼容；XPath 以 1.0 为目标，jq 兼容性以所选 jaq 版本的实际能力和兼容测试为准。大型单个 JSON/XML 的增量解析不纳入首版。

## 1. 核心设计：统一记录流，而非统一字符串

### 1.1 数据模型

采用单 crate、库核心加薄 CLI。核心值模型建议为：

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

pub struct Origin {
    pub path: Option<Arc<PathBuf>>,
    pub line: Option<u64>,
    pub record: u64,
}
```

- `Data`：JSON、YAML、TOML、CSV/TSV 归一化后的 null、布尔、数字、字符串、数组、对象。
- `Text`：文本行、原始文本、属性值、正则完整匹配、XPath 字符串结果。
- `Node`：XML/HTML 文档中的元素、属性、文本等节点，保留继续执行 XPath、`text()`、`attr()` 的能力。
- `Origin`：跟随每次 fan-out 复制，用于错误定位和 `--with-source`，默认不混入用户数据。

XML/HTML 的 `sxd_document::Package` 在单文档回调作用域内持有，`Item<'doc>` 只借用节点；在该作用域内编译并执行完整流水线、写出结果后释放文档。不使用 unsafe、自引用结构或将裸节点逃逸到文档之外。

### 1.2 流水线语义

```rust
pub struct Pipeline {
    pub stages: Vec<Stage>,
}

pub enum Stage {
    Jq(String),
    XPath(String),
    Regex(String),
    Text,
    Attr(String),
}
```

- 每个阶段接收一个 `Item`，同步产生 `0..N` 个 `Item`，并立即交给下一阶段。
- 0 个输出表示过滤；N 个输出表示展开。除 `-O json` 聚合输出外，不跨输入记录收集结果。
- 所有选择器在处理输入前编译；DSL、jq、XPath、正则编译错误在读取数据前报告。
- 类型错误包含阶段名称、实际类型、来源路径和记录位置，不静默忽略。

类型规则：

| 阶段 | 接受类型 | 输出与规则 |
|---|---|---|
| `jq(filter)` | `Data`、`Text` | `Text` 作为 JSON 字符串输入；经集中转换层交给 jaq；每个 jq 输出成为一个 `Data` |
| `xpath(expr)` | `Node` | nodeset 按文档顺序展开为 `Node`；字符串转 `Text`；数字/布尔转 `Data` |
| `re(pattern)` | `Text` | 对每个非重叠匹配产生一个结果，规则见下文 |
| `text()` | 所有类型 | Node 取 XPath string-value；Text 不变；Data 字符串去引号，其余转紧凑 JSON |
| `attr(name)` | 元素 Node | 输出属性 Text；属性不存在产生 0 个结果；非元素报类型错误 |

不做隐式 Node 到 JSON、JSON 文本自动解析等魔法转换。后续若增加 `parse("json")`，应作为显式阶段实现。

### 1.3 DSL MVP

```text
pipeline := stage ("|" stage)*
stage    := jq(FILTER)
          | xpath(STRING)
          | re(STRING)
          | text()
          | attr(STRING)
```

解析器只在括号深度为 0 且不在字符串内时识别管道符，因此 jq 自身的 `.a | .b` 不会被误拆。支持普通转义字符串及 `r"..."` 原始字符串；正则推荐原始字符串，避免反斜杠双重转义。

正则结果规则固定且有测试覆盖：

- 无捕获组：每个匹配输出完整匹配 `Text`。
- 有捕获组：每个匹配输出 `Data` 对象；`"0"` 为完整匹配，`"1"..."N"` 为对应捕获组，命名捕获同时以名称作为键，未参与匹配的捕获为 null。
- 命名捕获和数字键同时存在，便于既用 `jq(.id)` 也用 `jq(."1")` 访问。

示例：

```powershell
# JSON 中抽取消息，再用正则解析，最后重整形
ux -e 'jq(.items[].msg) | re(r"user=(?<id>\d+)") | jq({id})' a.json

# HTML 链接 -> 属性 -> 正则命名捕获 -> jq 取值
ux -e 'xpath("//a") | attr("href") | re(r"^https://(?<host>[^/]+)") | jq(.host)' page.html

# XML 命名空间、节点文本和正则组合
ux --ns b=urn:books -e 'xpath("//b:book") | text() | re(r"(?<sku>[A-Z]+-\d+)")' books.xml

# 日志逐行流式抽取，输出原始文本
cat app.log | ux -f text -O text -e 're(r"ERROR: (?<message>.*)") | jq(.message)'

# 递归选择文件，再对路径做模糊过滤
ux --fuzzy 'prodconfig' -e 'jq(.database.host)' .

# CSV 记录视为对象，直接使用 jq
ux -f csv -e 'jq(select(.status == "failed") | {id, error})' jobs.csv
```

## 2. CLI 契约

建议二进制名为 `ux`：

```text
ux [-e PIPELINE] [-f FORMAT] [-O jsonl|text|json]
   [--text-mode lines|raw] [--ns PREFIX=URI]...
   [--fuzzy QUERY] [--with-source] [--strict]
   [PATH|GLOB|-]...
```

### 输入与文件发现

- 无路径时读取 stdin；`-` 显式表示 stdin，一次调用最多一个。
- 显式文件直接输入；目录递归枚举文件；glob 支持 `*`、`?`、字符类和 `**`，结果排序并去重。
- `--fuzzy` 对规范化相对路径进行 nucleo 评分过滤；同分按路径字典序，保证结果可复现。实际打开原始 Path，不使用模糊匹配后的字符串反推路径。
- `-f auto|json|jsonl|yaml|toml|xml|html|text|csv|tsv`，默认 auto。
- auto 先按已知扩展名判断；stdin/未知扩展名只保守嗅探 JSON、XML/HTML，否则按文本。YAML、TOML、CSV、JSONL 在无法确定时应显式指定。
- 格式确定后解析失败就报错，不再降级为另一种格式。
- `--text-mode lines` 默认逐行读取；`raw` 将整个文件作为单个 Text，支持跨行正则，但不再流式。
- XML 命名空间通过重复 `--ns PREFIX=URI` 注入 XPath 上下文。

### 各格式输入语义

| 格式 | 输入项 | 流式边界 |
|---|---|---|
| JSON | 整个文档一个 Data | 完整解析 |
| JSONL/NDJSON | 每个非空行一个 Data，保留行号 | 逐行解析并立即输出 |
| YAML | 每个 YAML 文档一个 Data；非字符串映射键明确报错 | 首版完整解析 |
| TOML | 根表一个 Data；日期时间转字符串 | 完整解析 |
| XML | 根节点一个 Node | 完整解析，格式错误报错 |
| HTML | 根节点一个 Node | html5ever 容错解析，完整构建 DOM |
| text/log | 每行一个 Text，或 raw 模式全文一个 Text | lines 模式逐行 |
| CSV/TSV | 首行为表头，每条记录一个字符串字段对象 | 逐记录；重复/空表头报错 |

### 输出与错误

- 默认 `-O jsonl`：每个结果一行 JSON；Text 和 Node 输出 JSON 字符串，Node 取其 string-value；Data 输出紧凑 JSON。
- `-O text`：Text 原样输出；Node 输出 string-value；Data 字符串原样输出，其余输出紧凑 JSON。每条结果以换行结束。
- `-O json`：把所有结果聚合为一个 JSON 数组，完成后一次输出。
- `--with-source`：每项包装为 `{value, source:{path,line,record}}`，缺失的位置字段为 null。
- stdout 只写数据；诊断写 stderr，并包含文件、记录/行号及失败阶段。
- 默认当前文件首错即停、继续其他文件，最终有任何错误则退出 1；`--strict` 全局首错即停。
- 退出码：0 成功（包括零匹配）；1 输入、解析、求值或输出错误；2 CLI 参数或 DSL 编译错误。
- BrokenPipe 视为正常终止，不打印额外错误。
- JSONL、逐行文本、CSV/TSV 每产出一个结果就写并 flush，保证持续输入时在 EOF 前可见。

## 3. 模块与依赖

### 项目结构

保持单 Cargo package，不提前拆 workspace：

- `Cargo.toml`：依赖、binary 与 dev-dependencies。
- `src/main.rs`：解析参数、调用库入口、映射退出码。
- `src/lib.rs`：编排一次运行，暴露可测试的 `run` 入口。
- `src/cli.rs`：clap 参数与配置校验。
- `src/model.rs`：Item、Value、Origin、输入格式等公共类型。
- `src/dsl.rs`：lexer/parser、Stage AST、选择器预编译。
- `src/engine.rs`：流水线调度、0..N 分发和阶段类型检查。
- `src/stage/`：jq、xpath、regex 的真实执行适配器。
- `src/input.rs`：格式探测、reader 解码、单文档/单记录回调。
- `src/discover.rs`：文件、目录、glob、fuzzy 的确定性展开。
- `src/markup.rs`：XML/HTML Package 生命周期及 DOM 构建。
- `src/output.rs`：jsonl/text/json 序列化、source 包装和 flush。
- `src/error.rs`：带路径、记录和阶段上下文的错误类型。
- `tests/cli.rs` 与 `tests/fixtures/`：真实二进制端到端测试。

### Crate 选型

- CLI/错误：`clap`、`thiserror`，顶层可使用 `anyhow`。
- DSL：`winnow`，配合明确的深度/字符串状态测试。
- 通用值：`serde`、`serde_json`。
- jq：`jaq-core`、`jaq-std`、`jaq-json`；所有 serde_json 与 jaq 值互转集中在一个适配模块，验证多输出、运行时错误及不可表示值。
- XML/XPath：`sxd-document`、`sxd-xpath`。
- HTML：优先验证 `sxd-html`（crate 名/API 在实施时锁定），利用其 html5ever TreeSink 直接构建 sxd_document，而不是把 HTML 当 XML。
- YAML/TOML/CSV：采用维护中的 `serde_yaml_ng`、`toml`、`csv`。
- 正则：`regex`，利用其线性时间保证，支持命名捕获。
- 路径：`glob`、`walkdir`、`nucleo-matcher`。

### 必须先做的依赖验证

HTML/XPath 是最大技术风险。实现 markup 阶段时先用真实解析和执行代码验证：错位标签、属性、script 内容、SVG/MathML、XHTML 命名空间、节点生命周期及 `//a` 行为。若 `sxd-html` 不满足要求，固定兼容的 html5ever 版本，自行实现构建 sxd_document 的 TreeSink；不引入 libxml，不用“HTML 转 XML 字符串再猜解析”或 CSS 选择器冒充 XPath。

同时用小型真实程序锁定 jaq  crate 版本组合，验证 serde_json 互转、内部管道、多结果和错误传播。技术验证若失败就调整适配层，不留下占位实现。

## 4. 分阶段实施

### 阶段 1：可运行纵切

建立 Cargo 项目和核心模型，完成 DSL、jq、re、JSON/JSONL、逐行文本、stdin、jsonl/text/json 输出及错误上下文。

验收：JSON -> jq -> re -> jq 示例成功；JSONL 与文本在 stdin 未关闭时已输出首条结果；非法 DSL、类型错误和坏 JSONL 有正确退出码。

### 阶段 2：真实 XML/HTML XPath

完成上述依赖验证，加入 XML/HTML DOM、xpath、text、attr、命名空间及文档生命周期管理。

验收：XML 命名空间示例、容错 HTML 链接抽取示例成功；XPath 的 nodeset/string/number/boolean 结果均按契约输出。

### 阶段 3：补齐格式与格式探测

加入 YAML 多文档、TOML、CSV/TSV、扩展名映射、保守内容嗅探和 `--text-mode raw`。

验收：全部格式使用同一 jq 阶段；CSV/TSV 在 EOF 前输出；YAML 非字符串键、重复 CSV 表头、TOML 日期转换有明确测试。

### 阶段 4：文件发现与多文件语义

加入目录递归、glob、fuzzy、排序去重、多文件默认继续/strict、`--with-source` 和 BrokenPipe 处理。

验收：glob 与 fuzzy 组合顺序稳定；多文件部分失败仍处理其他文件且最终退出 1；strict 在首错停止；来源字段定位准确。

### 阶段 5：加固与发布

补齐 CLI golden、流式、错误及兼容性测试，执行 fmt、clippy、完整测试和 release 构建，确认生成单个可执行文件且运行时不依赖外部命令。

每个阶段均以真实 CLI 行为验收，不提交 mock 引擎、TODO 命令或仅占位接口。

## 5. 验证方案

### 单元测试

- DSL：嵌套括号、jq 内部 `|`、单双引号、原始字符串、转义、空阶段、未知阶段。
- 流水线：0..N 过滤/展开、Origin 保留、逐阶段类型错误。
- 正则：完整匹配、数字/命名捕获、未参与捕获、多次非重叠匹配、无效表达式。
- XPath：文档顺序、原子类型、缺失属性、命名空间。
- 路径：glob 展开、目录递归、去重、fuzzy 同分排序。
- 输出：Text/Data/Node、source 包装、JSON 聚合。

### 集成与 CLI golden

覆盖所有输入格式及至少一条跨引擎组合流水线；断言 stdout、stderr 和退出码。边界用例包括：空输入、零匹配、非法 UTF-8、坏 JSONL 行、坏 XML、容错 HTML、CSV 表头错误、多文件部分失败、stdin 与文件混用、未知扩展名和显式格式冲突。

jq 建立常用兼容用例集：字段/数组迭代、select、对象构造、内部管道、多输出和运行时错误；未支持语法必须真实报错并在能力清单中记录。

### 流式回归

测试启动真实 `ux` 子进程，向 stdin 写入一条完整 JSONL、文本行或 CSV 记录后保持 stdin 打开，通过带超时的读取通道断言 EOF 前收到对应输出，再关闭 stdin 校验最终结果。`-O json` 则验证其明确表现为完成后聚合输出。

### 最终命令

```powershell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

再直接运行 release 二进制执行本文所有 PowerShell 示例，验证输出、退出码和持续流行为。
