# 用户角色评审

> 评审人：用户角色（终端用户 + AI Agent 调用方）。评审对象：`ux`（UniExtract）0.3.1 工作树。
> 视角：一个真实用户/一个 Agent 拿到这个工具，能否不踩坑、不误解、高效完成任务。
> 证据以「文件:行号」标注，均基于对 README / `src/cli.rs` 长帮助 / `sample/CASES.md` / `ux-extract` SKILL / 源码的静态推演。

## 一句话结论

`ux` 在「错误要吵」这件事上做得极好（坏 XML、非法 UTF-8、`--max-results` 触顶、坏 parameter 全部显式报错退出非 0），但在「零结果要暗示」这件事上**恰恰是静默的**——而零匹配正是本工具最高频、最难自救的失败路径；「宁可吵不可骗」的哲学落在了硬错误上，却漏掉了最该吵的那一次「凭空空输出」。命名与 DSL 总体小而清晰，但 `--quiet` 的语义、`-f auto` 的误判、以及 Skill 里「默认带 `--quiet`」的教学，会共同把一个 Agent 引向「成功退出 + 空输出 = 没数据」的错误结论。

## 首用体验走查

> 全部在 Windows PowerShell（README 示例默认 shell）下核验。README 的「30 秒上手」「快速开始」用 `powershell` code fence。

