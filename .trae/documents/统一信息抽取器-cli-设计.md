# UniExtract 统一信息抽取 CLI：设计与实施计划（v3）

## 0. 修订说明

### 0.1 基线

v1 是「工作区为空、从零实现」的设计稿；v2（`ebf96ff`）把它重基线到已落地的 v0.1 实现（`0e6ec2a`），修正了一批契约矛盾并登记了代码偏差。**v3 是对 v2 设计本身的审查与整改**：v2 把实施缺口找得比较准，但自身仍留下若干自相矛盾、写得过死以致无法实现、以及把实现缺陷写成契约的条款；同时它删掉了 v1 的「非目标」，范围边界反而变模糊了。

v3 做三件事：

1. **修契约**：把 v2 中事实错误、互相冲突、不可实现的条款改对（§0.3 给出完整清单）；
2. **补模型**：把 v2 只写了一句话的两处核心机制补成可执行规格 —— `slurp` 的会话执行模型（§3）与阶段类型上界检查（§2.3）；
3. **重排计划**：把「先止血、再铺路、最后动架构」作为阶段顺序，并给每条验收标准一个可断言的观测点（§7）。

状态标记贯穿全文：`[已实现]` 指代码已落地并有测试覆盖；`[v3 契约]` 指 v3 新确定的目标契约；`[待实施]` 指 v2 遗留但未完成的目标。

> **状态同步（2026-11-14，阶段 1–6 实施收口）**：阶段 1–6 已全部落地，`Cargo.toml` 版本为 `0.2.0`。原 v3 的 `[v3 契约]` / `[待实施]` **状态列**已逐处改为 `[已实现]`，契约条款本身保留不改；§7 的验收标准保留为回归基线。实现与原计划有偏差的条目在 §0.5 与 §9 的 A11–A16 显式登记，不靠改写契约来掩盖。收口后另做了一次**DSL 解析器 PEG 化**（手写状态机 + winnow → pest 2，A15/A16），属实现替换而非契约变更：§4.1/§4.2 的语法、退出码与九条示例均保持不变。

### 0.2 v3 审查结论

对照 `Cargo.lock` 实际解析版本、`jaq-json 2.0.3` 源码与本地实测，共发现 **21 项缺陷**，按严重度分级：

| 级别 | 数量 | 代表问题 |
|---|---|---|
| **A 事实错误**（文档断言与依赖/运行行为相反） | 3 | v2 数值契约「NaN 输出 null」「超 isize 报错」全错；实测 `jq(nan)` 让 v0.1 直接抛错 |
| **B 自相矛盾** | 5 | UTF-16 支持 vs NUL 判二进制；Origin 表 `line=1` vs v2 输出契约「缺失字段为 null」；「编译期可判定类型」vs 动态 `Value` 模型 |
| **C 不可实现 / 严重低估** | 4 | `slurp` 只需「加个 collect 分支」；YAML 文档起始行不可得；`Val` 无 `Serialize`；每结果 flush 被写成契约 |
| **D 契约未定义** | 6 | slurp 作用域/位置/空输入/错误；fuzzy 命中判定；排序基准；glob 空匹配；stdin 顺序；单行体积上界 |
| **E 结构与表述** | 3 | 非目标章节丢失；`--force-text` 漏出参数表；验收标准含「百万级用例」等不可操作描述 |

完整逐条对应关系见 §0.3 修订索引。

### 0.3 v3 修订索引

| 编号 | 修订内容 | 修掉的问题 | 章节 |
|---|---|---|---|
| R13 | 恢复「目标 / 非目标 / 全局原则 / 术语」章节，v1 的 Context 不应丢 | E | §1 |
| R14 | I1 精确化：跨记录或跨文件持有必须物化为 `JsonValue`/`String`，`-O json` 不是例外 | B | §2.1 |
| R15 | Origin 表统一：无行概念的格式 `line = null`；删除「YAML 文档起始行」这一不可得要求 | B/C | §2.2 |
| R16 | 新增 `TypeSet` 静态上界检查，替代「编译期可判定类型错误」的过度承诺；运行期值级检查保留 | B/C | §2.3 |
| R17 | `attr()` 接受 Attribute 节点；类型表与 `TypeSet` 对齐 | D | §2.4 |
| R18 | 重写 `slurp` 执行模型为会话状态机，定义作用域、位置约束、空输入、Origin 折叠、错误语义、与输出模式组合 | C/D | §3 |
| R19 | `split()` 完整契约（空片段、trim、分隔符、体积上界） | D | §3.5 |
| R20 | 定义 DSL 解析不变量与 fuzz 面，明确无「字面 `\|`」转义 | E | §4 |
| R21 | 参数表补全 `--force-text`、`--no-default-ignore`、`--line-buffered`、`--max-record-size`；新增参数校验矩阵 | E | §5.1 §5.2 |
| R22 | 文件发现补齐：排序基准、fuzzy 命中判定、exclude 优先级、symlink、空 glob、stdin 顺序、遍历错误、串行确定性 | D | §5.3 |
| R23 | 编码/BOM/二进制：定义探测顺序，UTF-16/32 豁免 NUL 规则，补 `encoding_rs` 依赖，补单行/单记录上界 | A/B/D | §5.4 §6.2 |
| R24 | `--skip-incompatible` 收窄为静态可判矩阵，明确 stdin 行为与「中段不匹配不覆盖」 | B | §5.7 |
| R25 | flush 策略改为「默认批量 + `--line-buffered`/TTY 自动逐条」，不再把每结果 syscall 写成契约 | C | §5.5 |
| R26 | jaq 数值契约按实测重写：移除 `to_string` 回环，改显式 `val_to_json`；`NaN`/`±Inf` → `null`；大整数保真 | A | §6.3 |
| R27 | 结构表补 `discover`/`source`/`types` 模块；依赖表补 `encoding_rs`、移除未使用的 `anyhow` | E | §6.1 §6.2 |
| R28 | 计划重排为 7 阶段，阶段 0 止血前置、聚合延后到设计完备之后 | E | §7 |
| R29 | 破坏性变更清单与迁移策略；验收标准全部改为可断言观测点 | E | §5.8 §7 §8 |
| R30 | `--ns` 的 help 文案必须与「仅前缀解析」限制一致；新增 ADR 摘要与已知限制汇总 | E | §6.4 §9 §10 |

### 0.4 现状台账（更新）

| 偏差 | v2 状态 | v3 状态 | 影响 |
|---|---|---|---|
| `--with-source` 覆盖文本模式 | 静默改输出形态 | **[已实现]** `Cli::validate` 报 `Usage`，退出 2 | 参数组合语义明确 |
| `--fuzzy` 死开关 | 静默全量输入 | **[已实现]** fail-fast 报 `Usage`，退出 2；阶段 2 落地后改为过滤 | 不再有「以为在过滤」的静默错误 |
| `-` 多次 | `run` 内 `Query(2)` | **[已实现]** 移入 `validate`，`Usage(2)` | 校验统一在读取输入前 |
| `jq` 的 `NaN`/`±Inf` | 运行期解析错误，文档却写「输出 null」 | **[已实现]** 显式 `val_to_json`，映射为 `null`，含嵌套用例 | 与 jq 语义一致，文档与实现一致 |
| `jq` 字符串回环 | `to_string` → `from_str` | **[已实现]** 枚举遍历转换，保留数字字面量 | 消除非法 JSON 与精度风险 |
| 文件发现缺失 | 目录/glob/递归不可用 | **[已实现]** 阶段 2：`src/discover.rs`，`--fuzzy` 恢复为过滤 | 目录/glob/fuzzy/确定性排序可用 |
| 无聚合能力 | 无 `slurp`/`split` | **[已实现]** 阶段 3：`Pipeline`/`Session`/`Relay`，跨文件全局聚合 | JSONL/CSV 可跨记录统计 |
| BOM / 编码 / 二进制 | 未处理 | **[已实现]** 阶段 4：`src/source.rs`，BOM/GB18030/UTF-16/体积上界/XML 声明规范化 | 带 BOM 首行、GBK、UTF-16 均可用 |
| `-O json` 紧凑、缩进 | 紧凑单行 | **[已实现]** 阶段 5：默认缩进 2 空格，`--compact` 关闭 | 大数组人可读 |
| 每结果 flush | 写成契约并标「已有实现」 | **[已实现]** 阶段 5：默认 64 KiB 批量 + `--line-buffered` / TTY 逐条 | 大文件 syscall 数量级下降，管道场景显式可控 |
| 端到端测试 | `tests/` 不存在 | **[已实现]** 阶段 6：`tests/cli.rs`、`tests/jq_compat.rs`、CI、fuzz 骨架 | 46 单测 + 26 CLI + 3 兼容集 |
| XPath 命名空间 | 仅前缀解析 | **仍为已知限制**；help 文案已按 §6.4 修正 | 真 URI 匹配留待 v4，见 §10 |

### 0.5 阶段 1–6 实现状态与偏差登记

| 阶段 | 落地内容 | 验证 |
|---|---|---|
| 1 契约静态化 | `src/types.rs`（`TypeSet`/`source_types`/`Limits`）、DSL `slurp()`/`split()`、`CompiledStage::input_types`/`output_types`、`Pipeline::compile` 上界检查、`attr()` 支持 Attribute 节点 | `slurp() \| xpath(...)` 编译期 `Query(2)`；`-f json` 直接 `xpath(...)` 同样 |
| 2 文件发现 | `src/discover.rs`；`--max-depth`/`--exclude`/`--no-default-ignore`/`--fuzzy`；默认忽略目录在遍历期剪枝；确定性排序去重 | 空 glob 退出 0 + warning；`target/` 哨兵文件不出现在输出 |
| 3 会话执行 | `engine.rs` 重写为 `Pipeline`/`Session`/`Relay`；`ItemSink` trait；`input.rs`/`markup.rs` 改回调；`slurp()` 跨文件、`split()` | 双文件 `slurp() \| jq(length)` → 3；零输入 → `[]`；`slurp` + `-O json` → 双层数组 |
| 4 输入卫生 | `src/source.rs`；编码判定 → 二进制探测（UTF-16 豁免 NUL）→ 解码 → 剥 BOM → XML 声明规范化 → 格式嗅探；`--max-input-size`/`--max-record-size`；CRLF | BOM JSONL、GBK 回退、UTF-16 不误判、二进制跳过/显式报错、体积超限明确报错 |
| 5 输出收口 | `src/output.rs` 的 `OutputOptions`/`BufWriter`；`-O json` pretty + `--compact`；`--max-results` 非静默截断；`--line-buffered` + `IsTerminal`；`--compact` 误用 warning；重复 `--ns` warning | `--max-results 1` 第 2 条退出 1；EOF 前可见输出；`ux --help` 含命名空间限制说明 |
| 6 测试加固 | `tests/cli.rs`(26)、`tests/jq_compat.rs`(3)、`.github/workflows/ci.yml`、`fuzz/` 独立 workspace、`README.md`/`CHANGELOG.md` | `fmt --check` / `clippy -D warnings` / `test --all-targets` / `build --release` 全绿；release 二进制逐条跑 §4.4 九个示例 |

