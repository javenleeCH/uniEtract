# 测试角色评审

> 评审对象：`ux`（UniExtract）Rust CLI，仓库根目录即当前工作目录。
> 方法：逐文件阅读全部测试源码与门禁脚本、静态统计 `#[test]` 分布、静态扫描关键 CLI 旗标在两个测试层级（`src` 单测 / `tests` 集成 / `sample` 冒烟）的覆盖情况，并实际运行 `cargo test -- --list` 与两个 Python 门禁验证结论。
> 本机 Rust `1.100.0`（`rust-version = 1.88`，非精确 MSRV 复现）、Python `3.15.2`。`check_test_count.py` / `export_cases.py --check` 在本机可直接运行并复现下列结论。

## 一句话结论

测试体系文档充分、契约测试分层清晰、可复现性意识强，但存在**一个 P0 事实性错误（文档 158 与门禁 162 打架）**、**一批只在库内单测出现却完全没走端到端 CLI 的关键旗标盲区**，以及**一个脆弱的硬锚定门禁**——门禁目前"锚定了一个数字"，而没有"守住那个数字来自哪里"。

---

## 现状覆盖地图（已覆盖 vs 盲区）

> 实测 `cargo test --all-targets --locked -- --list` 汇总行合计 **162** 项：
> `src` 内嵌单测 90（+4）· agent 21 · cli 27 · jq_compat 3 · lib_run 6 · pathspec 7 · pathspec_scenario 8。

### 已覆盖（证据充分）

| 领域 | 来源 | 证据点 |
|---|---|---|
| DSL 结构扫描 | `src/dsl.rs` 15 项 | 任意井号 raw string、井号不匹配、字符串内 `)/|`、多字节、深度 127/128/1000、2 万字符长参数、2000 条确定性乱码不 panic |
| jq 数值契约 | `src/stage/jq.rs` 3 + `tests/jq_compat.rs` 3 | 任意精度整数、NaN/±Inf→null、字符串回落 Text、error() 上报 |
| HTML 容错树形 | `tests/cli.rs` §6.4 4 项 + `sample` 5 条 | tbody/template/noscript/大小写折叠专项 |
| XML 命名空间 URI 匹配 | `tests/agent.rs` + `tests/cli.rs` + `sample` 3 条 | 默认 ns、等价 URI 不同前缀、错 URI 零匹配 |
| 结构化诊断 / 退出码 | `tests/agent.rs` 21 + `src/diagnostic.rs` 3 + `src/error.rs` 4 | code/exit_code/字段只追加/quiet 永不压制错误 |
| `--with-source` 路径正斜杠 | `src/model.rs` 1 + `tests/cli.rs` | Windows `D:\OH\...` → `D:/OH/...` 回归 |
| 输入卫生 | `src/source.rs` 9 + `sample` 多条 | BOM 剥离、UTF-16 免二进制、NUL 二进制、GB18030 回退、坏 XML |
| `-O json` / slurp 收口 | `src/output.rs` 5 + `tests/cli.rs` + `tests/agent.rs` | abandon()、双层嵌套数组、pretty/compact、LF-only |
| 类型上界检查 | `src/engine.rs` 5 + `src/types.rs` 2 | 确定不可能链在读输入前报错、skip-incompatible 只看首阶段 |
| BrokenPipe 变体判定 | `tests/lib_run.rs` 6 项 | 输出侧=0、Runtime 包裹读=1、输入侧裸 IO=0 的负向如实用例 |
| pathspec 编译/剪枝/嵌套捕获 | `src/pathspec.rs` 11 + `tests/pathspec{,_scenario}.rs` 15 | 逃逸捕获组、32 上限、同名拒绝、段级剪枝、12 夹具 |
| panic 收敛 | `tests/lib_run.rs` + `src/lib.rs` | `catch_unwind` 无条件转 runtime，JSON 模式空 hook |

### 盲区矩阵（P0/P1 分级，详见下节）