| # | 命令（出处） | 判定 | 说明 |
|---|---|---|---|
| 1 | `'{"id":1}', '{"id":2}' \| ux -f jsonl -O text -e 'jq(.id)'`（README:21） | ✓通过 | 纯 ASCII JSON，PowerShell 管道逐元素喂 stdin，`\|` 在单引号内不会被拆。输出 `1\n2\n`。 |
| 2 | `ux --list .\sample\basic\`（README:22） | ✓通过 | 反斜杠路径在 PowerShell 正常；`--list` 不需要 `-e`（cli.rs:22 required_unless list/completions）。 |
| 3 | `ux --help`（README:23） | ✓通过 | 长帮助齐全，SKILL 预检正是靠 `STAGES/AGENT CONTRACT/PATH SPEC/ROADMAP` 段做断言（INSTALL.md:80）。 |
| 4 | `ux -e 'jq(.items[].msg) \| re(...) \| jq({id})' a.json`（README:30） | ?存疑 | 语法对，但 `a.json/page.html/app.log/jobs.jsonl/report.txt` 全是**抽象占位名**，仓库根目录不存在。首用照抄必得 `cannot open: no such file or directory`（rc=1）。与「30 秒上手」用真实路径 `.\sample\basic\` 形成不一致——同一个 README 里，只有 30 秒段能零依赖照跑，快速开始全部要自己造文件。 |
| 5 | `ux -e 'xpath("//a") \| attr("href") \| re(r"^https://(?<host>[^/]+)") \| jq(.host)' page.html`（README:33） | ✓通过（PowerShell） | `//a` 在 PowerShell 不会被改写（改写只发生在 MSYS2/Git Bash，README:232 已自述）。但这条恰恰是最高危句式：换到 Git Bash 就**静默零匹配**。 |
| 6 | `Get-Content app.log -Encoding utf8 \| ux -f text -O text -e 're(...) \| jq(.message)'`（README:36） | ✓通过 | `-f text` 显式，正确。`Get-Content` 逐行管道无问题。 |
| 7 | `ux -f jsonl -O text -e 'slurp() \| jq(length)' jobs.jsonl more.jsonl`（README:39） | ✓通过 | 显式 `-f jsonl` 规避了无扩展名误判；`slurp()` 语义正确。 |
| 8 | `ux -f text --text-mode raw -e 'split(...) \| re(...)' report.txt`（README:42） | ✓通过 | 无 shell 陷阱。 |
| 9 | `ux --fuzzy prodconfig --skip-incompatible -e 'jq(.database.host)' .`（README:45） | ✓通过 | 与 CASES#43 等价，已由 smoke 覆盖。 |
| 10 | `ux --list .\logs\` ／ `ux --quiet --error-format json -f jsonl -e '...' .\logs\`（README:48–49） | ?存疑 | `.logs\` 目录不存在（占位）。但**更关键**：第 49 行这条「Agent 推荐姿势」自带 `--quiet`，会把 `no input files matched` / `binary input skipped` / 空文件三类本可解释「空输出」的 warning 全部吞掉（见「静默失败雷区」）。 |

小结：PowerShell 下 README 命令本身没有跑不通的，主要问题是 (a) 快速开始的输入文件名是虚构的、(b) README 用 PowerShell、而「真实可复制命令清单」`CASES.md` 全是 bash（README:198 才补一句 Git Bash 要 `MSYS2_ARG_CONV_EXCL`），两套示例的坑位不同，跨读易踩。

## 静默失败雷区清单

「静默」的准确定义：`rc=0`（成功）且 stdout 为空，或 stdout 有输出但**少/错了**而没有任何 stderr 提示。以下按「多么隐蔽 × 多么难自救」排序。

| # | 场景 | 现象 | 用户/Agent 如何察觉 | 建议改进 |
|---|---|---|---|---|
| 1 | XPath 零匹配：HTML 隐式 `<tbody>`、大小写折叠（`//A`）、错命名空间 URI、谓词未命中（CASES#28、#34；agent.rs:426 明确断言「zero matches = clean stderr」） | rc=0，stdout 空，stderr 空 | 无法从工具侧察觉，只能靠「我知道应该有结果」的经验反推 | **最高优先**：文件输入且「打开了 ≥1 个非空文件、读了 N 条记录、产物 0 条」时，默认向 stderr 打一行可压制的汇总 warning（如 `processed 3 files, N records: 0 results`）。用 `--quiet` 才关闭，而非反过来。 |
| 2 | MSYS2/Git Bash 把 `-e` 里的 `//a`、`//b:book` 改写成 UNC 路径（README:232；CASES.md:16） | rc=0，stdout 空，stderr 空 | 环境相关，用户几乎不可能自己想到是 shell 在改参数 | 1) `ux` 可检测 XPath 以 `//` 开头且运行在已知危险环境时提示（难，因为参数已改）；2) 至少把「若在 Git Bash 且表达式含 `//`，请设 `MSYS2_ARG_CONV_EXCL`」放进 `--help` 的 STAGES 段而非只埋在 README/已知限制。 |
| 3 | 输入侧 BrokenPipe 被 `AppError::Io(BrokenPipe)` 一律判 0 且不写诊断（error.rs:78–80；lib.rs:185–187, 220；README:235） | rc=0，stdout 部分/空，stderr 空，**但上游管道根本没读完整** | 无法察觉；`rc=0` 被当成「读完了」会得出错误统计 | 0.4.0 已定输入侧改 `runtime(1)`（README:249）。这是正解，务必落地；在此之前 README 已如实告警，但 Agent 不会每次读 README。 |
| 4 | `--quiet` 隐藏「文件被跳过/空文件/没命中输入」（diagnostic.rs:27–30 warn 全被 quiet 吞；discover.rs:90–92、source.rs:52–54,71–73、lib.rs:197–205 全是 warning） | rc=0，stdout 空/缺，stderr 空 | 只能靠去掉 `--quiet` 重跑对照 | **最高优先**：把「可能会改变结果集」的告警（`no input files matched`、`binary input skipped`、`incompatible`、`empty file`）提升为不被 `--quiet` 压制，或拆成独立 `--quiet`（只压噪音）+ 默认不吞这些。当前 `--quiet` 的真实语义是「闭嘴」，和用户直觉的「安静」对不上。 |
| 5 | `--skip-incompatible` 静默跳过格式不匹配文件（lib.rs:197–205，warning） | rc=0，只输出部分结果 | 不加 `--skip-incompatible` 就是 rc=1 能看出来；加了之后跳过被 `-q` 隐藏就全丢了 | 同 #4：跳过是 warning 且可被 `-q` 吞。 |
| 6 | `slurp()` + `--with-source` 只保留第一条来源（engine.rs:167–170 `first_origin`；README:228） | 溯源元数据部分丢失，但输出量/退出码无异常 | 除非知道这条已知限制 | 至少在 `--with-source` 生效且存在 slurp 时打一个 warning，而不是仅埋进「已知限制」。 |
| 7 | `attr(NAME)` 对缺失属性产出 0 条而非报错（README:60） | 节点命中了但属性名拼错 → 静默少结果 | 靠 `--list`/人工对照 | 无法区分「节点选了但属性缺失」与「节点零匹配」；可考虑 warning 或至少在 attr 后对 drop 计数。 |
| 8 | `-f auto` 对无扩展名 JSONL 误判为 JSON（source.rs:274–314；CASES#26–27） | **这个不是静默**：rc=1 + `invalid JSON`，好 | —— | 已经很「吵」，保留。建议再加一句可执行提示：`invalid JSON ... (file looks like JSONL: multiple JSON lines; try -f jsonl)`，把「知道错了」升级为「知道怎么改」。 |
| 9 | 空文件：0 字节文件现在打 warning（source.rs:52–54，0.4.0 起），但空 stdin 不警告（source.rs:50–51） | 文件场景可察觉；管道场景 rc=0 空 stderr | 文件场景靠 warning；stdin 场景无法察觉 | 文件侧已修好，保留；stdin 的「空」与「上游没写」无法区分是客观事实，继续如实文档即可。 |
| 10 | `split()` 空片段丢弃、不 trim（README:61） | 切分后静默少了字段 | 靠对照 | 低危，文档已写明。 |

