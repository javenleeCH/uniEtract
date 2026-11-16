# Lead 综合评审与定稿

> 本文件是五个角色（产品 / 架构 / 测试 / 开发 / 用户）评审的会签与定稿。
> 各角色原始报告见 `review/product.md`、`review/architecture.md`、`review/testing.md`、`review/developer.md`、`review/user.md`。
> 本文件记录：跨角色冲突的裁决、证据确凿的结论、本轮已落地的改动，以及留待后续的优先级清单。

## 一句话结论

`ux` 是一个工程化基线高于同类 CLI 平均水平的工具：错误链一致、无 `unsafe`、`--locked` 可复现、发布链路机器化交叉校验、Agent 契约罕见地一等公民投入，核心架构（单记录流 + 统一 `Item` + 结构化诊断 + panic 收敛）稳健。**但**五角评审交叉比对后，最高置信度的问题不是功能缺口，而是三处「该吵的地方不够吵 / 该一致的地方不一致」：

1. **零匹配是静默的**（用户/产品会签）——最高频、最难自救的失败路径被刻成「零匹配 = 干净 stderr」的契约测试。
2. **文档/门禁/代码三处真相源打架**（测试/开发会签）——测试数 158 vs 162，路径归一化 `--list` 与诊断 `path` 不一致。
3. **一个被评估为 P0 的「不可捕获栈溢出」经实证不成立**（架构 P0，Lead 裁决）——但由此暴露出守卫的正确性依赖「意外遮挡」，值得用回归测试锚定。

本文件对以上三点给出裁决与落地记录；其余发现按优先级进入 backlog。

---

## 一、跨角色冲突裁决

### C1 · 架构 P0「DSL 深度守卫绕过 → 栈溢出」= 不成立（Lead 实测裁决）

- **架构角色主张**（`review/architecture.md` P0-1）：`check_nesting` 遇到未终止 raw string 时 `None => break`（`src/dsl.rs:84-85`）提前结束深度计数，其后深层 `(` 不再被 128 层守卫拦截，pest 的 `group` 间接递归会栈溢出 → 进程 abort，`catch_unwind` 与空 panic hook 都救不回。
- **Lead 裁决：不成立**。证据：
  1. 六种构造（`re(r#"`+深度、`jq(r"`+深度、`re(r#`+深度、`re(r#"`+8000、`re("`+深度、`jq('`+深度，深度 600–10000）在**原始代码**上全部干净退出 2，无崩溃、无 abort。
  2. 机制分析：pest 的 `balanced` 有序选择中，`raw_string`/`quoted_string` 会先把 `"` 吞进其 body；当终止符缺失时，pest 在该规则上失败并**回溯**到 `group`/`plain`，但 `plain = !( "(" | ")" | "'" | "\"" ) ~ ANY` 明确拒绝 `"`，`group` 需要 `(` 而当前字符是 `"`，于是解析在 `stage` 的死胡同处报「expected stage」——**从未把 `(` 交给递归的 `group`**。深括号被正确拒绝，栈溢出路径不存在。
- **结论**：架构角色结论意图正确（守卫不应依赖「意外遮挡」），但「可被构造 `-e` 击穿 0/1/2 契约」的描述不成立。**落地**：新增回归测试 `unterminated_raw_string_then_deep_parens_is_a_query_error_not_a_abort`（`src/dsl.rs`），把「未终止 raw string + 2000 层 `(` → `Query(2)` 而非 abort」这一安全行为用测试锚定，防止未来重构 pest 语法或守卫时回归为真崩溃。
- **对架构角色的反馈**：评审应区分「静态缺陷假设」与「已实证的崩溃」。该 P0 没有做最小复现实验就下了「进程 abort」结论；建议同类结论先跑一次真实二进制验证（本机 `cargo test` 已可承载）。

### C2 · 输入侧 BrokenPipe = 真实问题，但「提前到 0.3.x hotfix」需保守