| 旗标 / 边界 | 覆盖现状 |
|---|---|
| `--force-text` | **0 测试**（`src`/`tests` 均无匹配） |
| `--strict` 触发各阶段 | **0 测试**（代码存在 3 处分支，无一测试） |
| `--no-default-ignore` | **0 测试** |
| `--max-input-size` 极限/否决后格式 | 少量（json=3 字节、utf-16 转码=4），无 YAML/TOML/XML/HTML/CSV 整文档格式 |
| `--max-record-size` 极限 | jsonl=1 条 + `long-line.txt` 1 条 + `split` 单测，无 CSV 记录级、无 utf-16 转码后字节语义 |
| `--encoding gb18030`/`gbk` 显式 | `gb18030` 仅出现在 `sample/generate.py` 数据生成，**无任何行为断言**；`gbk` 显式与 `gb18030` 显式 0 测试 |
| `-O json` + `slurp()` 双层嵌套 + 多文件 | 仅 1 条单文件 `stdin` 用例（`tests/cli.rs` L143），无跨文件、无 `map` 型断言 |
| `--skip-incompatible` 各阶段组合 | 仅 xpath/attr 1 条；无 re/text/jq/split/slurp 首阶段组合 |
| `--fuzzy` × `--exclude` 交互 | 0 组合用例 |
| 非 UTF-8 文件名、Windows 大小写/分隔符在 `--with-source`/`--list` | 仅 1 条 `model.rs` 路径正斜杠；无非 UTF-8 文件名、无大小写 |
| 超大文件 | **0** 集成测试（默认 256MiB 上限无任何接近临界用例） |
| 空目录/符号链接/权限拒绝 | 仅 `pathspec` 3 类告警单测走 fake reporter，无真实 `read_dir` EACCES |
| MSYS2 `//a` UNC 改写 | 文档注明、无测试（合理，属 shell 行为） |
| `--line-buffered` 管道时序 | 1 条 `tests/cli.rs` L365，仅覆盖 posix，无 Windows 语义 |
| 5 种 shell 补全 | 仅 bash/powershell 各 1 条，zsh/fish/elvish **0 断言** |
| `--ns` 重复前缀告警 | 0 测试（`src/lib.rs` L128-135 逻辑） |
| CSV/TSV 空 header/重复 header | 0 测试（`src/input.rs` L205-217 逻辑） |

---

## 发现

### P0-1 · 测试总数文档与门禁自相矛盾：158 vs 162（事实性错误）

- **问题**：仓库同时存在三个互相矛盾的数字：
  - `docs/cli-design.md` L136：「Rust 测试共 **158 项**」，下一行表格把 `src/*` 内嵌单测写为 **86**；
  - `CHANGELOG.md` L47：同样「**158 项**（86 单测 + 21 + 27 + 3 + 6 + 7 + 8）」；
  - `.github/scripts/check_test_count.py` L13：`EXPECTED = 162`。
- **证据**：实际运行 `cargo test --all-targets --locked -- --list`，8 个测试二进制的汇总行求和 = **90 + 21 + 27 + 3 + 6 + 7 + 8 = 162**；其中 `src` 单测二进制为 **90**，不是文档写的 86。
- **风险**：CI 的 `verify` job（`.github/workflows/ci.yml` L47-48）跑 `check_test_count.py`，它锚定 162。因此 CI **当前是绿的**（162==162），但两份权威文档都写着 158——这意味着"文档对 CI 的锚定"已经失效，任何人按文档维护都会误导；一旦某处把 `expect` 改成 158 就会立刻红线。三个"真相源"打架，违背项目 A9（文档交叉引用可机器校验）目标。
- **建议**：立即把 `docs/cli-design.md` 第 7 节与 `CHANGELOG.md` 的 86→90、158→162 同步，并在 `check_test_count.py` 里把每个二进制的来源都打印出来（如 `90 src + 21 agent + ... = 162`），使差异定位不再靠猜。

### P0-2 · `--force-text`、`--strict`、`--no-default-ignore` 三个公开旗标完全无测试