### 对「宁可吵不可骗」的评估结论

该哲学当前的落地是**不对称**的：硬错误极端吵（坏 XML rc=1、非法 UTF-8 rc=1、`--max-results` 触顶 rc=1、坏 DSL rc=2、panic 收敛 rc=1），这非常好；但**最高频、最致命的软失败（零匹配）被显式做成了最安静的成功**（agent.rs:426 甚至把 `zero matches → clean stderr` 写成契约测试）。所以问题不是「对新手太重」，而是「该吵的地方没吵、不该吵的地方很吵」。**核心改进建议**：新增「读入统计 + 产物统计」的 stderr 汇总（默认开、可 `--quiet` 关），它同时解决雷区 #1/#4/#9 的自救问题，且不违背 stdout 纯净契约（只写 stderr）。

## 命名与心智模型

| 主题 | 评价 | 建议 |
|---|---|---|
| 阶段名 `jq/xpath/re/text/attr/split/slurp` | `text()` 是一个**零参数阶段**，看起来像类型而不是动作，第一次见「要拿文本所以写 `text()`」很拧；`re()` 太省，`regex()` 更直觉；`slurp()` 是 jq 内行黑话（对非 jq 用户不透明）。`attr()`/`split()` 清晰。 | `text()` 保留但帮助里写清是「取 string-value 的动作」；`re` 可加 `regex` 别名；`slurp` 已是 jq 生态共识，可保留并配一句白话。 |
| `-f`（输入格式） vs `-O`（输出模式） | `-f` 缺省 `auto`，多数时候不用想，但恰恰「不用想」把无扩展名 JSONL 陷阱藏了起来。`-O` 大写仅因要与 `-o` 区分，但 `-o` 并未占用，属可读性小折损。 | 保留，但把「`-f auto` 对无扩展名 JSONL 会误判，请显式 `-f jsonl`」做成 `--help` STAGES 段第一行，而非埋在 README 已知限制。 |
| `--quiet` | 真义是「压掉所有 warning（含跳文件/空文件/丢输入）」，名与实不符，且被 README/Skill 的推荐姿势默认带上，危险。 | 见雷区 #4：拆分 warning 严重度，或改默认。 |
| `--list` vs `--dry-run` | `--dry-run` 名字暗示「不真正产出」，但 `--list` 明确产出清单 JSONL。两者语义清晰，仅别名可懂性小差别。 | 保留。 |
| `--with-source` | 名字抽象，用户未必知道会**包一层** `{value, source:{...}}` 并改变输出形状，且与 `-O text` 互斥（到 `Usage(2)` 才撞）。 | 保留，help 里一句「把每条结果包成 `{value,source}`」即可降低误解。 |
| `--path-spec` | 语法 `{name:re:...}`/`{name:glob:...}` 严格而强大，但 0.3.1 删掉了 `time:/lit:/简写`（README:90），要求手写 `\d{4}-\d{2}-\d{2}` 正则且**无日历校验**（path-spec.md:40）。对首用者偏陡，NativeHeap 示例是一整条不可读的长命令。 | 保留严格性（符合「显式优于隐式」），但把「删掉的简写」在文档里用**负向迁移表**讲清（CHANGELOG:22 已有，README 略薄）。 |
| 退出码 0/1/2 | 清晰、稳定，很好。 | 保留。 |

## Agent 可用性评价（读完 `ux-extract/SKILL.md`）

**会让 Agent 踩坑的点（按概率）**：

1. **默认带 `--quiet` 的教学自相矛盾**。SKILL 自己在 workflow 第 4 步写「Add `--quiet` only when suppressing warnings is acceptable (never with pipe input when completeness matters)」，但 `references/recipes.md:109,113` 与 README:142 的「推荐 Agent 姿势」都是 `ux --quiet --error-format json ...`。一个 LLM 复制「无人值守」配方 → 带上 `--quiet` → 丢掉 `no input files matched`/`binary input skipped` → 把「文件被跳过」当成「真的没数据」。这是**最高优先要改**：把 recipe 里的 `--quiet` 移除或在同一条目里强标注「只在确知输入集完整时加」。

2. **SKILL 从不教 Agent 做「输入数 vs 输出数」对账**。它教 `--list` 先定输入集，但没教「M 个输入文件、产出 0 条时如何判断对错」。而零匹配正是 rc=0 且 stderr 空（agent.rs:426），属于「没有诊断可读」的场景，SKILL 的「Interpret the result 读 `code`/`exit_code`」直接失效。建议 SKILL 增补一条硬规则：**stdout 空时必须返回 `--list` 的交集与文件数，不能只报『成功』**。