- 产品（P0-1）、架构（P1-1）、用户（雷区 #3）三方一致：`AppError::Io(BrokenPipe)` 一律返回 0，输入端管道破裂会与「真零匹配」不可区分。
- **Lead 裁决：问题是真实的、已文档化的已知限制**，但三方对「何时修」有分歧（产品主张 0.3.x hotfix，架构主张 0.4.0 连带统一错误表示）。
- **结论**：0.4.0 已定稿「输入侧改 `runtime(1)`」；该方向正确。**不建议本轮在 0.3.x 擅动退出码语义**——因为 `tests/lib_run.rs` 已把「输入侧裸 Io BrokenPipe=0」作为**负向用例固化**，0.3.1 的 promise 是「按错误变体」而非「按方向」；单方面改会同时破坏 README/`--help`/契约文本/负向测试四处，属于一个需要版本号与迁移表配合的正式变更。**本轮动作**：维持现状，严格遵守 README 契约；留待 0.4.0 按架构方案的「先统一 Read/Write 错误表示，再按方向区分」落地。
- **给架构角色的落地要求**：P1-1 建议正确 —— 目录方向区分前必须先给 `AppError` 引入显式 `Read`/`Write` 侧（当前 `Runtime(String)` 包裹 vs 裸 `Io` 的偶然化是根因），并消去 `lib.rs` 四处复制的 `is_broken_pipe` 判断。

### C3 · 「零匹配 = 干净 stderr」= 最高置信度的产品缺陷（用户/产品会签，采纳）

- 用户角色（雷区 #1）与产品角色（P0 引言）都指向 `tests/agent.rs:426` 的 `zero_matches_is_success_with_clean_stderr`：它把「零匹配时 stderr 完全干净」写进契约，而零匹配恰是 XPath 隐式 `<tbody>`/大小写折叠/错命名空间/MSYS2 改 `//a` 等所有静默陷阱的共同出口。
- **Lead 裁决：采纳用户角色的「结果统计汇总」方案，但不改变「零匹配 = 退出 0」**（退出码语义不变，保持 Agent 契约 0/1/2 稳定）。
- **结论**：本轮**未实施**该改动，因为它是**行为新增**（默认向 stderr 打一行 `processed N files · R records · M results`），需要：①改 `tests/agent.rs:426` 与可能的 other 断言；②在 README 契约表新增该行并说明 `--quiet` 可压；③确认 `--quiet` 语义收紧（见下 C4）。这是一处需要版本/文档/测试三同步的正式变更，列入下一轮 backlog 首位，而非本轮把手伸进「干净 stderr」这一被 21 项 agent 契约测试保护的承诺面。

### C4 · `--quiet` 语义过重 = 采纳，列为与 C3 同批的变更

- 用户角色（雷区 #4/#5）指出 `--quiet` 会吞掉 `no input files matched`/`binary input skipped`/`incompatible`/`empty file` 等「解释空输出」的关键 warning，且 README:142 与 `references/recipes.md:109,113` 的「推荐 Agent 姿势」自带 `--quiet`，会诱导 Agent 把「文件被跳过」当「没数据」。
- **Lead 裁决：采纳，但拆分 risk 等级**。「收紧 `--quiet` 语义」是一处 reduce-surprise 的必要变更，但它与 C3（结果统计）纠缠，且会改变 `Reporter::warn` 的压制边界（目前 21 项契约测试依赖 quiet 压制 warning）。**结论**：C3/C4 合为「可观测零匹配」一揽子 backlog（见 §四 B-1），需统一设计 warning 严重度分级（info/result-affecting）后再落地；本轮不动。

---

## 二、本轮已落地（证据确凿，全部通过门禁）

以下取舍原则：**只做可验证、不触碰稳定契约承诺面、CI 全绿**的修复。

### 1. 测试总数真相源对齐（采纳测试 P0-1 / 开发 P2-3）

- 事实：`check_test_count.py` 锚定 162，但 `docs/cli-design.md` 与 `CHANGELOG.md` 写 158（单测 86）。实测 162。
- 修复：三处同步为 164→167（随新增测试滚动），最终 **167**。
- 触及：`docs/cli-design.md`、`CHANGELOG.md`、`.github/scripts/check_test_count.py`。