- **问题**：这三个是 README「输入卫生」与「已知限制」点名的契约旗标，但静态扫描 `src/**` 与 `tests/**` 的 `#[test]` 断言正文，**无一条**触及它们的行为。
- **证据**：
  - `src/source.rs` L60-74：`--force-text` 是二进制检测的唯一旁路，`true` 会进入文本处理；无测试喂 `b"a\0b"` + `--force-text` 验证「强制按文本读」分支。
  - `src/lib.rs` L170/L190/L233：`--strict` 有三个 `if cli.strict { break }` 分支（开文件失败、`InputSource::open` 失败、求值失败），**三处分支全部无测试**。
  - `src/discover.rs` L214：`--no-default-ignore` 决定是否剪枝 `target/` 等目录；`discover` 单测只测了默认剪枝（正值），未测 `no_default_ignore=true` 会把被忽略目录重新纳入。
- **风险**：一个用户/Agent 用 `--force-text` 强读二进制、或 `--strict` 期望"首错即停"，实际行为无回归保护，**这三条契约随时可能静默回退**而 CI 全绿。
- **建议**：见「建议立即补充」T2/T3/T7。

### P0-3 · GB18030/GBK 显式编码路径零断言，仅有一条靠"数据生成"无意通过的 auto 回退

- **问题**：README 承诺 `--encoding gbk|gb18030` 两种编码；但 `gb18030` 字符串只出现在 `sample/generate.py` L156（生成 `edge/gbk.txt`），`sample/smoke.py` 唯一的编码用例 `edge/gbk-auto-fallback` 用的是 `--encoding auto`，靠 `resolve_encoding` 的 auto 回退到 GB18030。**显式 `--encoding gbk` 与显式 `--encoding gb18030` 的 `decode()` 分支（`src/source.rs` L211-226）从未被断言**；`Encoding::Gbk` 与 `Encoding::Gb18030` 二者 decode 行为差异（GBK 是 GB18030 子集、4 字节区仅 GB18030 有）完全无覆盖。
- **证据**：`grep 'gb18030\|gbk' tests/` 与 `src/**` 的 `#[test]` 正文命中数为 **0**（`source.rs` 测试里 `auto_falls_back_to_gb18030` 也只走 auto，未显式指定）。
- **风险**：GB18030 4 字节码点（如扩展区汉字）若被 GBK 解码器报错、或 GBK 某些字节无 GB18030 映射，用户无法分辨，且无回归保护。
- **建议**：见 T8。

### P1-1 · `--max-input-size` / `--max-record-size` 极限覆盖率极低，且无尺寸语义矩阵

- **问题**：两个限制各自只有 1-3 条近临界用例，且没有覆盖"限制是否在**转码前按原始字节**算"的边界就位（A6 是设计要点）。
- **证据**：
  - `tests/agent.rs` L397-423（utf-16 转码 + `--max-input-size 4` 拒绝）、`tests/cli.rs` L262-274（`--max-input-size 3` json、`--max-record-size 3` jsonl）、`sample` `edge/max-record-size`（1KiB 拦 4096 字节行）。
  - 未覆盖：`--max-input-size` 用于 YAML/TOML/XML/HTML/CSV/TSV 整文档格式（`read_utf8_limited` 对这些格式的 `max_input_size` 分支）；`--max-record-size` 用于 CSV 记录级（`src/input.rs` L227）与 split 片段级（`src/stage/mod.rs` L40）；`--max-record-size` 恰好等于行长（边界 `>` vs `>=`）——`next_line` 用 `limit.saturating_add(1)` 读、再 `> limit` 判，**恰好等于 limit 的字节是否放行无测试**。
  - utf-16 转码后 `--max-record-size` 的语义（`source.rs` L138 转码路径把 `max_input_size` 置 0、但 `max_record_size` 原样传下，解码后字节数膨胀可能绕过单条记录上限）完全无测试。
- **风险**：尖角边界（=`N` 字节、`N+1` 字节）的放行/拒绝是本地工具最容易被 off-by-one 击穿的位置。
- **建议**：见 T4/T5。

### P1-2 · `-O json` + `slurp()` 双层嵌套 / 跨文件聚合只有一个单文件正向用例