**与 v3 原计划的三处偏差（详见 §9 A11–A13）：**

1. §8.5 原定 `xtask + COMPAT.md` 自动导出 jaq 内建函数表；实际改为 `tests/jq_compat.rs` 的**可执行兼容集**。理由：生成式矩阵只能反映「函数存在」，不能反映「语义与 jq 一致」；可执行断言防漂移且零额外工具链。
2. `fuzz/` 以独立 `[workspace]` 提供 `dsl_split` target，但 **CI 未跑 60 秒 smoke**：cargo-fuzz 需要 nightly + libfuzzer，会拖慢双平台门禁；保留骨架，按需本地运行。
3. §8.3 的「`-O json` / `slurp` 输入关闭前 stdout 为空」未写成独立 golden，只覆盖了 `--line-buffered` 的 EOF 前可见性；这两个模式的不流式由代码结构（聚合到 `finish`）保证，登记为残余验证缺口。

**收口后的增量（2026-11-14，非 v3 计划项，见 A15–A16）**：DSL 结构解析从「手写字节状态机 + winnow 读阶段名」换成 `src/dsl.pest` 的 pest 2 PEG，`src/dsl.rs` 只保留语义层；同时新增 128 层嵌套守卫，修复了深层括号导致**进程栈溢出**的实测缺陷。语法契约、退出码、`parse_pipeline()` fuzz 入口与 §4.4 九个示例的输出全部逐条回归通过。

---

## 1. 范围、非目标与全局原则

### 1.1 目标

- Rust 单 crate，纯 Rust 依赖，运行时不调用外部 `jq`、shell 或本地 C 库；库核心 + 薄 CLI。
- 组合式管道 DSL，把 jq、XPath、正则放进**同一条记录流**，阶段间用统一数据模型传递，而不是三个子命令。
- 首版输入：JSON、JSONL/NDJSON、YAML、TOML、XML、HTML、纯文本/日志、CSV/TSV。
- 路径支持目录递归、glob、模糊过滤。
- JSONL、逐行文本、CSV/TSV 流式处理；整文档格式完整解析；聚合能力显式放弃流式并告知用户。

### 1.2 非目标（v1 内容，v2 误删，v3 恢复）

- 不承诺 XPath 2.0/3.0，也不承诺 XPath 1.0 **完整**兼容（只承诺实际测过的函数与轴，见 §6.4）。
- 不承诺 libjq 100% 兼容；兼容范围以 `jaq-*` 锁定版本的实测结果为准（§8.5）。
- 不做大型单个 JSON/XML 的增量（SAX/流式 DOM）解析。
- 不做真正的 namespace-URI 匹配（需换 DOM 后端，属 v4 议题，见 §6.4）。
- 不做多行/并行文件处理、不做网络输入、不做输出写回文件（stdout 唯一数据出口）。
- 不提供插件或用户自定义阶段；阶段集合是封闭的（§4.1）。

### 1.3 全局设计原则

| 编号 | 原则 | 含义 |
|---|---|---|
| **P1** | 确定性 | 相同输入与参数必须给出逐字节相同的输出。文件顺序、fuzzy 同分排序、JSON 对象键序、随机数（fuzzy 打分器种子）都必须显式固定。**处理永远串行**，禁止随手引入并行遍历破坏顺序与 `record` 编号。 |
| **P2** | 宁可吵，不可骗 | 参数矛盾、未实现的能力、类型不兼容、二进制输入、体积超限都必须显式报错或 warning，绝不静默降级、静默忽略、静默改输出形态。零匹配是**正常结果**，不是错误。 |
| **P3** | stdout 纯净 | stdout 只写数据；所有诊断、warning、进度写 stderr。任何错误路径都不得把半截 JSON 写进 stdout（故 `-O json` 与 `slurp` 必须在全部输入成功后再输出）。 |
| **P4** | 流式优先，聚合显式 | 默认逐记录处理并尽快输出；任何跨记录收集（`slurp`、`-O json`）必须在 `--help` 与本文写明内存与延迟代价。 |
| **P5** | 校验前置 | 参数校验、DSL/jq/XPath/正则编译、静态类型上界检查全部在打开第一个输入之前完成，失败退出 2。 |
| **P6** | 借用模型不逃逸 | 见 I1（§2.1）。编译器负责强制，不用 `unsafe`/`transmute` 绕过。 |

### 1.4 术语

| 术语 | 定义 |
|---|---|
| **Item** | 流水线中流动的一条值 + 它的来源 `Origin`。 |
| **源类型** | 某输入格式交给流水线首阶段的 `Value` 变体：整文档格式为 `Data`，XML/HTML 为 `Node`，文本格式为 `Text`。 |
| **记录** | 输入层的自然单元：JSONL 的一行、CSV 的一行数据、text 的一行、一个 JSON/XML 文档。`record` 编号按**每个文件**从 1 重置。 |
| **会话（Session）** | 一次 `ux` 运行中，含 `slurp` 时覆盖**所有输入文件**的聚合上下文（§3.3）。 |
| **物化** | 把可能借用文档的值（`Node`）转换为拥有所有权的值（`String`/`JsonValue`），使其可跨记录、跨文件持有。 |
| **边界阶段** | `slurp()`：把逐条流切换为单条聚合值的阶段。一条管线至多一个。 |

---

## 2. 核心数据模型

### 2.1 Item / Value 与 I1 不变量

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

v1 曾考虑 arena + 稳定索引消灭 lifetime。**结论维持：不需要。** `markup::process` 持有 `Package`，`Item<'doc>` 只在文档作用域内流通，借用检查器已经保证了正确性。

> **I1（Node 不跨记录/跨文档）** —— `Node` 只能在同一 `Package` 的生命周期内使用。**任何可能同时持有两条及以上记录、或跨文件持有值的容器，其元素类型必须是 `JsonValue` 或 `String`；持有 `Node` 一律禁止。** 物化规则：`Node → node.string_value() → String`；`Text → String`；`Data → JsonValue` 原样。
>
> 适用范围包括：`slurp` 缓冲、`-O json` 聚合、未来任何跨文件缓冲或收集型阶段。**`-O json` 不是例外** —— 它之所以安全，恰恰因为 `OutputSink::aggregate: Vec<JsonValue>` 存的是已 `into_json()` 物化的值。v2 把 `-O json` 写成「例外」容易被误读成「可以不物化」，本版删除该表述。
>
> 该不变量由编译器强制（`Package` 是 `process_reader` 内的局部变量），不需运行时断言；编译报错时不要用 `unsafe` 绕过，而应把物化点前移到文件边界。

`Origin` 三字段：

```rust
pub struct Origin {
    pub path: Option<Arc<PathBuf>>,  // stdin 为 None
    pub line: Option<u64>,           // 无行概念时为 None → JSON 中为 null
    pub record: u64,                 // 每文件从 1 起；无来源时为 0
}
```

### 2.2 Origin 填充规则（v2 矛盾已修）

| 输入 | `path` | `line` | `record` |
|---|---|---|---|
| JSON / YAML 单文档 / TOML / XML / HTML | 文件路径 | **`null`** | 1 |
| YAML 多文档 | 文件路径 | **`null`** | 文档序号（1 起） |
| JSONL / NDJSON | 文件路径 | 该行行号 | 非空记录序号（1 起） |
| text lines | 文件路径 | 该行行号 | 同 `line`（空行也算一条，见 §5.6） |
| text raw | 文件路径 | **`null`** | 1 |
| CSV / TSV | 文件路径 | `position().line()`（物理行，含表头） | 记录序号（1 起，不含表头） |
| stdin | **`null`** | 同对应格式 | 同对应格式 |
| `slurp` 结果 | 首条输入的 `path` | 首条输入的 `line` | 首条输入的 `record`；零输入时全 `null` / `0` |

v2 把单文档格式写成 `line = 1`，却在同一份文档的输出契约里写「缺失字段为 null」，而代码实际是 `Origin::new(path, 1)` 即 `line = None`。**本版统一为 `null`，理由：单文档没有「行」这个概念，写 1 是伪造信息。** 同时删除 v2 的「YAML 多文档 `line` = 文档起始行（不可得时为 1）」—— `serde_yaml_ng::Deserializer::from_str` 不提供文档起始行，该要求必然永远落到兜底值，属于写了等于没写。若未来需要，应换带 span 的解析器，属独立议题。

### 2.3 类型系统与静态上界检查（v3 新增，替代 v2 的过度承诺）

v2 §4 要求「`slurp` 后接 `xpath` 必须是**编译期可判定的类型错误**」。但 v2 §1.2（本版 §2.4）的类型模型是运行期 `Value` 枚举分派：`jq` 输出 `Data` 还是 `Text` 取决于值，`xpath` 输出 `Node`/`String`/`Number`/`Boolean` 取决于表达式求值结果。**要求精确判定运行期类型，等于要求一套静态类型系统，与现有模型冲突且本版不做。**

可实现的替代方案是**类型集合上界检查**：

```rust
#[derive(Clone, Copy)]
pub struct TypeSet(u8);  // DATA | TEXT | NODE 位标志

impl CompiledStage {
    fn input_types(&self) -> TypeSet;   // 该阶段能接受的类型集合
    fn output_types(&self) -> TypeSet;  // 该阶段「可能」产出的类型集合（上界）
}
```

