# `--path-spec` 结构化路径匹配设计（0.4.0 实现状态）

> 实现位于 `src/pathspec.rs`；用户文档见 `README.md` 的 PATH SPEC 段与 `ux --help`；夹具与断言见 `tests/pathspec_scenario.rs`、`tests/pathspec.rs` 和 `tests/fixtures/pathmatch/data`；CLI 整体设计见 `docs/cli-design.md`。

## 1. 定位

`--path-spec` 用**结构化段语法**从目录树中筛出文件，并把段内命名捕获作为只读来源元数据带出。它与位置参数互斥，基准是当前工作目录，专用于「路径本身带语义」的场景（例如按时间戳目录 + 固定子路径挑 NativeHeap dump）。

设计目标是让**筛选发生在遍历过程中**：不匹配的目录根本不进入，而不是先递归整棵树再做全路径正则。

边界：

- 只做段级结构匹配，不做跨段通配；目录结尾的 spec 不隐式追加 `**`，末段按文件段处理。
- captures 是元数据，绝不注入记录流、绝不参与求值。
- 不做日历/时间语义校验，有效性由调用方写进 `re:` 片段。
- 遍历深度由 spec 的段数决定；`--max-depth` 在该模式下被忽略并告警。

## 2. 语法

spec 是 `/` 分隔的段序列，每段由字面片段与占位符组成：

```
data/{ts:re:\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}}_SRRG/{svc:glob:*}/txt/{f:glob:*.txt}
```

占位符只有两种，均为显式命名：

| 形式 | 片段语义 |
|---|---|
| `{name:re:...}` | Rust 正则片段，原样拼进 `(?P<name>...)` |
| `{name:glob:...}` | 单段 glob：`*` → `[^/]*`、`?` → `[^/]`，其余字符按字面转义 |

规则：

- 捕获名必须是合法 Rust 正则组名且全局唯一；重复名字在编译期拒绝。
- 段数 ≤ 64，captures ≤ 32。
- `glob` 不接受字符类 `[...]`（含字面 `]`），需要字符类时改用 `re:`。
- `re:` 片段禁止顶层 `|` 与不配平括号：片段会被拼进 `(?P<name>...)`，`a)|.*|(?:b` 一类输入会生成匹配任意文件名的分支，让段级剪枝失效。嵌套在 `(?:...)` 内的 alternation 合法；`\d{4}` 之类量词不参与括号计数；字符类以 `]` 开头时（`[]a]`）按字面成员处理。
- 转义支持 `\{` / `\}` / `\\` 三种；`split_segments` 负责分段时的花括号深度，`tokenize` 负责段内字面量，其他 `\X` 保留为字面两字符。
- 0.3.1 起不接受 `time:` / `lit:` / 简写占位与任何未知 kind，也不生成 `ts_fields` 派生字段；这类 spec 在任何目录被打开之前返回 `Usage(2)`。

## 3. 编译

`compile(spec)` 产出 `Compiled { segments: Vec<Segment>, ... }`，每个 `Segment` 保存一个独立 `Regex` 与该段的捕获名列表。

流程：

1. `split_segments`：按 `/` 切段，转义字符不参与花括号深度计数。
2. `tokenize`：把段拆成字面片段与占位体，字面部分用 `regex::escape` 转义。
3. `parse_placeholder`：`splitn(3, ':')` 得 `(name, kind, value)`；只识别 `re` / `glob`，其余返回 `Usage(2)`。
4. `validate_re_fragment`：扫描 `re:` 片段的圆括号深度、字符类与转义状态，拒绝顶层 `|` 与不配平括号。
5. `glob_fragment`：把单段 glob 翻译成正则片段；遇到字符类直接报错并提示改用 `re:`。
6. 拼装段正则 `\A(?:...)\z` 并交给 `regex::Regex::new` 兜底语法错误；失败统一为 `Usage(2)`。

`Cli::validate()` 在 `--list` 与正常运行的共同路径上调用 `compile`，因此预演与实跑看到同一套输入集与错误，且非法 spec 不会触碰文件系统。

## 4. 段级遍历与剪枝

`discover()` 维护一层 `frontier`：每个节点携带「当前目录路径 + 已匹配的段序号 + 前缀 parts + 已收集的 captures」。

- 逐层从 `fs::read_dir` 取条目，只把**当前段正则命中**的条目推进到下一层；未命中的目录整棵剪掉。
- 推进子层时按节点复制 `parts` / `captures`，每个 frontier 节点彼此独立。
- 中间段要求 `is_dir()`；末段要求 `is_file()`。符号链接文件按 `file_type()` 判定后跳过。
- 目录段由 `DEFAULT_IGNORED_DIRS` 过滤（`--no-default-ignore` 可关闭）；`--exclude` glob 与 `--fuzzy` 对归一化相对路径生效。
- 去重按 `canonicalize()` 结果的集合；非 UTF-8 文件名经 `to_string_lossy()` 参与匹配。
- 最终文件按归一化相对路径字典序排序，与 `--list` 输出顺序、实际处理顺序一致。
- frontier 按层展开而不设节点数上限：扇出由 spec 片段的宽窄决定，调用方用窄片段（具体目录名、受约束的 `re:`）控制规模。

## 5. captures

`capture_json()` 返回 `BTreeMap<String, JsonValue>`，值**一律是字符串**：