- **问题**：README「容易误判的点」明确强调 `-O json` 叠加 `slurp()` 时数组元素本身也是数组（双层）。现有测试只有 `tests/cli.rs` L143-149 一条：单文件 stdin、`slurp() | jq(map(.n))`，且断言仅 `stdout.contains("[\n")`——连"元素是数组"这一关键形状都没断言，跨文件、`--with-source` 只保留第一条来源（README「已知限制」L228）更无验证。
- **证据**：`tests/cli.rs` L142-149；`tests/agent.rs` 无 `-O json`+`slurp` 组合。
- **风险**：双层嵌套是"看起来是统计、实际是嵌套"的高价值陷阱，断言过弱等于没测。
- **建议**：见 T6。

### P1-3 · 断言过度依赖文案子串，且没有 golden 文件做"字节级稳定"的机器验证

- **问题**：
  - 大量断言是 `stderr_contains="..."` / `stdout(predicate::str::contains(...))` 形式的**英文字面子串**（如 `tests/cli.rs` L36 检查 9 条 `--help` 文案、`tests/agent.rs` L54/L111 检查 `"boom"`/`"mutually exclusive"`）。这些断言在帮助文案调整时是合理的（契约就是文案），但错误路径大量用 `contains("invalid XML")`、`contains("not bound")`，无法锁定字段结构，且并不脆弱。
  - 真正缺的是：commit 里没有 golden/snapshot 文件来机验证"多次调用**字节级稳定**"这条契约。`tests/agent.rs` L502-518 的 `list_is_byte_stable...` 只在**同一进程内**比对 3 次运行结果相等，没有跨进程、跨排序键（目录含同名不同大小写文件时）的确定性断言；`-O json` pretty 输出、`--with-source` 的完整字节形状都用零散 `contains`，没有整体 golden 对比。
- **风险**：字节级稳定是 A2/A4 级承诺，但没有一个"已知正确答案"的物证；一个静默改变键排序、浮点格式化或缩进的依赖升级无法被捕获（jq `keys` 排序已被 `tests/jq_compat.rs` 覆盖，但 `-O json` 的 serde 键排序未 golden）。
- **建议**：引入 `tests/snapshots/`（`insta` 或手写 `assert_eq!` 全量字节），至少覆盖 `-O json` pretty、`--list`、`--with-source` 三张输出。

### P1-4 · fuzz 只有 1 个 target（dsl_split），且全程不触碰其余 6 个编译面

- **问题**：`fuzz/fuzz_targets/dsl_split.rs` 只 `parse_pipeline` 一个入口；pathspec 编译器、类型系统、编码转码、jq/xpath/regex 阶段、发现器都无 fuzz 覆盖。
- **证据**：`fuzz/Cargo.toml` 仅声明 `[[bin]] dsl_split`；`check_fuzz_manifest.py` 只做 `cargo metadata --locked`（根本没执行任何 fuzz，仅是"清单能解析"）。
- **风险**：`src/dlsl.rs` L330 的 2000 条确定性乱码只覆盖 DSL；`validate_re_fragment`（`src/pathspec.rs` L207-255）的手写括号/字符类扫描是最容易被"看似合法、实则逃逸捕获组"的恶意 spec 击穿的面，0 fuzz。
- **建议**：增加 `pathspec_compile`、`encoding_detect`（随机字节 → decode → 不 panic）、`pipeline_compile`（随机管道 → `Pipeline::compile` 不 panic）三个 target；对 pathspec 用 `proptest` 或 `arbitrary` 生成 `{name:re:...}` 片段与嵌套命名组。

### P1-5 · panic hook 的全局 Mutex 串行化：正确但脆弱，且只有 6 个 lib 测试共享它

