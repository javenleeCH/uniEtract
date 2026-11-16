# 架构角色评审

## 一句话结论

整体架构（统一 `Item`/`Origin`/`ItemSink` 单记录流 + pest 管结构 + 结构化诊断 + 收敛 panic）是稳健、克制且可维护的，A1–A9/B1–B9 绝大多数经得起推敲；但存在一个可被构造 `-e` 触发的**深度守卫绕过 → 栈溢出 abort**（P0，直接击穿 0/1/2 契约），以及「输入端读错误包装不一致导致 BrokenPipe 分类偶然化」这一结构性根因（P1，0.4.0 的方向区分必须连带修复）。

---

## 架构概览评价

### 模块边界

分层清晰且依赖方向健康：`cli → error`，`discover/source/input/stage/pathspec → cli`，`lib` 做编排收口，`error/diagnostic/model/types` 是无反向依赖的底层。唯一真正收敛的对外面是 `cli`（lib.rs:1）+ `run`/`parse_pipeline`（lib.rs:35/269），符合 A1。

主要结构性代价是 **`Cli` 成为贯穿一切配置的 god struct**：`discover.rs`、`pathspec.rs`、`source.rs`、`input.rs`、`lib.rs` 全部直接持有 `&Cli` 并读取其 35+ 字段（cli.rs:15–134），例如 `source.rs` 要读 `encoding/force_text/max_input_size/paths`，`pathspec.rs` 要读 `exclude/fuzzy/max_depth/no_default_ignore`。小工具可接受，但新增阶段/输入/发现策略时都会再次扩大这个耦合面。建议按关注点拆成 `DiscoverConfig` / `InputConfig` / `OutputConfig` 三块，`Cli` 只负责 clap 装配（见 P1-3）。

### 数据模型

`Value<'doc>` 三变体（Data/Text/Node, model.rs:88–92）+ `Item<Origin, Value>` + `ItemSink` trait 是**正确且必要的**抽象：

- 用 trait（而非闭包）让 `Node<'doc>` 的文档借用跨模块传递而不泄漏（model.rs:13–15 的「不变量 I1」是真实的：`Item<'_>` 在 trait 方法里是 late-bound 生命周期，`&mut dyn ItemSink` 才能对象安全）。
- `Session::feed` 在边界把值物化成 `JsonValue`（engine.rs:171），`-f xml/html` 的 `Package` 在 `sink.accept` 同步调用期间存续（markup.rs:32–33），二者共同保证 `Node` 借用永不逃逸文档。这是**可靠**的设计。

代价也是结构性的：`Value` 是封闭枚举，**加一种新类型**会波及 `Value`、`TypeSet`、每个 stage 的 `input/output_types`、`into_json`、`Attrs`/`string_value`（model.rs:94–111）等 6+ 处；而**加一个新阶段（复用既有类型）成本低得多**（见「扩展性」）。若未来要引入「字节流」「时间戳」这类新 Item 类型，该枚举会成为最大阻力。

### 执行模型