- 键按名字字典序输出，与 spec 中的出现顺序无关。
- 多段捕获按遍历顺序合并到同一个 map，重名已在编译期拒绝。
- 0.4.0 起，`re:` 片段内的嵌套命名组（`{file:re:.*(?<ts>\d{8}_\d{6})}` 里的 `ts`）也是捕获：`Regex::capture_names()` 枚举全部命名组，显式占位名跳过、嵌套名注册进同一 map；跨段重名（含嵌套组与显式占位撞名）在 `compile()` 里 `Usage(2)`，同段重名由 regex crate 自身拒绝。0.3.1 曾静默丢弃嵌套组——匹配成功但字段不可见（实测反馈 P1）。
- captures 挂到 `Origin.captures`，仅通过 `--list` 的输入清单与 `--with-source` 的来源对象暴露。
- 求值阶段（DSL、`jq`、`xpath` 等）拿不到 captures；它们不是记录字段，也不会被隐式注入。

需要基于时间语义做过滤时，把有效性写进 `re:` 片段，或把 `--with-source` 的 JSONL 交给外部工具解析 `source.captures.ts` 字符串。

## 6. 诊断与错误矩阵

三类遍历异常不再静默跳过，统一由 `warn_io(kind, path, error, reporter)` 构造，格式 `kind=<kind> path=...: <error>`，经 `Reporter` 输出：

| kind | 触发点 |
|---|---|
| `read_dir` | `fs::read_dir` 打开目录失败，跳过该目录 |
| `read_dir_entry` | `ReadDir` 迭代单条目失败，跳过该条目 |
| `file_type` | `entry.file_type()` 失败，跳过该条目 |

文本模式为 `ux: warning: ...`；`--error-format json` 为 `{"code":"warning","message":...,"exit_code":0}`；`--quiet` 压制这三类告警。

| 场景 | 结果 |
|---|---|
| 未知 kind / `time:` / `lit:` / 简写 / 空段 / 捕获重名 / 片段括号逃逸 | `Usage(2)`，编译期，不打开任何目录 |
| `glob` 字符类 | `Usage(2)`，提示改用 `re:` |
| `--path-spec` 与位置参数并存 | clap `conflicts_with` → `Usage(2)` |
| `--max-depth` 与 `--path-spec` 并用 | warning：深度由段数决定，该参数被忽略 |
| 目录结尾 spec | 不追加 `**`，末段按文件段匹配 |
| 零匹配 | 退出 0，stderr 一行 `no input files matched` |
| 三类遍历异常 | warning，跳过对应目录或条目后继续 |

## 7. 夹具与验证

`tests/fixtures/pathmatch/data` 有 12 个 NativeHeap 风格文件，是夹具的单一事实源；`scenarios/pathmatch/generate.py` 负责重建它们并写入该目录。

| 测试 | 项数 | 内容 |
|---|---|---|
| `src/pathspec.rs` 内嵌单测 | 11 | 编译、转义与冒号正则、量词不闭合占位、正则逃逸拒绝、三类告警的文本/JSON/quiet 行为、目录尾不递归、非法 spec 提前失败、嵌套命名组进 captures、跨段重名 `Usage(2)` |
| `tests/pathspec.rs` | 7 | CLI 端到端：合法 spec、`--list` captures、`--with-source`、非法 kind 的 `Usage(2)`、简写负向用例 |
| `tests/pathspec_scenario.rs` | 8 | 12 夹具的字节集合、非法 spec、目录尾不递归、4/12 精确命中、显式 `re:` 排除 `02-30`、剪枝计数、3 条 pss 抽取与 `--with-source`、captures 子对象字节相等与 `--quiet` / glob / fuzzy 对照组 |

CI 由 `.github/scripts/check_contract_docs.py` 校验夹具数量恰为 12，并扫描权威文档中过期的占位语法与旧契约措辞。

## 8. 生效的决策

| # | 决定 | 理由 |
|---|---|---|
| B1 | 只保留显式 `re:` / `glob:` 占位 | 隐式 kind 推断与派生字段让错误难以定位，语义校验应显式写在正则里 |
| B2 | 每段独立 `Regex` + frontier 逐层剪枝 | 不匹配的目录不进入，扇出由段语法控制 |
| B3 | `re:` 片段做括号/顶层 `\|` 结构校验 | 防止片段逃逸捕获组使剪枝静默失效 |
| B4 | captures 降级为只读字符串元数据 | 来源信息与记录数据分离，求值行为不随 spec 改变 |
| B5 | 三类遍历异常统一告警 | 静默吞错会让「零匹配」与「丢文件」不可区分 |
| B6 | 目录结尾不追加 `**` | 显式语法，避免意外递归整棵树 |
| B7 | 夹具与断言迁移到 Rust 测试 | 单一事实源，无需 Python 解释器与 shell glob |
| B8 | `--max-depth` 在该模式下告警而非静默忽略 | 参数被忽略必须可观测 |
| B9 | 嵌套命名组并入 captures，跨段重名编译期拒绝 | 静默丢弃会让「文件名拆字段」这一主用例拿到一半结果；拒绝重名保证 map 键无歧义 |

## 9. 文档索引

`README.md`（用法与边界） · `ux --help`（PATH SPEC 段） · `CHANGELOG.md`（0.3.1 变更与迁移表） · `docs/cli-design.md`（CLI 整体设计） · `tests/pathspec_scenario.rs`（可执行断言）。