- **问题**：`tests/lib_run.rs` L51 的 `static HOOK_LOCK: Mutex<()>` 用全局锁串行化 panic hook 安装（正确），但这是**本仓库手工实现的进程级不变量**，脆弱点在于：任何未来新增的、直接调 `uniextract::run()` 或安装 panic hook 的测试若**忘记先 `hook_guard()`**，就会与并行测试竞态，且错误表现为随机 `abort`/不稳定的 hook 泄漏。
- **证据**：`tests/lib_run.rs` L53-57 `hook_guard` 取锁、L49-51 注释自述"serialize every test that installs one"。
- **风险**：这是隐式约束而非机械强制（不像编译期 trait 那样不可违反）；竞态隐患是"真实存在但难以稳定复现"。
- **建议**：把安装 hook 的入口收敛为一个 `with_panic_hook(closure)` 辅助函数（内部持锁 + 恢复），让调用方无法绕过；或在测试根上 `#[serial]`（`serial_test`）。至少补一条负向注释：**新增涉及 panic/hook 的测试必须经过 `hook_guard()`**。

### P2-1 · `check_test_count.py` 的汇总行正则依赖 libtest 私有格式，且 fallback 有缺陷

- **问题**：`SUMMARY = re.compile(r"(\d+)\s+tests?,\s+\d+\s+benchmarks?")` 依赖 libtest 每一行 `N tests, 0 benchmarks` 的稳定格式；`EXPECTED = 162` 是手写常量。二者问题：
  - 任意一处**新增一条 `#[test]` 就红线**（CI verify 步骤 `check_test_count.py` 必须手改 `EXPECTED` 并同步两份文档）。这是"数字锚定"而非"不变量锚定"：没有对"每个测试二进制名称 → 数量"的结构化断言（例如 uid 匹配 `90 src + 21 agent`）。
  - fallback `line.rstrip().endswith(": test")`（L36）在 `-- --list` 的真实输出里**不会命中**——列表行形如 `name::test: test`，结尾是 `: test`，但注释声称"future libtest drops summaries"，而 libtest 2.x 的 `--list` 在一行内同时打印名称与 `: test`，该 fallback 路径实际上不可达且错误。
- **证据**：`.github/scripts/check_test_count.py` L13/L16/L33-37。
- **风险**：新增测试是常态（尤其本评审建议补测），门禁把"加测试"变成"必须同步改 3 处魔数"的负担，反而抑制补测。
- **建议**：改为**断言文档数字 == 实际数字、且实际数字按二进制分解成结构化映射**（读 Cargo 的二进制列表，对每个二进制断言数量），把"补测就要改门禁"变成"补测后跑一次 `--fixup` 自动刷新文档"。

### P2-2 · `--skip-incompatible` 只覆盖 xpath 首阶段，其余首阶段无组合

- **问题**：`skip_incompatible` 的语义是"只看首阶段"（README L230 / `src/engine.rs` L74-80）。现有端到端只有 1 条 xpath+attr（`tests/cli.rs` L330-347），未覆盖首阶段为 `re`/`text`/`jq`/`split`/`slurp` 时对 10 种输入格式矩阵的判断。
- **证据**：`grep skip-incompatible` 命中 `tests/cli.rs` L335、`sample/smoke.py` L203 各 1 条，均以 `--skip-incompatible -O text -e "xpath(...)"`。
- **风险**：`accepts_source` 用 `source_types(format)` 的 DATA/TEXT/NODE 三分法判断；`slurp()` 作为首阶段时 `input_types()` 返回 `ALL`，`accepts_source` 恒 true，但此组合无测试，易被后续 `slurp` 改变收口逻辑时悄悄改变。
- **建议**：见 T9。

### P2-3 · `--fuzzy` 与 `--exclude` 交互、以及 `--list` 的 exclude 注释被忽略

- **问题**：`README` 明确 `--fuzzy` 是"后置过滤"、`--exclude` 是"追加排除 glob"。二者叠加时的顺序与语义（exclude 先于 fuzzy？fuzzy 后置过滤命中率是否 `match_paths` 对捕获件路径的作用）无组合测试；`--list` 与 `--exclude` 同用（列出清单应反映排除后的集合，A7）也无端到端。
- **证据**：`grep '--exclude'` 命中 `tests/agent.rs` L158（非法 glob `[` 报 usage）、`tests/cli.rs` L193（同样非法 glob）、`src/discover.rs` L313-329（单测 `respects_max_depth_and_exclude` 只测 `--max-depth`+`--exclude`，未叠 `--fuzzy`）。
- **建议**：见 T10。