### 2. 诊断 `path` 归一化不一致（采纳开发 P2-4 / 测试「Windows 对账」盲区）

- 事实：`--list`（lib.rs）与 `source.path`（model.rs）输出正斜杠 `D:/x`，但 `--error-format json` 的 eval 诊断 `path` 字段输出反斜杠 `D:\x`，三份输出无法直接对账。
- 修复：`src/error.rs` 的 `diagnostic()` 对 `path` 做 `.replace('\\', "/")`，与另两处对齐。新增单测 `eval_diagnostic_path_uses_forward_slashes_like_list_and_source`。
- 触及：`src/error.rs`。

### 3. 补三个公开旗标的端到端回归（采纳测试 P0-2）

`--force-text`、`--strict`（3 处分支）、`--no-default-ignore` 此前**零**端到端测试，契约随时可静默回退。新增 3 条 CLI 测试：
- `force_text_bypasses_binary_detection`（stdin NUL + 显式二进制文件）
- `strict_stops_at_the_first_file_error`（对照 non-strict 继续处理）
- `no_default_ignore_reincludes_ignored_directories`（target/ 剪枝 vs 纳入）
- 触及：`tests/cli.rs`（27→30 项）。

### 4. 架构 P0 回归锚定（裁决 C1 的落地）

- 新增单测 `unterminated_raw_string_then_deep_parens_is_a_query_error_not_a_abort`（`src/dsl.rs`），锁定「未终止 raw string + 深层括号 → 干净 `Query(2)`，不 abort」。

### 门禁验证结果

| 门禁 | 结果 |
|---|---|
| `cargo test --all-targets --locked` | 167 项全通过（92 单测 + 21 agent + 30 cli + 3 jq + 6 lib + 7 pathspec + 8 scenario） |
| `cargo clippy --all-targets --locked -- -D warnings` | 0 error |
| `cargo fmt --check` | 通过 |
| `python .github/scripts/check_test_count.py` | OK（167） |
| `python .github/scripts/check_contract_docs.py` | OK（12 pathmatch fixtures） |

---

## 三、各角色观点收敛与归属

| 主题 | 提出者 | 状态 | 归属 |
|---|---|---|---|
| 测试总数 158 vs 162 | 测试 P0-1 | ✅ 已修 | 本轮 |
| `--force-text`/`--strict`/`--no-default-ignore` 零测试 | 测试 P0-2 | ✅ 已补 | 本轮 |
| GB18030/GBK 显式编码零断言 | 测试 P0-3 | ⏸ backlog B-2 | 下轮 |
| 诊断 `path` 未归一化 | 开发 P2-4 | ✅ 已修 | 本轮 |
| DSL 栈溢出 P0 | 架构 P0-1 | ❌ 不成立（C1）+ 回归锚定 | 本轮 |
| 输入侧 BrokenPipe 判 0 | 产品 P0-1 / 架构 P1-1 / 用户 #3 | ⏸ 0.4.0（方向正确，不做 hotfix） | 0.4.0 |
| 零匹配静默 + 结果统计 | 用户 #1 / 产品 P0 引言 | ⏸ backlog B-1 | 下轮 |
| `--quiet` 语义过重 | 用户 #4/#5 | ⏸ backlog B-1（与结果统计同批） | 下轮 |
| schema 校验被误归「关系运算」 | 产品 P1-1 | ⏸ backlog B-3（重新评估，不改边界） | 评估中 |
| 无排序 / `--sort-by` | 产品 P1-2 | ⏸ backlog B-4（依赖 captures 求值） | 0.4.0+ |
| 命名 `-f`/`-O`/`text` 三义 | 产品 P1-3 / 用户 | ⏸ 文档 + `--help` 标注 | 下轮 |
| 输入选择四套并存 + 缺决策表 | 产品 P1-4 | ⏸ 0.4.0 附决策表 | 0.4.0 |
| skill zip 无一致性门禁 | 开发 P1-1 | ⏸ backlog B-5 | 下轮 |
| 主线含 0.4.0 行为但 version 仍 0.3.1 | 开发 P1-2 | ⏸ 版本策略 | 评估中 |
| `rust-toolchain.toml` 缺失 | 开发 P2-5 | ⏸ backlog B-6 | 下轮 |
| `check_test_count.py` 硬锚定摩擦 | 测试 P2-1 / 开发 | ⏸ 结构化分解 + `--fixup` | 下轮 |
| fuzz 仅 1 target、路径规范化未进 fuzz | 测试 P1-4 / 架构 P2-3 | ⏸ backlog B-7 | 下轮 |
| `validate_re_fragment` 手写状态机 | 架构 P2-4 | ⏸ 文档明确覆盖子集或改 runtime 判定 | 评估中 |
| `--list` 缺 format 字段 | 产品 P2-3 | ⏸ backlog B-8（追加只增字段） | 下轮 |