| 阶段 | `input_types` | `output_types` | 依据 |
|---|---|---|---|
| `jq` | Data, Text | Data, Text | 输出 JSON 字符串时回落 Text（R6） |
| `xpath` | Node | Node, Data, Text | nodeset / number+boolean / string |
| `attr` | Node | Text | 属性值；缺失时 0 条 |
| `re` | Text | Text, Data | 有无捕获组决定（R7） |
| `text` | Data, Text, Node | Text | 全类型转字符串 |
| `split` | Text | Text | 切成多段 Text |
| `slurp` | Data, Text, Node | **Data** | 恒为一个数组 |

编译期检查（`Pipeline::compile`，失败 `AppError::Query` → 退出 2）：

1. 相邻阶段 `i, i+1`：若 `output_types(i) ∩ input_types(i+1) == ∅`，报错「阶段 X 永不产出阶段 Y 可接受的类型」。这能抓到 `slurp() | xpath(...)`（`{Data} ∩ {Node} = ∅`）、`jq(...) | xpath(...)`、`text() | xpath(...)` 等**确定不可能**的组合，正是 v2 想要的效果。
2. 若 `-f` 显式指定格式，用其源类型与 `input_types(0)` 求交，为空则报错（不必等运行期）。
3. **交集非空不代表运行期一定兼容**：`jq(...)` 可能产出 `Data` 而 `re` 需要 `Text`。因此 `CompiledStage::evaluate` 里的值级类型检查**必须保留**，错误消息仍含阶段名、实际类型、来源路径与记录位置。

> 这条设计把「编译期能判多少」写成了可验证的边界，而不是一句无法落地的口号。`--skip-incompatible` 的静态判定（§5.7）复用同一张表。

### 2.4 阶段类型规则（在 v2 基础上补两处）

| 阶段 | 接受类型 | 输出与规则 |
|---|---|---|
| `jq(filter)` | `Data`、`Text` | `Text` 作为 JSON 字符串输入；输出 JSON 字符串时回落 `Text`，其余 `Data`；数值与 `NaN` 规则见 §6.3 |
| `xpath(expr)` | `Node` | nodeset 按文档顺序展开为 `Node`；字符串→`Text`；数字/布尔→`Data` |
| `re(pattern)` | `Text` | 无捕获组：每个非重叠匹配产出完整匹配 `Text`；有捕获组：每次匹配产出一个 `Data` 对象 |
| `text()` | 所有类型 | `Node`→string-value；`Text`→原样；`Data` 字符串→原样；其余→紧凑 JSON |
| `attr(name)` | **元素或属性 `Node`** | 元素：属性值 `Text`，缺失产出 0 条；**属性节点：直接产出其值 `Text`**；其它节点类型报错 |
| `slurp()` | 任意（物化） | 上游全部结果收敛为一个 `Data` 数组，见 §3 |
| `split(pattern)` | `Text` | 按正则切分成多个 `Text`，见 §3.5 |

v2 的 `attr` 只接受元素节点，导致 `xpath("//@href") | attr("href")` 这种自然写法报「非元素」错误。本版让它同时接受 Attribute 节点；`attr` 对属性节点的参数是**被忽略的**（值已经就是想要的），为保持语法一致仍要求一个字符串参数，并在 `--help` 说明。

不做隐式 `Node → JSON`、JSON 文本自动解析。需要时用 `jq` 显式处理。

---

## 3. 流水线与执行模型

### 3.1 StageSpec / CompiledStage

```rust
pub enum StageSpec {
    Jq(String), XPath(String), Regex(String),
    Text, Attr(String), Slurp, Split(String),
}

pub enum CompiledStage {
    Jq(JqStage), XPath(XPathStage), Regex(RegexStage),
    Text, Attr(String), Slurp, Split(Regex),
}
```

- `Pipeline::compile()` 完成全部编译、静态类型上界检查、边界阶段定位；任一失败即 `AppError::Query` → 退出码 2，**早于任何输入读取**（P5）。
- 每个非边界阶段接收一个 `Item`，同步产出 `0..N` 个 `Item`：0 表示过滤，N 表示展开。
- 除 `slurp` 与 `-O json` 外不跨记录收集（P4）。

### 3.2 无边界阶段：逐条流式

当前 `execute_from` 的深度优先递归保持不变，签名维持 `Item<'doc>` 借用：

```rust
pub fn execute<'doc>(
    &self,
    item: Item<'doc>,
    sink: &mut dyn FnMut(Item<'doc>) -> Result<(), AppError>,
) -> Result<(), AppError>;
```

每个文件独立调用一次，`Package` 在该文件的 `process_reader` 栈上，符合 I1。

### 3.3 含边界阶段：会话状态机（v3 重写 v2 的「collect 分支」）

v2 说 `slurp` 只需「`engine.rs` 引入一条收集上游再下发的执行路径」。**这是严重低估**：现有 `execute` 是纯函数式逐条调用，没有跨 item、更没有跨文件的状态载体；而 `Item<'doc>` 的 `Node` 借用会在 `process_reader` 返回时失效。要做跨文件聚合，必须把 pipeline 提升为**会话对象**，并改变 `lib.rs` 的编排与 `input::emit` 的接口。规格如下：

```rust
pub struct Pipeline {
    stages: Vec<CompiledStage>,
    boundary: Option<usize>,   // slurp 的下标，compile 期确定，至多一个
}

pub struct Session<'p> {
    pipeline: &'p Pipeline,
    buffer: Vec<JsonValue>,        // 已物化，符合 I1
    first_origin: Option<Origin>,  // 折叠后的来源
    failed: bool,
}

impl<'p> Session<'p> {
    /// 对每条输入记录：执行 stages[..boundary]，结果立即物化追加到 buffer。
    pub fn feed(&mut self, item: Item<'_>) -> Result<(), AppError>;
    /// 全部输入结束后：buffer → 一条 Data(Array)，执行 stages[boundary+1..]，送 sink。
    pub fn finish(self, sink: &mut dyn FnMut(Item<'static>)) -> Result<(), AppError>;
}
```

`lib.rs` 的编排相应变为：

```
validate → compile → discover → session = pipeline.session()
for each input:  process_reader(..., &mut |item| session.feed(item))
session.finish(&mut |item| output.emit(item))
output.finish()
```

`input::emit` 目前直接调 `pipeline.execute`，阶段 3 需改为接受一个 `&mut dyn FnMut(Item<'_>) -> Result<(), AppError>` 的回调，由 `lib.rs` 决定接到 `session.feed` 还是直接输出。**这是阶段 3 的主要改动面，不是「加个分支」。**

### 3.4 `slurp()` 完整契约（v2 完全未定义的部分）

| 议题 | 契约 |
|---|---|
| **作用域** | 一个 `Session` 覆盖**整次运行的所有输入**（所有文件 + stdin）。`ux -f jsonl -e 'slurp() \| jq(length)' a.jsonl b.jsonl` 给出两文件记录总数。 |
| **位置** | 允许出现在任意位置（含第一个、最后一个）；**至多一个**，第二个在 `compile` 期报 `Query(2)`。「上游」指 `slurp` 之前所有阶段的输出，不是原始输入。 |
| **物化** | `feed` 时立即把每个上游结果转成 `JsonValue`：`Node → String(string_value)`、`Text → String`、`Data` 原样。缓冲里永不出现 `Node`（I1）。 |
| **零输入** | `finish` **仍产出一条 `[]`**（与 `jq -s` 一致），`Origin` 为 `path=null, line=null, record=0`。这是可观测行为，必须有 golden 测试。 |
| **Origin 折叠** | 取上游第一条结果的 `Origin`；`--with-source` 只包最外层一条，**不保留逐条来源**。逐条来源需在未来提供 `origin()` 阶段（本版不做，见 §10）。 |
| **错误语义** | `feed` 或 `finish` 出现任何求值/解析错误 → **整次运行以退出码 1 终止，不输出部分聚合结果**。理由：聚合本就不流式（P3/P4），输出半截结果会让用户误以为是完整统计；「宁可吵，不可骗」（P2）优先于「尽量多给数据」。`--strict` 在此路径下无额外效果。 |
| **与 `-O json`** | `slurp` 产出 1 条 `Data(Array)`，`-O json` 再聚合为一个数组 → 输出 `[[...]]`。这是**预期行为**，要单个数组请用 `-O jsonl` 或让 `jq` 直接产出对象。文档与示例必须给出该组合的结果，避免用户以为是 bug。 |
| **与 `--with-source`** | 组合合法：产出 `{value: [...], source: {首条来源}}` 一行。 |
| **与 `--skip-incompatible`** | 跳过文件不产生 `feed`，不影响聚合；全被跳过时按「零输入」处理，输出 `[]`。 |
| **内存** | 与上游结果总数成正比；`--help` 与 README 必须写明「`slurp` 放弃流式与内存上界」。 |

### 3.5 `split(pattern)` 契约

- 只接受 `Text`；用 `regex::Regex::split` 按正则切分，**分隔符不保留**。
- 空片段（`""`）丢弃；片段**不 trim**（保真，需要时用 `jq` 的 `ltrimstr`/`gsub` 处理）。
- 每个片段受 `--max-record-size` 约束（§5.4）。
- 典型用法：`-f text --text-mode raw -e 'split(r"\r?\n-{16}\r?\n") | re(...)'`。
- 正则编译失败 → `Query(2)`；与 `slurp` 同管线时，`split` 在 `slurp` 之后作用于单个 `Data` 数组会被静态上界检查拦下（`{Data} ∩ {Text} = ∅`）。

### 3.6 错误传播与 `--strict`

- 默认：**当前文件首错即停、继续其他文件**，最终有任何错误退出 1；`--strict` 全局首错即停。
- 诊断格式：`ux: {path}:{line} (record N): stage \`{name}\`: {message}`，含失败阶段名与实际类型。
- `BrokenPipe` 视为正常终止（退出 0）；检测必须覆盖 `AppError::Io` **以及** `Runtime` 中被包装的 IO 错误，避免 Windows 管道下把 EPIPE 当成失败。
- `warning` 类（`--skip-incompatible`、二进制跳过）不计入 `had_errors`；但显式传入的单个文件被跳过时须在 stderr 明确写出「0 个文件被处理」，避免「无输出 + 退出 0」被误读为零匹配（P2）。

---

## 4. DSL

### 4.1 语法

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

管道符只在**括号深度为 0 且不在字符串内**时切分，因此 jq 自身的 `.a | .b` 不会被误拆（`dsl.rs` 单测已覆盖嵌套括号与原始字符串）。阶段名只接受 ASCII 字母与下划线；`text()`/`slurp()` 不接受参数，其余要求单个带括号参数。