### P2-4 · 5 种 shell 补全只测得 2 种，zsh/fish/elvish 无断言

- **问题**：README 与 `--completions` 均宣称支持 5 种 shell，但 `tests/agent.rs` L230-249 只断言 bash（含 `_ux`/`complete`）与 powershell（含 `Register-ArgumentCompleter`）；zsh/fish/elvish 的脚本生成路径零测试。
- **风险**：补全脚本是纯生成代码，未来 `clap_complete` 升级把某个 `Shell` 枚举改名/去重后，非主流的 shell 会静默坏掉。
- **建议**：补 3 条最小断言（脚本非空 + 含 shell 特征串）。

### P2-5 · `lib_run.rs` 的 BrokenPipe 输入侧负向用例依赖 `Cursor`/mock 而非真实管道

- **问题**：`tests/lib_run.rs` 用自定义 `BrokenInput` / `ImmediateBrokenInput` mock 精确再现输入侧首次 `fill_buf`、逐行 `read_until` 的裸 BrokenPipe，这是高质量的如实负向。但它不是真实 OS 管道：没有覆盖 `main.rs` 经 `std::io::stdin().lock()` 注入真实 stdin 的路径（真实管道关闭是 `EPIPE`/`BrokenPipe`，mock 是人工构造）。
- **风险**：4.0.0 目标"输入侧管道破裂 → `runtime(1)`"（README L249）的验收点是**真实 stdin 管道**；当前 mock 无法验证 `main → run → InputSource::open → fill_buf` 的真实链路。
- **建议**：增加一条集成测试（`tests/cli.rs` 已有 `line_buffered_emits_before_stdin_is_closed` 的真实子进程范式，可复用）：父进程 `spawn` ux、写部分数据后 `drop(stdin)`，断言退出码与 stderr；并在 0.4.0 落地这条时同步这条集成测试为正向断言。

---

## 建议立即补充的测试清单（具体、可落地）

> 全部是"小而独立、能并进现有分层"的测试，标注建议文件与断言思路。

1. **T1 — 测试总数真相源对齐（P0-1）**：改 `docs/cli-design.md` L136/L140 与 `CHANGELOG.md` L47 的 86→90、158→162；`check_test_count.py` 打印二进制分解。零新增测试，纯事实修正。

2. **T2 — `--force-text` 二进制旁路（P0-2）**：`tests/cli.rs`
   `ux -f text --force-text -O text -e 'text()'` 喂 `b"a\0b"`，断言 `rc=0`、stdout 是文本（含 NUL 的原始字符串，按 text 处理），且**无** `binary input skipped`；对照不加 `--force-text` 时 `rc=0`+该 warning（现有 `binary-stdin-skip`）。再补显式路径文件 + `--force-text` 时**不再**报 `binary input rejected`（`src/source.rs` L67 的 explicit 分支）。

3. **T3 — `--strict` 首错即停（P0-2）**：`tests/cli.rs`
   目录两个文件 `good.json`（合法）+ `bad.json`（`{oops`），`-f json --strict -O text -e 'jq(.)'`。断言：`rc=1`、stdout 只含 good 的产出、stderr 只报一次 `invalid JSON`（不再处理 good 之后的 bad，且 good 在 bad 之前因字典序）。反面对照：不加 `--strict` 时 bad 格式错误后继续、`rc=1` 且 good 结果保留。

4. **T4 — `--max-input-size` 否决整文档格式矩阵（P1-1）**：`tests/cli.rs`
   对 `-f yaml` / `-f toml` / `-f xml` / `-f html` / `-f csv` 各用 `--max-input-size` 略小于输入字节，断言 `rc=1`、stderr 含 `--max-input-size`、stdout 空；再对 `--max-input-size` 恰好等于字节数断言放行（边界 `==`）。