---

## 四、剩余 backlog（按优先级，供后续轮次选择）

- **B-1（最高）**：「可观测零匹配」一揽子 —— 文件输入时默认向 stderr 打 `processed N files · R records · M results`（`--quiet` 可压），同时收紧 `--quiet` 对「改变结果集」警告的压制，移除 README/recipes 推荐姿势里的裸 `--quiet`。触及：`lib.rs` 统计、`Reporter` 严重度分级、`tests/agent.rs:426` 契约更新、README/SKILL。
- **B-2**：显式 `--encoding gbk`/`gb18030` 端到端断言（含 GB18030 4 字节码点、`gbk` 缺失映射的报错分支）。
- **B-3**：把「单条记录 schema 校验」从「不做关系运算」清单拆出，评估轻量 `--schema`/终阶段断言（不触碰跨文件 join）。
- **B-4**：`--sort-by <capture>`（依赖 0.4.0 captures 求值注入）。
- **B-5**：`dist/ux-extract.zip` 与 `.agents/skills/` 的一致性门禁（解包逐文件 sha256 比对或幂等重生成）。
- **B-6**：`rust-toolchain.toml`（channel 1.88.0 + rustfmt/clippy）或 `just` 封装本地门禁。
- **B-7**：`pathspec_compile` / `encoding_detect` / `pipeline_compile` fuzz targets；`check_fuzz_manifest.py` 升级为真实短时 fuzz smoke。
- **B-8**：`--list` 清单追加 `"format"` 字段（只追加不重命名）。

---

## 五、对评审流程本身的复盘（面向「产品-架构-测试-开发-用户」的多 agent 评审目标）

1. **结论质量**：五个角色都产出了可定位、分级的证据型结论（文件:行号），没有空话。产品角色对「差异化成立/不成立场景」的二元区分、用户角色对「静默失败雷区」的排雷表、测试角色对「盲区矩阵」、开发角色对「发布链路单一真源」的剖析、架构角色对「A/B 决策逐条看板」都是高质量。
2. **一个系统性教训**：架构角色的 P0 未做最小复现就下「进程 abort」结论，被 Lead 实测推翻。**多 agent 评审必须把「可执行的假设」与「已实证的事实」分开**；凡宣称「崩溃/击穿契约/数据丢失」的，应先跑一次真实二进制复现。建议后续轮次给每个角色明确「复现要求」。
3. **收敛效率**：五个角色有大量共识交集（BrokenPipe、零匹配、路径归一、文档锚定），说明项目真实的、值得修的债是收敛的、可枚举的，而非发散的新功能。这本身是「打磨」而非「扩张」的信号。
4. **落地原则（本轮实证有效）**：只做「可验证 + 不碰稳定契约 + CI 全绿」的修复，把「行为新增/退出码改变/契约面改变」的单独立项为 backlog。这使本轮在新增 5 个回归测试、修复 2 个真实不一致的同时，保持了 0/1/2 契约与 21 项 agent 契约测试的零回归。