### 4.2 字符串与转义

- 普通字符串支持 `\n \r \t \\ \' \"` 六种转义，其余报错并提示改用原始字符串；`r"..."` 与 `r#"..."#` 支持任意井号数，且内容中可含 `"`、`(`、`)`、`|`。
- **没有「字面 `|`」转义**：`|` 只能靠放在括号内或字符串内来保留。这不是缺陷而是有意约束，写入 `--help`。
- 已知解析面（fuzz 目标，§8.5）：未闭合原始字符串、`)`/`(` 深度失衡、反斜杠位于末尾、多字节 UTF-8 字符紧邻分隔符、`r` 后紧跟 `"` 但并非字符串起点。

**解析实现（v0.2.0，PEG 化）**：`src/dsl.pest` 用 **pest 2** 描述结构，`src/dsl.rs` 只做语义。

| 层 | 归属 | 职责 |
|---|---|---|
| 结构层 | `dsl.pest` | `pipeline = SOI ~ stage ~ ("\|" ~ stage)* ~ EOI`；`stage_name`；`inner = @{ balanced* }` 以 atomic 规则原样捕获括号内文本；`balanced` 用 `raw_string / quoted_string / group / plain` 保证字符串内的括号不干扰 stage 闭合 |
| 语义层 | `dsl.rs` | 阶段名分派到 `StageSpec`、六种转义解码、raw string 内容提取、错误文案 |

- 任意井号 raw string 用 pest 栈实现：`PUSH("#"*)` 记录井号串、内容循环用 `!("\"" ~ PEEK)` 做终止前瞻、结尾用 `"\"" ~ POP`。**结尾必须是 `POP`，不能写 `PEEK ~ POP`** —— `PEEK` 成功时会消耗输入中的井号串，再 `POP` 等于要求井号串出现两次（实测所有 raw string 都会静默失败并被 `plain` 回退路径"救"一部分，是本次踩到的真 bug）。
- grammar 是**结构**校验，字面量合法性由 Rust 侧二次校验（`unterminated raw string`、`unsupported escape` 等），两者失败都映射 `AppError::Query`（退出 2）。结构错误文案统一为 `invalid pipeline at byte N: <pest message>`。
- **嵌套深度上限 128**（`dsl.rs::check_nesting`）：`group` 是间接递归（`balanced → group → balanced`），实测约 500 层会**直接栈溢出终止进程**。该守卫只扫描深度并跳过字符串，不承担任何语法判定；超限返回 `pipeline nesting exceeds 128 levels`（退出 2）。不用 `pest::set_call_limit`：它按规则调用次数计数，与输入长度线性相关，会误拒长正则却仍放行足以爆栈的嵌套；也不启用 `stack-guard` feature：嵌套已被限死，动态栈扩展没有收益。注意 `pest 2.9.6` 的默认 `std` feature 本身无条件引入 `pest_stack_guard` → `stacker` → `psm`/`cc`（`cargo tree -i pest_stack_guard` 实测），**关掉 `stack-guard` 并不会移除这些构建期依赖**；本次曾观察到 `psm 0.1.32` 的 build script 在 release 构建中偶发 `STATUS_ACCESS_VIOLATION`，但依赖不变的后续构建均成功，判定为偶发构建故障，不作为设计依据。详见 A15/A16。
- `jq(FILTER)` 的内层不经过字面量解码，原文交给 jaq，因此内部的引号、转义、`|` 全部保留。

### 4.3 `re()` 的类型多态（沿用 v2 R7，保留）

- 无捕获组 → `Text` 流；有捕获组 → `Data` 对象流。同一条 `re(...) | jq(...)` 因加一个捕获组而改变类型，是**有意为之**，必须写进文档与 `--help`。
- 对象同时含数字键与命名键（`{"0":..., "1":..., "id":...}`），便于 `jq(.id)` 与 `jq(."1")`；副作用是 `jq(keys)` 会多出数字键。
- `regex` crate **无回溯引用、无 lookaround**。需要断言/回溯时，先用 `re` 粗切再用 `jq` 二次处理。

### 4.4 示例

```powershell
# JSON 内抽取 → 正则命名捕获 → 重整形
ux -e 'jq(.items[].msg) | re(r"user=(?<id>\d+)") | jq({id})' a.json

# HTML 链接 → 属性 → 正则 → 取值
ux -e 'xpath("//a") | attr("href") | re(r"^https://(?<host>[^/]+)") | jq(.host)' page.html

# XML 前缀（见 §6.4 限制：前缀需与文档一致）
# books.xml = <root xmlns:b="urn:books"><b:book sku="ABC-123"/></root>
# sku 是属性，必须用 attr() 读取；用 text() 只会得到空字符串
ux --ns b=urn:books -e 'xpath("//b:book") | attr("sku") | re(r"(?<sku>[A-Z]+-\d+)")' books.xml

# 日志逐行流式抽取
Get-Content app.log -Encoding utf8 | ux -f text -O text -e 're(r"ERROR: (?<message>.*)") | jq(.message)'

# JSONL 聚合统计（输出为全局总数）
ux -f jsonl -O text -e 'slurp() | jq(length)' jobs.jsonl more.jsonl

# raw 文本先切分再抽字段
ux -f text --text-mode raw -e 'split(r"\r?\n----------------\r?\n") | re(r"(?<id>\d{6})")' report.txt

# CSV 过滤投影
ux -f csv -e 'jq(select(.status == "failed") | {id, error})' jobs.csv

# 目录递归 + 模糊过滤 + 跳过不兼容文件
ux --fuzzy prodconfig --skip-incompatible -e 'jq(.database.host)' .

# 需要 EOF 前可见的管道场景
tail -f app.log | ux --line-buffered -f text -O text -e 're(r"ERROR: (?<m>.*)") | jq(.m)'
```

> **Git Bash / MSYS2 注意**：在 Windows 的 Git Bash 里跑上面含 `//a`、`//b:book` 的表达式时，MSYS2 会把 `//x` 当 UNC 路径改写参数，导致 XPath 静默零匹配。执行前设 `MSYS2_ARG_CONV_EXCL='*'`（或用 PowerShell）。这是验证环境的坑，不是 `ux` 的解析缺陷。

---

## 5. CLI 契约

> **状态说明（2026-11-14 更新）**：§5.1 参数表已**全部实现**于 `src/cli.rs`（v0.2.0），`Cli::validate` 是唯一的前置参数校验入口，DSL/选择器编译与类型上界检查在 `Pipeline::compile` 中同样前置（P5）。§5.1–§5.6 现在既是目标契约也是现状描述；与 v0.1 的破坏性差异见 §5.8 与 `CHANGELOG.md`。

### 5.1 参数表

```text
ux -e PIPELINE
   [-f auto|json|jsonl|yaml|toml|xml|html|text|csv|tsv]      # 默认 auto
   [-O jsonl|text|json]                                      # 默认 jsonl
   [--compact]                                               # -O json 关闭缩进
   [--line-buffered]                                         # 每结果 flush，见 §5.5
   [--text-mode lines|raw]                                   # 默认 lines
   [--ns PREFIX=URI]...                                      # 可重复，仅前缀解析，见 §6.4
   [--fuzzy QUERY] [--max-depth N]
   [--exclude GLOB]... [--no-default-ignore]                 # 可重复
   [--skip-incompatible]
   [--encoding utf-8|utf-16le|utf-16be|gbk|gb18030|auto]    # 默认 utf-8
   [--force-text]                                            # 跳过二进制探测
   [--max-input-size SIZE]                                   # 默认 256MiB，0 = 不限
   [--max-record-size SIZE]                                  # 默认 8MiB，0 = 不限
   [--max-results N]                                         # 默认 0 = 不限
   [--with-source] [--strict]
   [PATH|GLOB|-]...
```

`SIZE` 语法：`N` 字节，或 `N` + `KiB|MiB|GiB`（大小写敏感，1024 进制），解析失败 → `Usage(2)`。`N` 用 `u64` 解析，溢出 → `Usage(2)`。`--max-depth 0` 表示只处理目录自身条目（不递归）。

### 5.2 参数校验矩阵（P5：全部在读取输入前完成）

| 组合 | 结果 | 状态 |
|---|---|---|
| `--with-source` + `-O text` | `Usage(2)`，提示改用 jsonl/json | **[已实现]** |
| `--fuzzy` 出现 | 合法，作为后置过滤；空 `QUERY` → `Usage(2)`（原 fail-fast 已在阶段 2 移除） | **[已实现]** |
| `-` 出现 > 1 次 | `Usage(2)` | **[已实现]** |
| `--exclude` 非法 glob | `Usage(2)`，指出具体 pattern | **[已实现]** |
| `SIZE` 语法非法 / 溢出 | `Usage(2)` | **[已实现]** |
| 多个 `slurp()` / 静态类型上界为空 | `Query(2)`，指出阶段序号与类型集合 | **[已实现]** |
| `--compact` 与非 `-O json` | 忽略并在 stderr 给一行提示，不报错 | **[已实现]** |
| `--encoding` 与任意 `-f` | 合法（编码对所有格式都生效）；v2 那句「与 `--text-mode raw` 之外的模式无关」是笔误，已删 | **[已实现]** |
| `--force-text` + 显式单文件 | 合法，按文本强制处理 | **[已实现]** |
| `--ns` 前缀重复 | 后者覆盖前者，stderr 一行 warning | **[已实现]** |
| `--max-results N` 与 `slurp`/`-O json` | 合法：限制的是最终送 sink 的条数 | **[已实现]** |

### 5.3 输入与文件发现（v2 的六处未定义，v3 补齐）