5. **T5 — `--max-record-size` 边界与 CSV 记录级（P1-1）**：`tests/cli.rs` + `src/input.rs`
   `next_line` 的 3 字节限额 + `abcdef\n` 已测 `>`；补 `abc\n`（`== limit`）放行。集成：CSV 单记录字段字节 > `--max-record-size` 报错（`src/input.rs` L227 分支），以及 utf-16 转码后 `--max-record-size` 的语义（解码后膨胀的单行是否拦）。

6. **T6 — `-O json`+`slurp()` 双层嵌套 + 跨文件 + `--with-source`（P1-2）**：`tests/cli.rs`
   `-f jsonl -O json -e 'slurp() | jq(.)'` 两个文件，断言输出是 `[[{...},{...}]]` 形状（`serde_json` 解析后校验 `out[0]` 是数组、长度=2）；`--with-source` 叠加时 `slurp` 只保留首条来源（`README` L228），断言 `source.line` 取首个文件首行且后续记录来源不覆盖。

7. **T7 — `--no-default-ignore` 重新纳入被忽略目录（P0-2）**：`tests/cli.rs`
   临时目录 `target/generated.json` + `keep.txt`，`ux -e 'text()' dir`（默认）只输出 `keep.txt`；加 `--no-default-ignore` 后 `target/` 内文件被纳入。断言两次 `--list` 的路径集合差集。

8. **T8 — 显式 `--encoding gbk` / `--encoding gb18030`（P0-3）**：`tests/cli.rs` + `src/source.rs`
   用 `sample/edge/gbk.txt`（GBK 字节）跑 `--encoding gbk` 与 `--encoding gb18030`，断言输出 `中文 GBK 样本...` 逐字正确、`rc=0`、无 `assumed GB18030` 告警（显式指定不告警）。补一个 GB18030 4 字节码点夹具（如 U+20000 区汉字），断言 `gb18030` 解出、`gbk` 报 `cannot be decoded`（`src/source.rs` L219-224）。

9. **T9 — `--skip-incompatible` 各首阶段组合（P2-2）**：`tests/cli.rs`
   对 first stage ∈ {`re`,`text`,`jq`,`split`,`slurp`}，混合一个不兼容格式文件 + 一个兼容文件，断言只产出兼容文件结果且 `stderr` 含 `incompatible`；尤其 `slurp()` 首阶段（`input_types=ALL`）应跳过**任何**→实际是跳过空文件，重点验证 `accepts_source` 恒 true 不导致对坏文件仍然报错。更明确：构造 `--skip-incompatible -e 'slurp() | jq(length)'` 混合 `.jsonl` 与 `.html`，断言 HTML 被跳（因为 `accepts_source` 对 `Auto` 返回 true、但 `-f` 明确指定时首阶段 slurp 仍应处理——需按当前语义写死预期）。

10. **T10 — `--fuzzy`×`--exclude` 交互 + `--list` 一致性（P2-3）**：`tests/cli.rs`
    目录 3 文件，`--exclude 'exclude*.txt' --fuzzy 'hit'`，断言 `--list` 与实际处理路径集合一致（A7），且被 exclude 的即使 fuzzy 命中也不出现；验证 `--list` 不含 stderr 泄露。

11. **T11 — shell 补全 zsh/fish/elvish（P2-4）**：`tests/agent.rs`
    3 条 `--completions zsh|fish|elvish` 最小断言（`rc=0`、stdout 非空、含各自特征字 `#compdef` / `complete` / `_arguments`）。

12. **T12 — 真实 stdin 管道 BrokenPipe（P2-5）**：`tests/cli.rs`
    复用 `line_buffered_emits_before_stdin_is_closed` 父进程范式：spawn ux 读 stdin，写半截后 `drop(child.stdin)`，断言当前 0.3.1 `rc=0`、stderr 空（如实负向）；4.0.0 落地后把断言改为 `rc=1` + `runtime` 诊断（README L249 验收点）。

---

## 门禁体系评价与建议

**总体**：设计意识在同类 CLI 中偏强——三层测试（单测/集成/冒烟）+ 三个 Python 门禁 + MSRV 固定 + 双 OS + `--locked` + fuzz 清单校验，方向上对了。但门禁"可信度"参差。