3. **HTML 隐式 `<tbody>` / 大小写折叠不在 Hard rules**，只出现在「Troubleshooting」尾部（SKILL「Zero matches with exit 0」一句带过）。这是 HTML 抽取头号静默零匹配源，却被放在 troubleshooting 末尾而非硬规则。建议挪进 Hard rules。

4. **预检依赖 Python**（`ux_probe.py`）。一个只有 `ux` 二进制、没有 Python 的 Agent 会被这步卡住或跳过。小摩擦，可接受，但应在 SKILL 里给「无 Python 时跳过预检、直接靠 `--help` 判能力」的降级路径。

5. **`--version` 不可信 → 靠 `--help`/实测**（SKILL「Preflight」）是对的、防御性的，保留。

**SKILL 中未见事实性错误或过时表述**（硬规则逐条核对无误：`--with-source`×`-O text` 互斥、无扩展名 JSONL 用 `-f`、`--max-results` 是硬错误、`--ns` 前缀、MSYS2 陷阱、BrokenPipe 不可作完整性证明——全都与源码/README 一致）。问题不在「教错」，而在「教得不够防守」：它给的是「如何跑通」，不是「如何不被静默零匹配骗」。

## 建议更改或移除的设计

### 保留（做对了，别改）

1. **硬错误必显式 + 稳定 0/1/2 退出码 + 结构化诊断**（error.rs、diagnostic.rs）。坏 XML、非法 UTF-8、`--max-results` 触顶、坏 DSL、panic 收敛为 `runtime(1)`——这条「宁可吵」的骨架是同类 CLI 的榜样。
2. **stdout 只承载数据 / stderr 每条诊断一行**。这是 Agent 安全性的根基，勿动。
3. **`--list` 先定输入集、`--no-stdin` 防挂起、TTY 保护**（cli.rs:187–194）。防 Agent 挂起设计得很到位。
4. **`-f auto` 无扩展名误判仍报 rc=1**（而不是静默）。这证明团队有能力让「误判」可观测，只是漏了零匹配这一头。
5. **`--path-spec` 编译期校验前置**（cli.rs:151–156），坏 spec 打开目录前 `Usage(2)`。

### 必须改（最高优先）

1. **补一条「结果统计」stderr 汇总**（默认开、`--quiet` 关）：文件输入时输出 `processed N files · R records · M results`。一行解决零匹配、空文件、跳文件三类的自救盲区，且不碰 stdout 纯数据契约。这是本次评审最该落地的一项。
2. **收紧 `--quiet` 语义**：把「改变结果集的告警」（`no input files matched` / `binary input skipped` / `incompatible` / `empty file`）从 `--quiet` 的压制范围里挪出来，或拆成 `--suppress-input-warnings` 显式才压。同时**移除 README:142 与 recipes.md:109,113 里「推荐 Agent 姿势」自带的 `--quiet`**。
3. **修正 SKILL 内部矛盾**：Hard rules 增补「stdout 空必须与 `--list` 对账输入数」；把 HTML `<tbody>`/大小写陷阱升入 Hard rules；recipes 的无人值守示例去掉 `--quiet` 或加醒目反例。

### 应该改（体验优化，非阻塞）

4. `invalid JSON` 时追加 `try -f jsonl`（无扩展名 JSONL 的一键自救提示）。
5. `--with-source` + `slurp()` 时打 warning「只保留第一条来源」，而非只埋已知限制。
6. `warn` 的 JSON 对象补结构化 `path`/`file` 字段（当前跳文件/空文件的路径只藏在 `message` 字符串里，机器消费要靠脆弱的字符串解析，违背「结构化诊断」初衷）。
7. README「快速开始」的抽象文件名改为指向 `sample/basic/*.json` 等真实固定夹具，或明标「替换成你的文件」；让首用者零心智照抄能跑（30 秒段已经是真实路径，快速开始应看齐）。
8. `--help` 的 STAGES 段补一行 MSYS2 `//a` 提示与 `-f auto` 无扩展名提示（当前只在 README/已知限制）。

### 建议移除/重新考虑

9. **不要把「零匹配 = clean stderr」继续写进契约测试**（agent.rs:426）。不是要改成退出非 0，而是改为「零匹配 = stderr 出现一条可压制的汇总」——把这条契约测试跟着 #1 一起改掉，否则新告警会与旧契约冲突。
10. `--quiet` 别名 `--silent`（cli.rs:108）在当前「吞掉关键 warning」语义下是危险的第二入口，建议在 #2 收紧语义后再保留，或去掉别名避免 Agent 图省事用 `-s`。