- 无路径：读 stdin；`-` 显式表示 stdin。
- **处理顺序**：按命令行给出的路径顺序展开；每个路径内部按**规范化相对路径字典序**排序。排序基准为「该输入路径自身」：输入是目录时相对该目录，是 glob 时相对 glob 的**非通配前缀目录**，是显式文件时只有一项。`-` 按它在参数中的位置插入，不参与排序。**这条基准必须固定**，否则 `.` 与 `../other` 混用时输出顺序不可复现。
- **目录**：`walkdir` 递归，`filter_entry` **在遍历时**剪枝（不是先走完再过滤）；默认忽略目录名：`.git`、`.hg`、`.svn`、`target`、`node_modules`、`.venv`、`__pycache__`、`.trae`。默认忽略表按**目录名**匹配，与「隐藏文件不默认忽略」并不矛盾（后者按文件名）；`.env`、`.npmrc` 等隐藏文件正常处理。`--no-default-ignore` 清空该表；`--exclude GLOB` 在默认表**之后追加**，匹配完整相对路径。
- **符号链接**：不跟随（`walkdir` 默认），避免循环；若目录路径本身是 symlink，允许进入一次并在文档说明。
- **glob**：`*`、`?`、字符类、`**`；展开后按规范化绝对路径去重；**空匹配不等于错误**：所有输入路径展开后为空 → 无输出、退出 0、stderr 一行 `ux: no input files matched`（P2 要求可观测）。显式传入的不存在文件仍按打开失败处理（退出 1）。
- **`--fuzzy QUERY`**：**后置过滤器**（先全量枚举再过滤，不做目录剪枝），匹配对象是规范化相对路径；命中判定为 `nucleo_matcher` 打分 `> 0` **且**查询字符全部匹配（避免短查询命中过多）；大小写不敏感；同分按字典序；打分器使用固定种子保证 P1。`--max-depth` 是逃逸阀。打开文件用**原始 `Path`**，绝不用匹配后的字符串反推。
- **遍历错误**：单个条目权限/IO 错误 → stderr warning + 跳过，不计入退出码；若因此导致零文件被处理，追加 `no input files matched`。`--strict` 时遍历错误改为终止。
- **串行**：文件处理永远串行（P1），不得引入并行；理由写进 ADR（§9）。

### 5.4 编码、BOM、二进制与体积（v2 的自相矛盾已修）

v2 一边规定 `auto` 要按 BOM 识别 UTF-16LE/BE，一边规定「前 4KiB 含 NUL 即判二进制」。**UTF-16LE 文本每隔一个字节就是 NUL，会被自己的规则全部跳过。** v3 用固定的探测顺序解决：

```
① 读取前 4 KiB 原始字节
② 由 --encoding 或 BOM 确定编码（UTF-8 / UTF-16LE / UTF-16BE / UTF-32 / GB18030 回退）
③ 二进制探测：
   - 编码为 UTF-16/UTF-32 → 豁免 NUL 规则，只用「非可打印字节占比 > 30%」
   - 其余编码 → 前 4 KiB 含 NUL，或非可打印字节占比 > 30% → 判二进制
④ 解码为 UTF-8 String
⑤ 剥离 BOM（BOM 不是内容；对 raw 模式同样剥离）
⑥ 格式探测（此时看到的是已解码、已剥离 BOM 的前缀，detect_format 不再需要猜 BOM）
⑦ 按格式交给对应 reader
```

- 依赖 `encoding_rs`（纯 Rust），**v2 的 §3.2 依赖表漏了它**。
- `--encoding auto`：BOM 判定 → 严格 UTF-8 → 回退 GB18030；成功回退时 stderr 打印 `` `path`: assumed GB18030 `` 以便复现。`gbk` 与 `gb18030` 显式指定时不做回退。
- **解码层必须包在 `BufRead` 之外**：现有 `process_reader(&mut dyn BufRead)`、`csv::ReaderBuilder::from_reader`、`lines()` 都假定 UTF-8。阶段 4 需引入 `InputSource`，对外仍暴露 `BufRead`，内部完成解码 + BOM 剥离；`fill_buf()` 的格式嗅探看到的就是干净前缀，顺序不再有歧义。
- **UTF-16 的 XML 声明**：解码后内容已是 UTF-8 字符串，而 XML 声明可能仍写 `encoding="UTF-16"`，`sxd-document` 可能据此重复解码。阶段 4 需在解码层规范化/移除该声明并加用例；未落地前列为已知限制（§10）。
- **二进制跳过**：目录发现中遇到 → warning + 跳过，退出码 0；**显式文件**被判二进制 → 报错退出 1（而不是静默跳过），避免「传了一个文件、零输出、退出 0」被误读为零匹配（P2）。`--force-text` 两种情况都强制按文本处理。
- **体积**：`--max-input-size` 只约束整文档格式（json/yaml/toml/xml/html/text raw）；`--max-record-size` 约束单条 JSONL 行、text 行、CSV 记录、`split` 片段 —— **v2 说「逐条流式不受限制」是错的**：`BufRead::lines()` 会把整行读进内存，单行 1 GiB 照样 OOM。超限报明确错误并提示当前限制与调参方式，不截断、不 OOM。
- **行终止符**：`lines` 模式剥离行尾 `\r`（兼容 CRLF）；`raw` 模式保留原始内容。这只影响**输入**；输出换行恒为 LF（§5.5），两者不矛盾。

### 5.5 输出与 flush

- `-O jsonl`（默认）：每结果一行紧凑 JSON；`Text`/`Node` 输出 JSON 字符串（`Node` 取 string-value）。
- `-O text`：`Text` 原样；`Node` 取 string-value；`Data` 字符串原样、其余紧凑 JSON；每结果以 `\n` 结束。
- `-O json`：聚合为一个 JSON 数组，全部输入成功后一次输出；默认缩进 2 空格，`--compact` 关闭。零结果输出 `[]`。
- **换行恒为 LF**，不做平台 CR 转换（否则管道到 `grep`/`Select-String` 才暴露）；`raw` 内容里的原始 `\r` 不处理。
- **flush 策略（修正 v2）**：v2 把「每产出一个结果即 flush」写成契约并标「已有实现」，等于把 N 次 syscall 的性能缺陷合法化。v3 改为：`OutputSink` 内部 `BufWriter`（64 KiB），缓冲满或 `finish()` 时刷新；**`--line-buffered` 或 stdout 为终端时**（`main.rs` 用 `IsTerminal` 判定并传入）每结果 flush。需要 EOF 前可见的管道场景显式加 `--line-buffered`。`-O json` / `slurp` 本就不流式，忽略该 flag。流式回归测试相应断言「加 flag 时 EOF 前可见 / 不加时输入关闭后可见」（§8.3）。
- `--with-source`：每项包为 `{value, source:{path,line,record}}`，缺失字段为 `null`；与 `-O text` 互斥（§5.2）。
- `--max-results N`：达到第 N+1 条时以 `AppError::Runtime` 终止（退出 1），**不静默截断**；选择「错误而非正常结束」是为了不与零匹配/正常完成混淆（P2）。已写出的前 N 条保留。
- stdout 只写数据，诊断全在 stderr。

### 5.6 各格式输入语义

| 格式 | 输入项 | 流式边界 | 体积约束 |
|---|---|---|---|
| JSON | 整个文档一个 `Data` | 完整解析 | `--max-input-size` |
| JSONL/NDJSON | 每个非空行一个 `Data`，保留行号 | 逐行解析并立即输出 | `--max-record-size` |
| YAML | 每个 YAML 文档一个 `Data`；非字符串映射键报错 | 全文读入后逐文档解析 | `--max-input-size` |
| TOML | 根表一个 `Data`；日期时间转字符串；NaN/Inf 报错 | 完整解析 | `--max-input-size` |
| XML | 根节点一个 `Node` | 完整解析，格式错误报错 | `--max-input-size` |
| HTML | 根节点一个 `Node` | `sxd_html` 容错解析，完整构建 DOM | `--max-input-size` |
| text/log | 每行一个 `Text`（**空行也算一条**），或 raw 全文一个 `Text` | lines 逐行 | `--max-record-size` / `--max-input-size` |
| CSV/TSV | 首行表头，每条记录一个字符串字段对象；空/重复表头报错 | 逐记录 | `--max-record-size` |

`-f auto`：先扩展名；stdin/未知扩展名只保守嗅探 JSON（`{`/`[`）与 HTML/XML（`<`），否则按文本。YAML/TOML/CSV/JSONL 在未知扩展名下必须显式指定 —— 格式一旦确定，解析失败即报错，**不降级重试**。注意 JSONL 若被误判为 JSON 会整文档解析失败，这是已知限制（§10），需要显式 `-f jsonl`。

CSV 字段一律字符串：`jq(select(.n > 5))` 会因字符串比较失效，文档必须写明「先 `jq(.n |= tonumber)`」。

### 5.7 `--skip-incompatible` 的准确边界（v2 名不副实）

v2 说用 `CompiledStage::accepts_source(InputFormat)` 静态判定，**只对首个阶段与文件源类型做判定**，却把 flag 命名得像能解决所有混合格式问题。v3 明确：

| 首阶段 | 需要类型 | json/yaml/toml/csv/tsv (Data) | xml/html (Node) | text (Text) |
|---|---|---|---|---|
| `jq` | Data, Text | 兼容 | 不兼容 | 兼容 |
| `xpath` / `attr` | Node | 不兼容 | 兼容 | 不兼容 |
| `re` / `split` | Text | 不兼容 | 不兼容 | 兼容 |
| `text` / `slurp` | 任意 | 兼容 | 兼容 | 兼容 |

- 判定复用 §2.3 的 `TypeSet`；`-f auto` 时用**运行时探测结果**（探测已完成，仍早于求值）；stdin 同样适用。
- 跳过整文件 → stderr 一行 warning，**不计入** `had_errors`。
- **中段不匹配不在此列**：`jq(...) | xpath(...)`、`jq(...) | re(...)` 这类最常见的组合，中段类型错误仍按失败处理（退出 1）。`--help` 必须写「仅跳过首阶段与源文件类型不兼容的文件」，不得暗示通用容错。
- 静态上界检查（§2.3）会在编译期直接拒绝「确定不可能」的阶段链，与这个 flag 是两层不同机制。

### 5.8 退出码、诊断与破坏性变更

| 退出码 | 含义 |
|---|---|
| 0 | 成功（含零匹配）；`BrokenPipe` 正常终止；二进制/不兼容文件被跳过且仍有其它输入 |
| 1 | 输入/解析/求值/输出错误；`--max-results` 触顶；显式文件被判二进制 |
| 2 | 参数矛盾、未实现的参数、DSL/jq/XPath/正则编译错误、静态类型上界检查失败 |

**破坏性变更清单（v0.1 → v0.2）**，逐条需要 CHANGELOG 与 README 同步：