### 可信的（保持）

- `check_contract_docs.py`：必需文件断言 + 过期文案黑名单 + 12 夹具计数，**实用且有边**。黑白名单针对本项目历史坑（`time:`/`lit:`/BrokenPipe 方向措辞），是真正的防漂移门禁。
- `sample/export_cases.py --check`：`CASES.md` 由 `smoke.py` 的 `CASES` 单一真相源**同源生成**（`importlib` 直接加载 `smoke.py`），`--check` 全文比对，**不会漂移**。这是本项目门禁里最干净的同步机制。
- `smoke.py` 不经 shell 直调二进制（`subprocess.run([str(ux)] + ...)`），规避 MSYS2 `//a` 转换，且 `--only` 支持分片；45 条断言精确到字节（`stdout` 精确相等）。
- `cargo test --all-targets --locked` 与 `cargo build --release --locked` 双 OS：真实执行，可信。

### 不可信/需重构的（按优先级）

1. **`check_test_count.py` 应立即改为"结构化分解 + 可自动刷新"**（P2-1）。当前它锚定一个孤立的、需要手工同步的魔数 162，且与两份文档矛盾。理想形态：脚本统计出 `{binary: count}`，把该结构写进一个 `tests/COUNTS.json`（或注释块），并断言"文档数字 == 求和 == 各二进制分解"；补测后跑 `--fixup` 一次性刷新文档与 JSON，而非手改 3 处。这样"新增测试"从红线变成一次机械刷新，反而鼓励补测。

2. **`check_fuzz_manifest.py` 名不副实**（P1-4）。它只跑 `cargo metadata --locked`，等价于"fuzz 工作区能解析依赖"，**不执行任何 fuzz 运行、不校验 fuzz target 数量或清单内容**。建议：①至少断言 `fuzz/Cargo.toml` 里 `[[bin]]` 数量 == 期望的 target 数；②在 CI 增加一个"短时 fuzz smoke"（`cargo fuzz run dsl_split -- -max_total_time=15`，仅 ubuntu），否则 fuzz 这条防线纯粹是纸面。

3. **缺 golden/snapshot 门禁**（P1-3）。"字节级稳定"承诺应由 golden 文件物证化：在 `tests/snapshots/` 放 `-O json` pretty、`--list`、`--with-source` 三张完整字节输出，用 `assert_eq`（或 `insta`）在**跨进程**维度锁定，而非同一进程内跑 3 次比对。

4. **CLI 参数矩阵测试缺失是最低垂果实**（P0-2/P2-2/P2-3）。`--force-text`/`--strict`/`--no-default-ignore` 零覆盖说明"旗标 → 行为"映射没有被穷举。建议引入一页**参数矩阵清单**（`tests/cli_matrix.md` 或注释表），每个公开旗标至少一行"正向 + 负向 + 组合"，让盲区一眼可见（现在的 README 能力边界很全，但测试没有镜像它）。

5. **脆弱的文案断言集中在 `--help`（可接受）与错误子串（可收紧）**。`tests/cli.rs` L25-37 对 9 条 `--help` 文案用 `contains` 是**有意**的契约回归（可接受）；但错误路径大量 `contains("invalid XML")` 未来改为结构化字段断言（`code`/`exit_code`）会更稳。安全做法是与 JSON 诊断对齐：`--error-format json` 下断言 `code`/`message` 字段而非人类文案。

### 门禁结论

- 三个 Python 门禁里，`check_contract_docs.py` 与 `export_cases.py --check` **可信**；`check_test_count.py` 需要从"锚定魔数"升级为"结构化分解 + 自动刷新"；`check_fuzz_manifest.py` 需要从"清单能解析"升级为"至少短时真实 fuzz"。
- 最大的风险不在门禁本身的实现，而在**文档与门禁对不上**（158 vs 162）这一信号：它说明"测试数量"这个被门禁当作不变的量，已经漂移而无人察觉。这正是本评审第一要修的事。