串行 + 字典序 + 三态 sink 的表达是干净的。`Pipeline`/`Relay`/`Session` 三者把「无边界全校直跑」「slurp 会话聚合」「跨文件全局聚合」统一到 `ItemSink`，边界（`slurp` 至多一次，engine.rs:34–38）在编译期确认。类型上界检查（engine.rs:42–66）与运行期值级检查（stage/* 的 `else { return Err }`）**分工正确**：前者只做「相邻 hop 的声明上界交集」，且 `output_types`/`input_types` 两个方向都严格是实际行为的超集（我已逐一核对，无「误判为不可能」的假阳性），因此「非空交集 ≠ 一定兼容」的语义是**可靠的**。

唯一的不一致：`source_types(Auto) = None`（types.rs:49）使 `-f auto` 下「读入前报错」的承诺失效，`--skip-incompatible` 也恒真（engine.rs:75–80）。这是可接受的工程取舍，但文档说「fail before reading」应明确限定为「显式 `-f` 才成立」。

---

## 发现

### P0-1 · DSL 深度守卫可被未终止 raw string 提前 break 绕过 → 栈溢出 abort

**问题**：`check_nesting` 扫描到 `r`（可选 `#`）+`"` 的 raw string 前缀后，若找不到配对的终止符 `"###`，执行 `None => break`（dsl.rs:85），**直接结束整个深度扫描循环**，其后所有未配对 `(` 都不再计数。于是 pest 的 `group` 间接递归（dsl.pest:23–24）会原样执行这段深层括号——这正是 128 层守卫（dsl.rs:70–113）与 `catch_unwind` 存在的原因。

**证据**：
- dsl.rs:72–113 `check_nesting`；关键在 dsl.rs:84–85 的 `None => break, // let the grammar report the unterminated literal`。
- dsl.rs:146–159 `raw_prefix` 把任意「`r` + `#`* + `"` 且随后无终止符」判为 raw string 起点。
- dsl.pest:23 `group = _{ "(" ~ balanced* ~ ")" }` 对未配对 `(` 递归下降。
- CHANGELOG.md:127 自述「约 500 层时进程栈溢出终止」——即该 guard 想挡住的崩溃形态。

**影响**：`ux -e 're(r#"(((…(500 层)'` 一类构造（未终止 raw string 后接深括号）会绕过守卫，触发**栈溢出 → 进程 abort（`SIGSEGV`/`STATUS_STACK_OVERFLOW`），非 101 的 panic**，`catch_unwind`（lib.rs:44）与空 panic hook（main.rs:28）都**拦截不了**。直接击穿 A4 承诺的 0/1/2 稳定契约；这是可被恶意/误用 `-e` 触发的拒绝服务。

**建议方案**：`check_nesting` 绝不允许提前退出。`None` 分支不要 `break`，改为「把当前 raw string 起点之后的所有字节按 `plain` 继续扫描括号深度」（即：从 `index+1` 继续，忽略 raw-string 语义），或更简单——遇到未终止 raw string 直接返回 `Query`（它本就是非法输入，grammar 最终也会拒绝），但**先完成全量深度计数**再报错。补一条 `nesting_guard_rejects_unterminated_raw_string_then_deep_parens` 回归测试。长远看，pest 关闭 `stack-guard` 且不用 `set_call_limit` 的决策（dsl.rs:61–69）没问题，前提是自研守卫必须完整。

### P1-1 · 输入端读错误包装不一致，BrokenPipe 分类是「偶然」而非「方向」

**问题**：0.3.1 按错误**变体**判 BrokenPipe：`AppError::Io(BrokenPipe)` → 0（error.rs:78–80），`Runtime(String)` 包裹读错误 → 1。但输入端的两类读路径对 `io::Error` 的包装方式**不统一**：
- `fill_buf()?`（source.rs:47）与 `read_until()?`/`take().read_until()?`（input.rs:287/291）经 `#[from]`（error.rs:31）变成裸 `AppError::Io` → 命中 0；
- 整文档 `read_to_end()` 被 `map_err(Runtime)` 包裹（input.rs:256–258、source.rs:92–94/106–108）→ 相同 BrokenPipe 变成 `Runtime` → 1。

**证据**：上述行；lib.rs:185–187 / 219–222 / 247–249 / 258–261 四处重复的 `if error.is_broken_pipe() { return 0 }`；tests/lib_run.rs:193–231 甚至把「输入侧裸 Io BrokenPipe=0」作为**负向用例固化**了这条不一致。

**影响**：「输入端管道破裂」的退出码随**读取路径**漂移（首 `fill_buf`/逐行读返回 0，整文档读返回 1），脚本无法依赖 rc 判断输入是否读完；README:235 已诚实文档化，但根因不是「没区分方向」，而是「输入错误包装分层失守」。0.4.0 的「按输入/输出侧区分」（README:249、CHANGELOG:58）**必要但不充分**。

**建议方案**：0.4.0 实现方向区分时，**先统一输入错误表示**——给 `AppError` 增加显式的 `Read { .. }` / `Write { .. }`（或至少一个 `Side` 字段），让 `source.rs`/`input.rs` 的所有读操作走同一 wrap 函数；`is_broken_pipe` 仅对**输出侧** `Write` 生效。否则「方向」标签会继续被贴错位置。

### P1-2 · `Cli` god struct 贯穿配置，低内聚且随功能扩张加速耦合

**问题**：`discover.rs`、`pathspec.rs`、`source.rs`、`input.rs`、`lib.rs` 全部接收整棵 `&Cli`（如 pathspec.rs:355–360、source.rs:34–44、input.rs:12–16、lib.rs:35–40），各读无关字段。

**证据**：cli.rs:15–134（35 个 pub 字段）；source.rs 直接依赖 `cli.paths` 判「显式二进制」并读 `encoding/force_text/max_input_size`；pathspec.rs 读 `exclude/fuzzy/no_default_ignore/max_depth`。

**影响**：单测要构造完整 `Cli`（见 source.rs:323–325 的 `try_parse_from` 样板）；新增一个配置维度会在 5+ 个模块传染；`Cli` 事实上成为「配置总线」。

**建议方案**：拆 `InputConfig { encoding, force_text, max_input_size, max_record_size, text_mode }`、`DiscoverConfig { exclude, fuzzy, no_default_ignore, max_depth, paths }`、`OutputConfig { output, with_source, compact, line_buffered, max_results }`，`run_inner` 装配后只把所需 struct 下发。纯内部重构，不影响 CLI 契约（A1 保证）。

### P1-3 · `Origin` 在「多对一」聚合阶段的来源语义失真

**问题**：`slurp()` 把多条记录收敛为一条数组，但 `Session` 只保留 `first_origin`（engine.rs:134–136、168–170、184–187），数组的 `source.record/path/line` 只反映第一条。

**证据**：engine.rs:168–170（`if first_origin.is_none()` 只记首个）、README:228「slurp 的 --with-source 只保留第一条来源」。

**影响**：`--with-source -O jsonl` 下，聚合数组的 provenance 指向单条记录，读者无法区分「这是聚合数组」与「这是原始单条数组」；一旦后续做 captures 求值注入（0.4.0），这种「把来源当 Item 级标量」的模型会让「聚合项的来源」更含糊。

**建议方案**：短期接受并文档化；长期给 `Item` 引入 `Origin` 的**列表/聚合形态**（或给 slurp 数组项单独承载逐条 `source`），或在 `--with-source` 输出上显式标记 `"aggregated": true` 而非静默借用 first_origin。

### P2-1 · path spec 被重复编译，且编译产物不跨 validate/discover 复用

**问题**：一次正常运行里 `pathspec::compile` 至少执行两次——`Cli::validate()`（cli.rs:154–156）+ `pathspec::discover()`（pathspec.rs:364），每次都是若干 `Regex::new`（pathspec.rs:295–299）。

**影响**：纯性能/一致性冗余（同一输入结果必然相同，不算 bug），但暴露出「校验期编译」与「执行期编译」没有共享结果的架构空隙。

**建议方案**：`validate` 阶段把 `Compiled` 缓存进 `Cli`（或由 `run_inner` 编译一次后同时用于校验与 `discover`），`discover` 接受 `&Compiled`。

### P2-2 · `--path-spec` frontier 无节点数上限，扇出可致内存膨胀

**问题**：frontier 按层展开（pathspec.rs:378–436），每层 `next` 无上限，且每节点复制 `parts`/`captures`（pathspec.rs:429–431）。

**证据**：pathscr-design.md:67「frontier 按层展开而不设节点数上限：扇出由 spec 片段宽窄决定」。docs/path-spec-design.md:67。

**影响**：`{x:re:.*}` 之类宽片段在超大目录下会把整层目录条目复制进内存（与 glob `*` 的 unbounded 展开同源）。调用方可控，但作为「结构化匹配」卖点（避免全树递归）时，最坏扇出仍可能逼近全树。

**建议方案**：增加 `--max-frontier`/复用 `--max-results` 语义的可观测告警（触顶报错而非静默），或对 frontier 复制改为借用（因 64 段/32 captures 上限，复制成本本有界，问题在**节点数量**而非单个节点体积）。

### P2-3 · fuzz 覆盖停在一个 DSL parse 靶，未覆盖新增的手写扫描器面

**问题**：`fuzz/fuzz_targets/dsl_split.rs:9–12` 只 fuzz `parse_pipeline`。而 0.3.1 引入/强化的 `split_segments`、`tokenize`、`validate_re_fragment`、`glob_fragment`（pathspec.rs:57–255）是**新的手写词法器**，风险面已迁移，却完全不在 fuzz 内。dsl.rs:331–364 的自测只有确定性 PRNG 2000 条。

**影响**：`validate_re_fragment` 的括号/类/转义状态机（pathspec.rs:207–255）是「手写 vs regex 语义」最容易分歧的点（见 P2-4），没有 coverage。

**建议方案**：新增 `fuzz/fuzz_targets/pathspec_compile.rs` fuzz `pathspec::compile`（断言永不 panic 且错误必为 `Usage`）；若 libfuzzer 不便，先在单测内加大结构化生成器规模。

### P2-4 · `validate_re_fragment` 是 regex 语义的手写复刻，分歧方向是「误拒合法正则」

**问题**：B3 用自写状态机（pathspec.rs:207–255）禁止「顶层 `|`」「不配平括号」，但 Rust `regex` 还支持 `(?#...)` 注释、`(?P<name>...)` 嵌套等该状态机不懂的构造。

**证据**：pathspec.rs:227–249；例如注释 `(?# a ) b )` 内含 `)` 会被当作「逃逸捕获组」误拒。

**影响**：低风险、方向保守（**误拒**合法正则，不会放行非法），但会让「文件名拆字段」主用例偶发 `Usage(2)`，且修复需维护两份语法。

**建议方案**：保留结构校验，但把「是否逃逸」交给 regex crate 判定——拼装 `(?P<name>...)` 后验证**整段是否仍锚定**（例如用一个探针字符串验证匹配是否可能为空/是否退化），或至少为 `(?#...)` 显式跳过注释正文。折中：文档明确 B3 的覆盖子集。

### P2-5 · `run_inner` 220 行单函数 + 四处复制的 `is_broken_pipe` 判断

**问题**：`run_inner`（lib.rs:67–265）同时编排 validate/discover/pipeline/output/session/错误累积/limit/abandon，且 `if error.is_broken_pipe() { return 0 }` 在 open/process/finish 三处复制（lib.rs:185、220、247、259）。

**影响**：每加一个新错误出口都要记住重复这段；P1-1 的方向区分改造会直接触碰这三处，重复逻辑是回归温床。

**建议方案**：抽 `fn classify_read_error` / 集中一处 `handle_error(code)` 收敛退出码与 BrokenPipe；与 P1-1 一并解决。

### P2-6 · 版本/夹具/测试数被多份文档与 CI 脚本硬锚定，真理来源分散且脆

**问题**：A9 依赖 `.github/scripts/check_contract_docs.py / check_test_count.py / check_release_version.py` 把「12 夹具」「158 测试」「0.3.1 版本字符串」等硬编码在脚本里对账（docs/cli-design.md:136–154、CHANGELOG:47）。

**影响**：任何一次「加一个测试/加一个夹具」都要同步改脚本常量，门禁本身成为变更摩擦源；文档语义（README/docs/cli-design/help 三处重复 agent 契约与 BrokenPipe 措辞）靠字符串扫描防漂移，维护成本高且易误报。

**建议方案**：接受短中期；长期把「测试数/夹具数」等可计算事实改为 CI 只做「下限/存在性」断言，或让脚本从单一 fixture 目录扫描派生数量，而非硬编码。

---

## 对生效架构决策(A/B系列)的逐条看板

| # | 决策 | 结论 | 理由 |
|---|---|---|---|
| A1 | CLI 唯一稳定契约，库接口不公共化 | 认可 | `pub` 面收敛到位（lib.rs:1/35/269）；`Cli` 全 pub 是「事实上 API」但已声明非稳定，可接受 |
| A2 | 7 阶段单记录流 + 统一 `Item` | 认可（带保留） | 抽象正确；代价是封闭 `Value` 枚举让「加新类型」比「加新阶段」贵一个数量级，见扩展性 |
| A3 | 结构化诊断 + 0/1/2 | 认可 | `ErrorCode`/`diagnostic()` 字段只追加（error.rs:96–127），契约干净 |
| A4 | `catch_unwind` 无条件 + JSON 空 hook | 存疑 | 机制对，但有两处脆弱：P0 的栈溢出不可捕获；进程级 hook 与库直调双轨（tests/lib_run.rs:51 用全局 Mutex 串行化，脆弱）。「无条件收敛」的措辞应限定为「可捕获 panic」 |
| A5 | 限制触顶即停 + `abandon()` | 认可 | 语义正确（output.rs:77–80）；仅 note：slurp.finish 的 limit 出口（lib.rs:244–251）与主循环出口（lib.rs:225–230）结构不一，但都未写出残缺数组 |
| A6 | 转码前按原始字节限流 | 认可 | 防 OOM + 防二次误拒（source.rs:83–109），取舍合理 |
| A7 | `--list` 与运行共用 `validate()` | 认可 | 一致性正确；note：path spec 在 validate 与 discover 各编译一次（P2-1） |
| A8 | MSRV 1.88 + 双 lock `--locked` | 认可 | 工程可复现性，非架构风险 |
| A9 | 文档分散 + CI 门禁防漂移 | 存疑 | 目标正确，但硬编码常量对账（P2-6）使门禁自身成为变更摩擦；建议降为「存在性/下限」断言 |
| B1 | 只保留显式 `re:`/`glob:` | 认可 | 消除隐式推导，错误可定位（CHANGELOG:22） |
| B2 | 每段独立 Regex + frontier 剪枝 | 认可（带保留） | 剪枝有效；node 数无上限（P2-2），但能力边界文档清楚 |
| B3 | `re:` 括号/顶层 `\|` 结构校验 | 存疑 | 意图正确、方向保守（只会误拒）；但手写词法器与 regex 语义分歧（P2-4），且无 fuzz（P2-3） |
| B4 | captures 降级只读字符串元数据 | 认可 | 来源/数据分离干净；0.4.0 opt-in 注入方向合理，但需明确入口（见开放问题 2） |
| B5 | 三类遍历异常统一告警 | 认可 | `warn_io` 单漏斗（pathspec.rs:345–352），零匹配可区别于丢文件 |
| B6 | 目录结尾不追加 `**` | 认可 | 显式优于隐式，避免意外递归 |
| B7 | 夹具/断言迁移 Rust 测试 | 认可 | 单一可执行真相，无解释器依赖 |
| B8 | `--max-depth` 该模式告警 | 认可 | 忽略必须可观测 |
| B9 | 嵌套命名组进 captures + 跨段重名拒绝 | 认可 | 正确；`capture_names()` 枚举 + 编译期去重（pathspec.rs:307–322），BTreeMap 键无歧义 |

---

## 建议的重构或设计变更

1. **修 P0（最高优先级）**：`check_nesting` 不再 `break`；对未终止 raw string 先完成全量深度计数再报 `Query`。这是唯一会击穿 0/1/2 契约、且 `catch_unwind` 救不回的缺陷。
2. **修 P1-1（0.4.0 前置）**：引入输入/输出侧的显式错误类型，统一 `source.rs`/`input.rs` 的读错误 wrap，再实现「读取侧 → 1、写出侧 `BrokenPipe` → 0」。去掉四处复制的 `is_broken_pipe` 判断。
3. **拆 `Cli` god struct**（P1-2）：`InputConfig`/`DiscoverConfig`/`OutputConfig` 三个内聚配置块，纯内部重构。
4. **统一 path spec 编译**（P2-1）：编译一次、结果复用；顺带开启 `pathspec_compile` fuzz（P2-3）补上已迁移的风险面。
5. **为 `-f auto` 明确「静态检查边界」**：文档把「读入前报错」限定为「显式 `-f`」；`--skip-incompatible` 对 auto 的行为保持现状但要在 `--help` 说清（当前只靠运行时错误兜底）。

---

## 开放问题

1. **captures 求值注入（0.4.0）的入口形态**：是新增一个 nullary 阶段（如 `source()`/`capture()` 产出 captures 对象进入记录流），还是在 jq 里隐式注入 `$captures` 变量？前者保 B4「只读元数据、显式 opt-in」的一致性且不动 `Value`，推荐；后者会重蹈 0.3.0 `ts_fields` 派生字段的覆辙（CHANGELOG:23）。
2. **`Value` 枚举的演进路径**：若未来要支持「字节流（binary/raw bytes）」「时间戳」等 Item 类型，A2 的封闭三变体模型是否要升级为 `enum Value { Data, Text, Node, Bytes(..), ... } + TypeSet` 的第 4/5 bit？现在没有，但扩展成本（见 A2 保留）值得提前决定是否引入「自定义类型 + trait 分派」的逃生舱。
3. **`Origin` 的聚合语义**：slurp/`-O json` 这类「多 Item→单 Item」聚合，provenance 是否需要一个「列表化」形（P1-3），尤其是要做 captures 求值后，聚合数组的 `source.captures` 指向谁？
4. **进程级 panic hook 的进程共享**：JSON 空 hook（main.rs:25–29）与库直调 `run()` 的「调用方自行管理」双轨，在嵌入场景（库被其他进程复用）下可能擦掉宿主 hook。是否值得把 hook 安装移进 `run()` 内部并在返回后恢复（借用 tests/lib_run.rs 的 QuietHook 模式产品化）？