| 变更 | 旧行为 | 新行为 | 状态 |
|---|---|---|---|
| `--with-source -O text` | 静默输出 JSON 行 | `Usage(2)` | **[已实现]** |
| `--fuzzy` | 静默全量输入 | 正常后置过滤（阶段 0 曾短暂 fail-fast，阶段 2 已移除） | **[已实现]** |
| `-` 多次 | `Query(2)` | `Usage(2)`（文案更准） | **[已实现]** |
| `jq` 的 `nan`/`infinite` | 运行期错误 | `null` | **[已实现]** |
| 默认 flush | 每结果 syscall | 批量；管道需 `--line-buffered` | **[已实现]** |
| `-O json` | 紧凑单行 | 默认缩进 2 空格 | **[已实现]** |
| 空 glob | 打开失败（退出 1） | 无输出 + warning + 退出 0 | **[已实现]** |
| 显式二进制文件 | 按文本读 | 报错退出 1 | **[已实现]** |

0.x 阶段允许破坏性变更，但必须在 CHANGELOG 给出「旧命令 → 新命令」的迁移对照。

---

## 6. 模块与依赖

### 6.1 项目结构

```text
src/main.rs      clap 解析、IsTerminal 判定、退出码映射        [已实现]
src/lib.rs       编排：validate → compile → discover → session/relay → finish  [已实现]
src/cli.rs       参数定义、Cli::validate、parse_size、参数校验矩阵  [已实现]
src/dsl.pest     pest PEG：管道切分、括号/字符串配平、任意井号 raw string  [已实现，见 §4.2]
src/dsl.rs       StageSpec 语义映射、转义解码、嵌套深度守卫        [已实现]
src/types.rs     TypeSet 与静态上界检查、Limits                  [已实现]
src/engine.rs    Pipeline / Session / Relay / ItemSink 执行模型   [已实现]
src/model.rs     Item / Value / Origin / ItemSink trait          [已实现]
src/stage/       jq / regex / xpath / text / attr / slurp / split 适配器  [已实现]
src/source.rs    InputSource：编码解码、BOM、二进制探测、体积、XML 声明规范化  [已实现]
src/input.rs     逐格式读取、emit 回调、行/记录上界             [已实现]
src/markup.rs    Package 生命周期、XML/HTML 构建                [已实现]
src/discover.rs  目录递归、glob、fuzzy、排序去重                [已实现]
src/output.rs    jsonl/text/json、source 包装、flush 策略        [已实现]
src/error.rs     AppError（Query / Usage / Eval / Runtime / Io）  [已实现]
tests/cli.rs     端到端 golden、流式回归、HTML 容错专项(26)      [已实现]
tests/jq_compat.rs  jq 兼容集(3)                                [已实现，见 A11]
fuzz/            libfuzzer DSL 解析 target（独立 workspace）     [已实现，见 A12]
.github/workflows/ci.yml  fmt/clippy/test/release 双平台门禁    [已实现]
README.md / CHANGELOG.md  用户文档与破坏性变更迁移对照          [已实现]
```

v2 的结构表只补了 `discover.rs`，漏了编码层与类型检查的归属，本版补齐；阶段 1–6 落地后再补齐 `types.rs`、`source.rs`、`tests/`、`fuzz/`、CI 与文档的实际位置。

**实现细节与 v3 草案的两点差异**（契约不变，只改实现路径）：

- §3.3 草案写 `Session` 带 `failed: bool`；实际实现没有该字段，聚合出错不输出部分结果由 `lib.rs` 的 `had_errors` 在 `session.finish()` 前统一拦截，语义等价且状态更少。
- §3.3 草案让 `input::emit` 直接接 `&mut dyn FnMut(Item<'_>)`；实际引入 `model::ItemSink` trait 对象，避免闭包生命周期与 `Node<'doc>` 借用耦合，I1 仍由 `accept()` 调用期保证。

### 6.2 依赖与锁定

当前 `Cargo.toml`（**v0.2.0 实际内容**，`anyhow` 已移除、`encoding_rs` 已写入）：

```toml
[package]
name = "uniextract"; version = "0.2.0"; edition = "2024"

[dependencies]
clap = { version = "4.5", features = ["derive"] }
csv = "1.3"; encoding_rs = "0.8"; glob = "0.3"
jaq-core = "3.1.2"; jaq-std = "3.0.4"
jaq-json = { version = "2.0.3", features = ["serde"] }
nucleo-matcher = "0.3"; pest = "2"; pest_derive = "2"; regex = "1"
serde = { version = "1", features = ["derive"] }
serde_json = { version = "1", features = ["arbitrary_precision"] }
serde_yaml_ng = "0.10"; sxd-document = "0.3"; sxd_html = "0.1.3"; sxd-xpath = "0.4"
thiserror = "2"; toml = "0.9"; walkdir = "2"

[dev-dependencies]
assert_cmd = "2"; predicates = "3"; tempfile = "3"
```

v2 的 §3.2 一边要求 GB18030 解码一边漏掉 `encoding_rs`；v3 显式登记，阶段 4 已真正写入并带来 `simdutf8` / `core_detect` 等传递依赖。`fuzz/` 使用独立 `[workspace]`，`libfuzzer-sys` 不进入主 crate 依赖图，普通 `cargo test` 不需要 nightly。

**DSL 解析器依赖变更（v0.2.0 PEG 化）**：移除直接依赖 `winnow = "0.7"`（原实现只用了 `take_while` 读阶段名），改为 `pest = "2"` + `pest_derive = "2"`，语法落在 `src/dsl.pest`，由 derive 宏在编译期生成 `Rule`。`Cargo.lock` 中仍存在 `winnow 0.7.15`（`toml 0.9`）与 `winnow 1.0.5`（`toml_parser 1.1.5`）作为**传递依赖**（`cargo tree -i winnow@<version>` 复核），这不代表 `src/` 仍在使用它。`pest 2.9.6` 的默认 `std` feature 无条件带入 `pest_stack_guard` → `stacker` → `psm`/`cc` 等构建期依赖（含少量平台栈辅助 C/汇编），`cargo tree -i pest_stack_guard` 实测可达；`stack-guard` 只是再启用动态栈扩展，本项目已有 128 层限深，不启用（A16）。运行时不链接任何外部 C 库。

1. **jaq 三件套跨版本线**由 `Cargo.lock` 锁死；任一方升级必须先跑 §8.5 的 jq 兼容集，不允许仅 `cargo update` 后提交。
2. `fuzzy` 打分对版本敏感，`nucleo-matcher` 同样锁定；升级需重跑 §8.2 的同分排序 golden。
3. `sxd_html = 0.1.3` 在当前 `Cargo.lock` 解析到 `html5ever 0.38.0`（实证），因此不需要自研 TreeSink；残余风险是上游 crate 体量小、维护活跃度低，处理方式是锁版本 + 自有容错测试集。

### 6.3 jaq 数值契约（v2 事实错误，按源码与实测重写）

**v2 写错的两条，实测结果（2026-11，jaq-json 2.0.3）：**

| v2 断言 | 实测 | 依据 |
|---|---|---|
| 「超出 `isize` 范围的整数 → 报错，禁止静默降为 f64」 | **不报错，也不降为 f64**。`Num` 有 `Int(isize)` / `BigInt(Rc<BigInt>)` / `Float(f64)` / `Dec(Rc<String>)` 四变体，大整数进 `BigInt`/`Dec`。`{"n":123456789012345678901234567890}` 经 `jq(.)` 原样保真 | `jaq-json/src/num.rs` + 本地回归测试 |
| 「NaN/±Inf 在 jaq 侧输出为 null（与 jq 一致）」 | **正好相反**。`Num::Display` 打印 `NaN`/`Infinity`/`-Infinity`；jaq 源码注释明确说「jq 把 nan 输出 null，jaq 与之不同，jaq 可能产出非法 JSON」。v0.1 的 `to_string → from_str` 回环因此直接抛 `jq result cannot be represented as JSON` | `jaq-json/src/write.rs:279` + 本地实测 |

**v3 契约：**

- `jaq-json` 的 `serde` feature **只实现 `Deserialize`，没有 `Serialize`**，所以不能简单 `serde_json::to_value(val)`。v2 §0.3 说「保留字符串回环」是无奈但错误的保守。
- **[已实现]** 改为显式 `val_to_json(&Val) -> JsonValue` 枚举遍历：`Int → Number::from`、`BigInt`/`Dec → serde_json::from_str::<Number>(字面量)`（配合 `arbitrary_precision` 保留原始字面）、`Float` 有限值 → `from_f64`、**非有限值 → `null`**（采用 jq 语义）、`TStr`/`BStr → String::from_utf8_lossy`、数组/对象递归（非字符串对象键用其紧凑 JSON 文本）。
- 输入侧仍用 `serde_json::from_value::<Val>`（`Deserialize` 可用）；实测 `arbitrary_precision` 组合下数字不会退化为对象。
- 回归测试固定三件事：大整数字面量保真、`nan`/`infinite`/嵌套 `[nan, infinite]` → `null`、JSON 字符串输出回落 `Value::Text`。
- 已知偏差：非 UTF-8 字节串按 lossy 转换（无输入路径会产生 `BStr`，仅 jq 内建函数可能）、非字符串对象键退化为文本。两者写入 §10。

### 6.4 已证实的依赖边界（沿用 v2，补 help 一致性）

- **HTML 可用**：`//a`、`//script | text()`（`a < b` 不被当标签）、`//svg:circle` 均通过（`markup.rs` 单测）。阶段 6 已补齐 `tests/cli.rs` 的四条容错专项，实测固定如下（升级 `sxd_html`/`html5ever` 必须重跑）：
  - 隐式 `<tbody>`：html5ever 会插入 `tbody`，`//table/tr` **零匹配**，`//table/tbody/tr` 才命中。这是最容易让用户以为「XPath 坏了」的一点，已写入 §10。
  - `<template>`：内容**会**展开进 XPath 可见的树，`//p` 能取到模板内节点。
  - `<noscript>`：在 scripting 开启的解析模式下内容是**原始文本**，内部标签不会成为元素节点；`//noscript | text()` 返回未解析的标记文本。
  - 元素名/属性名**折叠为小写**：大写源码 `<A HREF="/u">` 只能用 `//a` + `attr("href")` 命中，`//A` + `attr("HREF")` 零匹配。
  - 验证环境坑：Windows Git Bash / MSYS2 会把 `-e` 参数里的 `//a` 当 UNC 路径改写，导致静默零匹配；见 §4.4 与 README。
- **XPath 命名空间是残的**：`sxd-document` 节点名是「含前缀的字符串」，不建模 namespace URI；`Context::set_namespace` 只做表达式前缀解析。默认命名空间（`xmlns="urn:books"`）与等价但不同前缀的文档都匹配不上 `--ns b=...` 的 `//b:book`。
  → **[已实现]** `src/cli.rs` 的 `--ns` help 文案已改为「XPath expression prefix binding (PREFIX=URI); the URI is NOT matched against document namespaces」，README 与 §10 同步给出「按文档实际前缀写表达式」的绕过办法。真 URI 匹配需换 `roxmltree` 后端，属 v4。
- **XPath 1.0 完整性**：nodeset/string/number/boolean 四类结果已正确映射；`id()`、`lang()` 等未实测，**不要在文档里承诺 XPath 1.0 完整兼容**。

---

## 7. 实施计划与完成情况（阶段 0–6 全部完成）

v2 把**架构级改动、设计最含糊**的聚合排在第一阶段，却把风险最低、能立刻消除死开关的收口排在最后。v3 按「先止血 → 再铺路 → 后动架构 → 最后加固」排序，且每阶段给出可断言的观测点。**该顺序在阶段 1–6 实施中被完整遵循，所有阶段于 2026-11-14 完成（v0.2.0）。** 下表保留原始可断言验收标准作为回归依据。

| 阶段 | 内容 | 前置 | 验收（可断言） |
|---|---|---|---|
| **0 止血与一致性** **[已完成]** | 参数校验集中到 `Cli::validate`（互斥 / 死开关 / `-` 多次）、`AppError::Usage(2)`、jq 数值契约修正与回归测试；移除未使用的 `anyhow`；全量 `cargo fmt` 使 fmt/clippy 门禁归零 | 无 | 阶段 0 当时的基线：`cargo test` 15 项通过、`cargo fmt --check` 与 `cargo clippy --all-targets -- -D warnings` 均通过；`ux -e 'text()' -O text --with-source` 退出 2 且 stderr 含 `mutually exclusive`；`'null' \| ux -e 'jq(nan)' -f json` 输出 `null`。**历史项**：`ux -e 'text()' --fuzzy x` 当时退出 2，阶段 2 起已改为正常过滤，勿再作为回归断言 |
| **1 契约静态化** **[已完成]** | `src/types.rs`：`TypeSet`、`input_types`/`output_types`、`Pipeline::compile` 上界检查；`attr` 支持 Attribute 节点；补 DSL 的 `slurp`/`split` 解析与参数校验 | 阶段 0 | `slurp() \| xpath("//a")` 编译期 `Query(2)`；`jq(.) \| xpath(...)` 同样；`xpath("//@href") \| attr("href")` 输出属性值；DSL 单测覆盖两个新阶段 |
| **2 文件发现** **[已完成]** | `src/discover.rs`：目录递归 + `filter_entry` 剪枝、glob、fuzzy、去重排序、`--max-depth`、`--exclude`、`--no-default-ignore`；`lib.rs` 改为「发现 → 逐个处理」；**移除 `--fuzzy` 的 fail-fast** | 阶段 1 | `ux --fuzzy prodconfig -e 'text()' <fixture>` 只输出命中文件；同分排序两次运行 `diff` 为空；在 `target/` 放哨兵文件，断言输出中不存在（证明剪枝发生在遍历期）；空 glob 退出 0 + warning |
| **3 会话执行与聚合** **[已完成]** | 按 §3.3/§3.4 重写 `engine.rs` 为 `Pipeline`/`Session`；`input::emit` 改回调；实现 `slurp`/`split` | 阶段 1（类型表已就绪） | 双文件 `slurp() \| jq(length)` 给总数；零输入输出 `[]`；`slurp() \| xpath(...)` 编译期被拒；`slurp` + `-O json` 输出 `[[...]]` 并写进文档；聚合中途出错时 stdout 为空、退出 1 |
| **4 输入卫生** **[已完成]** | `src/source.rs`：解码层、BOM 剥离、编码选项、二进制探测与 UTF-16 豁免、`--max-input-size` / `--max-record-size`、CRLF | 阶段 2（发现层已提供 `Path`） | 带 BOM 的 JSONL 首行可抽取；GBK 日志 `--encoding gbk` 输出正确中文；UTF-16LE 文本不被判二进制；含 PNG 的目录不输出二进制且退出 0；显式 PNG 退出 1；单行超限给明确错误而非 OOM |
| **5 输出与 CLI 收口** **[已完成]** | `-O json` 缩进 + `--compact`；flush 策略 + `--line-buffered` + TTY 判定；`--max-results`；`--ns` help 文案；`--compact` 误用提示；CHANGELOG/README/迁移对照 | 阶段 3（结果计数已稳定） | `-O json` 人可读且 LF 结尾；`--with-source -O text` 退出 2；`--max-results 10` 在第 11 条退出 1；`--line-buffered` 下 EOF 前可见输出；`ux --help` 输出中含命名空间限制说明 |
| **6 测试与加固** **[已完成]** | `tests/` 端到端 golden、流式回归、可执行兼容集（替代生成器，见 A11）、fuzz target、CI 配置、HTML 容错专项 | 阶段 5 | §8 全部通过；`cargo fmt --check` / `clippy -D warnings` / `test` / `build --release` 全绿；release 二进制逐条跑 §4.4 九个示例 |

**实测收尾数据**：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --all-targets`（46 单测 + 26 CLI + 3 兼容集 = 75 项）、`cargo build --release` 全绿；§4.4 九个示例在 release 二进制上逐条通过，退出码 0。

**阶段 3 排在阶段 2 之后的理由（实施中已验证）**：它是唯一会改动 `lib.rs` 编排与 `input::emit` 接口的阶段，先验证完「发现 → 逐文件」链路再动手，避免同时调试两套不确定性。实施结果证明这个顺序有效：阶段 3 重写 `engine.rs` 时，`discover.rs` 的输出契约已经稳定。

---

## 8. 验证方案

### 8.1 单元测试（现状 56 项）

原有：DSL（嵌套括号、jq 内部 `|`、原始字符串、空/未闭合阶段）、output（source 包装）、markup（容错 HTML、SVG 命名空间、坏 XML）、regex（数字/命名捕获）、cli（4 项参数校验）、jq（大整数、NaN/Inf、字符串回落）。

阶段 1–6 已补齐：`TypeSet` 交集、`Limits`、DSL 的 `slurp`/`split` 解析、`split` 空片段与体积上界、`Session` 物化/零输入/双文件、`attr` 属性节点、`-O text` 三种 `Value`、LF 不变量、`OutputOptions` 的 pretty/compact/`--max-results`、`discover` 剪枝与排序、`source` 编码/BOM/UTF-16/GBK/二进制、XML 声明规范化、体积边界、`next_line` 的 CRLF。`lib.rs` 另暴露 `parse_pipeline()` 供 fuzz target 调用。

PEG 化后 `dsl.rs` 另补 10 项：任意井号 raw string、井号数不匹配、字符串内含 `)`/`|`、多字节字符紧邻分隔符、阶段内空白容忍、单独的 `r` 不误判、128 层嵌套边界与超限、2 万字符长参数不误拒、2000 条确定性乱码输入不 panic、结构错误携带字节位置。

总测试数：56 单测 + 26 `tests/cli.rs` + 3 `tests/jq_compat.rs` = **85 项**。

### 8.2 集成与 CLI golden

覆盖所有输入格式及至少一条跨引擎组合管线；断言 **stdout / stderr / 退出码** 三元组。边界清单：空输入、零匹配、非法 UTF-8、BOM、UTF-16、GBK、坏 JSONL 行、坏 XML、容错 HTML、CSV 空/重复表头、二进制、多文件部分失败、`--skip-incompatible` 的中段不匹配仍失败、stdin 与文件混用、未知扩展名、显式格式冲突、空 glob、`--max-input-size`/`--max-record-size`/`--max-results` 触发、`slurp` 跨文件与零输入、`slurp` + `-O json` 的双层数组。

### 8.3 流式回归

启动真实 `ux` 子进程，写入一条记录后**保持 stdin 打开**，用带超时的读取通道断言：

- `--line-buffered` 或 stdout 为 TTY：EOF 前收到输出；
- 默认（管道、批量）：EOF 前不要求可见，关闭 stdin 后校验最终结果；
- `-O json` 与 `slurp`：断言「输入关闭前 stdout 为空」—— 这是它们放弃流式的可观测证据。

Windows 与 Linux/macOS 的管道缓冲行为不同，测试需标记 `#[cfg]` 或使用平台专属超时；CI 当前覆盖 `ubuntu-latest` 与 `windows-latest` 两档。

**残余验证缺口（登记，不掩盖）**：三条断言中只有 `--line-buffered` 的 EOF 前可见性落地为 `tests/cli.rs::line_buffered_emits_before_stdin_is_closed`（10 秒超时通道）。「默认批量缓冲 EOF 前不可见」依赖平台管道缓冲、容易 flakes；「`-O json` / `slurp` 输入关闭前 stdout 为空」由代码结构（结果聚合到 `output.finish()`）保证，但没写黑盒断言。两条列为后续加固项，不影响 v0.2.0 的功能正确性（另见 §0.5 偏差 3）。

### 8.4 退出码矩阵

对 §5.2 每行组合各一条 golden，断言退出码且 stderr 含约定关键字。特别固定「零匹配 = 0」「全部文件被跳过但有 warning = 0」「显式二进制文件 = 1」「参数矛盾 = 2」。

### 8.5 兼容性矩阵与工具落地（v2 悬空项，v3 补齐位置；实施有偏差，见 A11–A12）

- **jq 兼容集【已实现，方式与 v3 草案不同】**：落地为 `tests/jq_compat.rs` 的**可执行兼容集**（3 项大组，覆盖字段访问、数组迭代、`select`、对象构造、内层管道、`map`/`length`/`keys`/`tonumber`/`add`/`sort`/`unique`、`nan`/`infinite` → `null`、大整数、运行期 `error()`、非法语法编译期退出 2）。**未实现** `xtask + COMPAT.md` 生成器：函数表只能证明「内建存在」，不能证明「语义与 jq 一致」，且多一套生成物与 `git diff` 漂移检查；可执行断言由 `cargo test` 直接兜住，理由记入 A11。`jaq-*` 升级时仍必须重跑此集（§6.2 第 1 条不变）。
- **HTML 容错集【已实现】**：§6.4 列出的四个未覆盖点各一条 `tests/cli.rs` 用例（隐式 `<tbody>`、`<template>`、`<noscript>`、大小写折叠），随 `sxd_html` 升级重跑；实测结论已写入 §6.4。
- **fuzz【骨架已实现，CI smoke 未落地】**：`fuzz/fuzz_targets/dsl_split.rs` 调用 `uniextract::parse_pipeline()`，种子语料取 §4.2 的五个解析面；`fuzz/Cargo.toml` 用独立 `[workspace]`，避免主 crate 的 `cargo test` 依赖 nightly/libfuzzer。**CI 未跑 60 秒 smoke** —— 理由与替代做法见 A12。PEG 化后该 target 与入口函数不变（`parse_pipeline` 仍只调 `dsl::parse`）；另在 `dsl.rs::malformed_inputs_never_panic` 内用固定 LCG 种子生成 2000 条乱序 token 串并覆盖五个解析面，作为**无需 nightly、随 `cargo test` 自动执行**的最小 fuzz 替代。

### 8.6 最终命令（实际验收链路）

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

再用 release 二进制逐条跑 §4.4 全部九个示例，断言 stdout、退出码与流式行为。**v0.2.0 收尾实测：四条命令全绿，75 项测试通过，九个示例退出码 0。**

在 Windows Git Bash 下跑示例前必须 `export MSYS2_ARG_CONV_EXCL='*'`，且 fixture 用 heredoc 写入（`printf` 的格式串会被 MSYS 把 `\n` 改成 `/n`）；两者都会造成静默零匹配/错误结果，已在收尾时踩过并记入 §4.4 与 README。cleanup 不要用 `rm -rf` 删临时目录（safe-delete 拦截），把 fixture 放在 gitignored 的 `target/` 下即可。

---

## 9. 决策记录（ADR 摘要）

| 编号 | 决策 | 理由 | 放弃的替代方案 |
|---|---|---|---|
| A1 | 保留 `Item<'doc>` 借用模型，不引入 arena | 借用检查器已强制 I1，arena 增加索引协议与回查开销 | arena + 稳定索引 |
| A2 | 类型检查用「集合上界 + 运行期值级」两层，不做静态类型系统 | 可抓确定不可能的组合，成本可控；精确判定需重构整个值模型 | 完整静态类型推导 |
| A3 | `slurp` 覆盖整次运行（跨文件），不是一个文件一次 | `jq -s` 的心智模型；多文件统计是主要用例 | 每文件聚合（无法算总数） |
| A4 | 聚合出错时不输出部分结果、退出 1 | 聚合本就不流式，半截统计比没有统计更危险（P2/P3） | 尽量输出已缓冲部分 |
| A5 | 默认批量 flush，逐条 flush 需 `--line-buffered` | 每结果 syscall 在大文件上是数量级开销；交互场景由 TTY 自动兜住 | 恒每结果 flush |
| A6 | 文件处理串行 | 输出顺序与 `record` 编号必须可复现（P1） | rayon 并行（顺序不确定） |
| A7 | 单文档格式 `line = null` | 没有行概念就不伪造信息 | 填 1 |
| A8 | 非有限浮点映射 `null`，用显式 `val_to_json` 去掉字符串回环 | jaq 会打印非法 JSON；字符串回环既报错又威胁精度 | 保留 `to_string → from_str` |
| A9 | `--fuzzy` 未实现期间 fail-fast（阶段 2 后移除） | 死开关静默全量输入是「骗」，比不可用更糟（P2）；发现层落地后即恢复过滤 | 继续静默忽略 / 永久禁用 |
| A10 | XPath 命名空间维持「仅前缀解析」 | 换 DOM 后端成本远超收益，属 v4 | `roxmltree` 替换 |
| **A11** | jq 兼容矩阵用 `tests/jq_compat.rs` 可执行断言，不用 `xtask + COMPAT.md` 生成器 | 生成式函数表只能证明「内建存在」，不能证明「语义与 jq 一致」；可执行集随 `cargo test` 自动防漂移，零额外工具链与生成物 | `xtask` 导出 `jaq_std::defs()` + `git diff --exit-code`（v3 §8.5 原案） |
| **A12** | `fuzz/` 用独立 `[workspace]`，CI 不跑 60 秒 smoke | cargo-fuzz 需 nightly + libfuzzer，会拖慢双平台常规门禁；主 crate 的 `cargo test` 不应依赖 nightly。保留 target 与种子语料，按需本地跑 | 把 fuzz 加进 workspace 并在每次 PR smoke（v3 §8.5 原案） |
| **A13** | `Session` 不持 `failed` 字段，聚合出错由 `lib.rs` 的 `had_errors` 在 `finish()` 前拦截 | 语义等价但状态更少；`Session` 只负责物化，退出策略集中在一处 | §3.3 草案的 `failed: bool` |
| **A14** | §4.4 XML 示例改用 `attr("sku")` 而非 `text()` | 示例 fixture 的 `sku` 是属性；`text()` 得到空字符串会让示例静默零输出，违反 P2 的「别用看似能跑的例子骗人」 | 改 fixture 为元素文本 |
| **A15** | DSL 结构解析换成 **pest 2 PEG**（`src/dsl.pest`），移除直接依赖 `winnow`；语法只有结构层，语义仍在 `dsl.rs` | 原「手写字节状态机 + winnow 只用来读阶段名」是割裂的：切分规则与语法描述分离、错误位置靠手算字节偏移。PEG 把「管道只在顶层切分」「字符串内括号不闭 stage」「任意井号 raw string」写进一份可审查的语法；`StageSpec` 仍为阶段名唯一事实来源，避免 grammar 与 Rust enum 双份定义 | ① 全量 AST 化（grammar 里枚举 7 个 stage 名，新增阶段要改两处）；② 继续手写状态机；③ 保留 winnow 只做局部替换（无法表达任意井号 raw string 的结构配平） |
| **A16** | 用 `check_nesting(128)` 限深，**不**用 `pest::set_call_limit`，也**不**启用 `stack-guard` | 实测：pest 的 `group` 间接递归在约 500 层时直接栈溢出终止进程（P2 不可接受）。`set_call_limit` 按规则调用次数计数，与输入长度线性相关——设为能拦住 500 层的值会误拒 2 万字符的长正则，设大了又拦不住；`stack-guard` 只是在默认 `std` 已引入的 `pest_stack_guard` 上额外开启动态栈扩展，嵌套已被限死后无收益（且 `psm`/`cc` 并不会因关掉它而消失；曾有一次 release 构建期 `psm` build script 偶发 `STATUS_ACCESS_VIOLATION`，依赖不变的重试均成功，不作为决策依据）。显式深度守卫只数括号、不判语法，给出可解释的 `nesting exceeds 128` | `set_call_limit(Some(N))` / `stack-guard` feature / grammar 手工展开固定层数 |

---

## 10. 已知限制（汇总，须同步 README 与 `--help`）

1. `--ns` 不实现 namespace-URI 匹配；默认命名空间与不同前缀的等价文档匹配不上。`--ns` 的 help 文案已明确写出该限制，绕过办法是「按文档实际前缀写表达式」。
2. `-f auto` 会把无扩展名的 JSONL 误判为 JSON；需显式 `-f jsonl`。格式判定后不降级重试。
3. XPath 1.0 不承诺完整兼容；只保证实测过的函数与轴。（`div` 虽是 XPath 运算符关键字，但 `//div` 作为名称测试在 `sxd-xpath` 中可正常匹配，已实测确认，不要写进限制清单。）
4. `slurp` 放弃流式，内存与上游记录数成正比；`--with-source` 只保留首条来源，无逐条来源阶段。
5. `split` / `re` 基于 `regex` crate：无回溯引用、无 lookaround。
6. 非 UTF-8 字节串（`BStr`）按 lossy UTF-8 转字符串；jaq 产生的非字符串对象键退化为紧凑 JSON 文本。
7. ~~UTF-16 输入若带 `encoding="UTF-16"` XML 声明可能重复解码~~ **已解决**：阶段 4 的 `normalize_xml_declaration()` 会在解码后剥离 XML 声明中的 `encoding=...`，并有单测覆盖。
8. **HTML 解析行为（由 `sxd_html`/html5ever 决定，升级需重跑 §6.4 用例）**：隐式 `<tbody>` 使 `//table/tr` 零匹配（须写 `//table/tbody/tr`）；`<noscript>` 内容按原始文本处理；`<template>` 内容会展开进 XPath 可见树；元素名与属性名折叠为小写。
9. **验证环境**：Windows Git Bash / MSYS2 会改写 `-e` 中形如 `//a` 的参数导致静默零匹配；需 `MSYS2_ARG_CONV_EXCL='*'` 或改用 PowerShell。这是 shell 行为，不是 `ux` 缺陷，但会误导用户，已同步 README。
10. `--skip-incompatible` 只覆盖首阶段与源类型；阶段链中段的类型不匹配仍为错误。
11. CSV 字段一律字符串，数值比较需先 `jq(.n |= tonumber)`。
12. 不跟随符号链接；不处理网络输入；输出只走 stdout。
13. **残余验证缺口**：§8.3 的「默认批量缓冲 EOF 前不可见」与「`-O json`/`slurp` 输入关闭前 stdout 为空」未写成黑盒 golden；fuzz 的 60 秒 CI smoke 未落地（A12）。
14. **DSL 括号嵌套上限 128 层**：超出返回 `Query(2)` 且 stderr 写明 `pipeline nesting exceeds 128 levels`。pest 的 `group` 是间接递归，未限深时约 500 层会栈溢出**终止进程**；守卫只统计深度并跳过字符串，不限制参数长度（长正则不受影响）。理由见 A16。
15. **DSL 是两层校验**：`dsl.pest` 保证结构（顶层管道、括号配平、字符串边界），`dsl.rs` 保证字面量合法（六种转义、raw string 井号数匹配、阶段名）。因此残缺 raw string 可能先被 grammar 的 `plain` 回退路径当作结构合法，再由 Rust 侧以 `unterminated raw string in \`re\`` 拒绝；两者退出码同为 2，但错误文案来源不同。
