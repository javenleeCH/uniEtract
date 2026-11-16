# 开发角色评审

> 评审对象：仓库根目录 `ux`（UniExtract）。本机工具链：`rustc 1.100.0` / `cargo 1.100.0`，**未安装 1.88.0**（`rustup toolchain list` 仅 stable-x86_64-pc-windows-msvc）。已实测 `cargo build --locked` 成功（exit 0，dev profile 9.49s，部分缓存）。无 `unsafe` 自查（`src/` 内 0 处）。

## 一句话结论

工程化基线**高于同类小型 CLI 的平均水平**：错误链一致（`thiserror` + 单一 `Reporter` + 稳定 `0/1/2`）、无 `unsafe`、`--locked` 可复现、发布链路由 4 个纯 Python 门禁脚本机器化交叉校验；主要改进空间集中在**「文档/脚本身上的单一真源被多份 hand-write 计数与硬编码锚定」**以及**「skill 分发包 `dist/ux-extract.zip` 与 `.agents/skills/` 源缺一道自动一致性门禁」**两处工程债，属 P1/P2 而非阻断。

## 代码质量评价

**认可的点**

- **错误处理高度一致**。`src/error.rs` 用 `thiserror` 派生 `AppError`，六类变体 → `ErrorCode`（`query/usage/eval/runtime/limit_exceeded/io`）→ `exit_code()`（2 错参数/编译，1 运行期）→ `diagnostic()`（结构化 JSON）四层映射收口清晰，且 `error.rs:130-190` 有稳定性单测兜底。全仓未发现手写 `Display/evaluate` 混用。
- **stderr 单收口**。`src/diagnostic.rs::Reporter` 是唯一写 stderr 的通道，`--quiet`/`--error-format json` 语义集中在一点，`diagnostic()::放对了对象。这是本项目最漂亮的一处收口。
- **`pub` 面收敛到位**。`src/lib.rs:1` 只 `pub mod cli`，其余模块 `pub(crate)`；对外仅 `run`/`parse_pipeline` 两个函数且文档明确「库 API 不承诺稳定」（`docs/cli-design.md:9`、`CHANGELOG.md:31`）。
- **panic 收敛与文档自洽**。`lib.rs:19-55` 无条件 `catch_unwind` → `runtime(1)`，JSON 模式 `main.rs:28` 装空 panic hook，与 README/契约一致。
- **注释质量高**。`dsl.rs:56-70` 对「为何不用 `pest::set_call_limit`/`stack-guard`」的解释、`source.rs:248-272` 对 UTF-16 与二进制检测互斥的说明、`pathspec.rs:207-255` 对 `re:` 片段逃逸的风险声明，都是可读性标杆，不是凑字数的注释。

**需改进**

- **路径归一化三处重复且不一致**。`src/lib.rs:90,94`（`--list`）与 `src/model.rs:123`（`source_json`）各自 `.to_string_lossy().replace('\\', "/")`，逻辑重复；而 `src/error.rs:109-115` 的 eval 诊断 `path` 字段却**不做**正斜杠归一化 —— 同一路径在 `--with-source`/`--list` 里是 `D:/x`、在 `--error-format json` 的 `path` 里是 `D:\x`，Agent 对账三份输出时仍会失配。建议抽 `fn slash_normalize(path: &Path) -> String` 单一助手，三处共用。
- **轻微 dead indirection**：`error.rs:64-66` 的 `fatal` 只是 `error` 的原地委派；`engine.rs:98-106` 的 `execute_range` 只是 `execute_from` 的一行包装。可删其一。
- **`cli.rs` 长帮助是巨型内联字符串**（`cli.rs:6-14`，`after_long_help` 被截断处仍有数百行），内容与 `README.md`/`docs/cli-design.md`/skill 的 `references/cli-contract.md` 四份并行。`check_contract_docs.py` 只做**禁令短语**扫描（`BANNED_DOCS`/`BANNED_ALL`），不校验四份文档正向一致性，text 漂移只能靠 `tests/cli.rs` 里对 PATH SPEC / ROADMAP 文案的少量断言兜着。建议为 `--help` 引入「从单一源生成」或至少对 stage 表/退出码表做快照断言。

## 构建·依赖·工具链评价

- **无 `unsafe`**：`grep '\bunsafe\b' src/` 0 命中。仅传递依赖 `unsafe-libyaml`（`Cargo.lock:1219`，`serde_yaml_ng` 引入）含 C 风格 unsafe 解析器，属「已知、单一、可控」，可纳入安全审计白名单即可，不必砍。
- **依赖可精简单位有限**：`cargo tree --edges normal` 直接依赖 21 个、全树约 183 个条目。大头非本项目引入，而是 lib 选型决定：
  - jaq 三件套（jq 阶段）→ 拖入 `jiff`/`jiff-tzdb`（完整 datetime + tzdb，`Cargo.lock:406-455`）；
  - `sxd_html`（HTML）→ `html5ever`/`markup5ever`/`tendril`/`string_cache`/`phf`/`web_atoms`；
  - `pest` 默认 `std` feature → `pest_stack_guard`→`stacker`→`psm`→`cc`/`ar_archive_writer`/`object`（代码注释 `dsl.rs:68-70` 已如实指出关 `stack-guard` 也移不掉它们）；
  - `toml 0.9` 自身导致 **两个 `winnow` 版本**（`0.7.15` 与 `1.0.5`，`Cargo.lock:1292,1298`）与 **两个 `syn` 版本**（`2.0.119`/`3.0.10`）。
  这些是功能对价的传递依赖，**不是可随手砍的重复依赖**；真正的可行动作是：`Cargo.toml` 依赖版本放得偏宽（`pest = "2"`、`regex = "1"`、`clap = "4.5"` 而 lock 已到 `4.6.7`），可考虑把规律性升级交给 Dependabot/Renovate 而不是手动 lock 漂移。
- **锁文件治理**：`Cargo.lock`(v4) 与 `fuzz/Cargo.lock`(v4) 双锁，`fuzz/Cargo.toml:22` 用 `[workspace]` 空表把自己摘出根 workspace（避免 libfuzzer 拖入 `cargo test`），`check_fuzz_manifest.py` 与 `check_release_version.py:87-90` 都对双锁做了 `--locked`/版本一致性校验 —— 这个是加分项。
- **MSRV 1.88 / edition 2024 取舍合理**。edition 2024 最低 Rust 1.85，1.88 高于下限且是 CI 基准（`ci.yml:34-36` 明确「避免 `-D warnings` 随新版 lint 漂移」）。但**本机默认 1.100 与 CI 1.88 分叉**没有本地强制（详下）。

## 开发体验与门禁摩擦

- **`check_test_count.py` 的 `162` 硬锚定是把「加测试」变成「改三份文档」的负担**。实测 `src/` 内 **90 个** `#[test]`（`grep` 计数），`docs/cli-design.md:136` 已手工改成 162/90/11，`CHANGELOG.md:47` 改成 162，`check_test_count.py:13` `EXPECTED=162` —— 每次新增/删除一个用例要同步 4 处且没有任何自动更新命令。这能防漂移，但对贡献者摩擦大。建议二选一：(a) 保留硬锚定，但在脚本里 `print(f"... run: sed ...")` 给出一键更新命令；(b) 把「总数」降为「下限断言」（`>= 当前值`）同时保留「每套件个数」表由生成器重算，避免一个用例删除就 red。
- **本地工具链无 pin**：仓库**没有 `rust-toolchain.toml`**（`glob **/rust-toolchain*` 空）。README `:200-206` 让开发者手动 `cargo +1.88.0 fmt/clippy/test`，但本机默认 1.100 —— clippy `-D warnings` 在 1.88 与 1.100 报的 lint 集合不同（README 自己也承认 `clippy::uninlined_format_args` 只在 1.88 报）。建议提交 `rust-toolchain.toml`（`channel = "1.88.0"`，components rustfmt/clippy，targets 补 mac 交叉目标）或提供 `just`/`Makefile` 封装三条门禁。
- **`cargo build --locked` 可复现性已验证**：本地 exit 0，9.49s（部分缓存，冷构建会更高）。`.gitattributes` 全仓 LF + 二进制夹具 `binary` 标记（`:19-26`）保证跨平台字节级一致，是细致的好实践。
- 值得肯定：所有门禁脚本是纯 stdlib、`argv` 调 cargo（无 `python -c` 内联、无 `rg`），bash 与 pwsh 行为一致（`check_contract_docs.py:1-8` 显式说明），Windows 上也能本地跑 —— 这是刻意做的可移植性工程，认可。

## 发布与分发链路评价

- **链路整体稳健**：`release.yml` 单一 `dist/release` 输出目录，四平台 matrix（`fail-fast:false`）→ upload-artifact → publish job merge-multiple → `package_release.py --collect` 重算哈希 → `softprops/action-gh-release@v2`。`package_release.py:93-170` 的 `collect` 会核对「每个 target 恰好一个包 + 无多余包 + 哈希重算一致」，`check_release_version.py` 对 `Cargo.toml/Cargo.lock/fuzz/Cargo.lock/CHANGELOG/README/cli-design/ux --version` 七处做交叉校验。**这比多数仓库强**。
- **版本号是多处 hand-write，但已被机器化围住**：真正的单一真源是 `Cargo.toml` 的 `version`，tag 通过 `GITHUB_REF`(或 dispatch 输入) 反向核对 `v{version}`（`check_release_version.py:81-85`），README/CHANGELOG/文档由 `check_release_version.py:87-106` 正则核对。残留风险在**语义层面**：`check_release_version.py:98` 只查 README 含 `ux {version}` 子串，不校验「当前版本」叙述段、`0.4.0 时间表`、下载 URL 里钉住的 `v0.3.1`（`README.md:269`、`INSTALL.md:110`）等手写引用是否同步刷新 —— 版号升到 0.4.0 时，README 里 `releases/download/v0.3.1/ux-extract.zip` 这类**钉死旧版本号的 URL 不会被任何脚本抓到**。建议把「版本字面量出现点」做成可枚举清单（或让脚本对 README/INSTALL 里的 `download/v<semver>` 正则穷举比对）。
- **单点脆弱：`dist/ux-extract.zip` 与 `.agents/skills/ux-extract/` 无自动一致性门禁**（P1，见「发现」）。
- `release.yml:36` 用 `macos-15-intel` 跑 Intel，注释标注了 `macos-13` 2025-12-04 退役 —— 对 runner 生命周期有意识，认可。

## 发现

### P1

**1. skill 分发包 `dist/ux-extract.zip` 可能静默漂移，无门禁。**
- **问题**：`dist/ux-extract.zip`（19549 字节）被提交进仓库，但它是从 `.agents/skills/ux-extract/` **手工打包**的产物；重新打包依赖仓库外的 `package_skill.py`（`INSTALL.md:112-117` 里 `<skill-creator>/scripts/package_skill.py`）。`release.yml:88` 直接用 `--extra-file dist/ux-extract.zip` 原样上传，`package_release.py:123-135` 只校验 zip 存在并算哈希，**从不校验 zip 内容 == 当前 `.agents/skills/` 源**。
- **证据**：`release.yml:88`；`package_release.py:123-135`；`INSTALL.md:112-117`；`.github/scripts/check_contract_docs.py` 的 `REQUIRED` 列表包含 `Cargo.lock` 等但不含 `dist/ux-extract.zip`，也没有任何 hash 比对。
- **影响**：贡献者改了 `SKILL.md`/`references/*` 却忘 repackage，下一版 Release 会把**旧 skill 内容**随新版本号发布（`SHA256SUMS` 还会一本正经给出一个「新」哈希），直接违背 SKILL.md 自己「behavior 与 version 可能分叉」的警惕。
- **建议**：在 CI（verify 或 release 的 build 前）加一步「repackage + diff」：用 stdlib `zipfile` 解 `dist/ux-extract.zip` 与 `.agents/skills/ux-extract/` 逐文件 sha256 比对，不一致即 fail；或 vendor 打包脚本到 `.github/scripts/package_skill.py`，让 zip 从源幂等重生成而非手工提交。可执行思路：
  ```bash
  python - <<'PY'
  import zipfile, hashlib, pathlib
  src = pathlib.Path('.agents/skills/ux-extract')
  z = zipfile.ZipFile('dist/ux-extract.zip')
  want = {p.relative_to(src).as_posix() for p in src.rglob('*') if p.is_file()}
  got = {n for n in z.namelist()}
  # zip 内路径带 ux-extract/ 前缀；比对减去前缀后的集合与逐文件哈希
  ...
  PY
  ```

**2. 主线上已合并多个 0.4.0 行为，但 `ux --version` 仍报 0.3.1，且无门禁防止「行为/版本」错配长期存在。**
- **问题**：`CHANGELOG.md:3-11` 的「0.4.0（未发布）」已含嵌套命名组捕获、空文件告警、skill 迁移、`source.path` 修复等**已实现**行为；`pathspec.rs:632`、`source.rs:52-54` 等代码确实落地；同时 SKILL.md/references 已写成「since 0.4.0 / 0.4.0 起」。但这些功能随 HEAD 编译出来的二进制 `--version` 仍是 `0.3.1`（`Cargo.toml:3`）。
- **证据**：`Cargo.toml:3`(`version="0.3.1"`) vs `CHANGELOG.md:3`(`## 0.4.0（未发布）`) vs `SKILL.md:75`(`since 0.4.0`) vs `pathspec.rs:632`(已实现)。
- **影响**：SKILL.md 自己也警告（`SKILL.md:30`）「working tree 可实现下一版行为却仍报旧版本号，别信 `--version`」——这被当成文档化的现状接受，但没有**机器校验** `--help` 的 ROADMAP/「since X」文案与 `Cargo.toml` 版本的一致性。下一版打 tag 时，很容易出现「README 说 0.4.0 已 GA，但 `--help` 里 PATH SPEC 段仍写 `Since 0.3.1`」的漂移。
- **建议**：把 `--help`/README/CHANGELOG 中所有 `Since X`/`X 起`/`releases/download/vX` 版本字面量纳入 `check_release_version.py` 的可枚举比对清单；或收敛为「文档只写当前版本号，历史版本引用统一由变更脚本改写」。

### P2

**3. `check_test_count.py` 的 `EXPECTED=162` + 4 处手工计数是加测试的累赘（详见上文开发体验）。**
- **证据**：`check_test_count.py:13`；`docs/cli-design.md:136-146`；`CHANGELOG.md:47`。
- **建议**：脚本失败时打印一键 dry-run 更新命令，或改为单套件表由生成器维护、总数做下限校验。

**4. 路径归一化重复且诊断 `path` 不一致。**
- **证据**：`lib.rs:90,94`/`model.rs:123` 归一正斜杠，`error.rs:109-115` 未归一。
- **影响**：Windows 下 Agent 对账 `--list`/`--with-source` 与结构化诊断的 `path` 字段会失配。
- **建议**：抽公共助手统一三处。

**5. 无 `rust-toolchain.toml`，本机默认 1.100 与 CI 1.88 分叉，clippy `-D warnings` 结论可能不一致。**
- **证据**：`glob **/rust-toolchain*` 空；`ci.yml:34-36` pin 1.88；本机 `rustc 1.100.0`；README `:45,200-206` 依赖开发者手敲 `+1.88.0`。
- **建议**：提交 `rust-toolchain.toml`（channel 1.88.0 + rustfmt/clippy），或提供 `just lint`/`just test` 封装三条门禁。

**6. `Cargo.toml` 缺 `repository`/`homepage`/`readme` 元数据。**
- **证据**：`Cargo.toml:1-9` 有 description/license/keywords/categories，无 repository。
- **影响**：若发布到 crates.io（`publish` 未禁），页面与 `javenleeCH/uniEtract`（README 多处引用）断链；也失掉 `cargo install` 后的追溯。
- **建议**：补 `repository = "https://github.com/javenleeCH/uniEtract"` 与 `readme = "README.md"`。

**7. `docs/` 与 `--help` 的正向内容并行冗余未被机器校验（只禁短语）。**
- **证据**：`check_contract_docs.py:44-78` 只做 `BANNED_DOCS`/`BANNED_ALL` 反向扫描；stage 表/退出码表在 `cli.rs`、`README.md`、`references/cli-contract.md`、`docs/cli-design.md` 重复四份。
- **建议**：至少对「7 阶段名称 + 6 error code + 3 退出码」做跨文档快照断言。

**8. 依赖 hygiene 小项**：`toml 0.9` 拖入双 `winnow`/双 `syn`（`Cargo.lock:1060-1069,1291-1301`），`pest` 默认 feature 拖入 `cc`/`stacker`/`psm`（已文档化）。不阻断，建议用 Dependabot/Renovate 规律化管理 `Cargo.toml` 里偏宽的 `pest="2"`/`regex="1"` 锁漂移。

## 开放问题

1. **`fuzz/Cargo.lock` 是否真的有人定期 re-lock？** `fuzz` 是独立 workspace，`check_fuzz_manifest.py` 只验 `--locked` 能解析，不验体系列与主 lock 依赖图同步；`uniextract` 自身的完整性由 `check_release_version.py:87-90` 只比 `version` 字段，不比依赖是否一致。长期看两份 lock 可能各自漂移。
2. **`edit`/`package_release.py` 的 `--extra-file` 语义是「打包时再算一次哈希」，但 `dist/ux-extract.zip` 是预先提交的**：未来若想让 skill 也「随版本打包」，是否改成由 `.agents/skills/ux-extract/` 在 release build 里幂等生成，消除「手工打包」这唯一非机器化步骤？
3. **`check_release_version.py:98` 的 `ux {version}` 子串匹配**能否覆盖「README 多处钉死旧版 `download/v0.3.1` URL」这类升级漏改？是否需要把版本字面量出现点做成 manifest 进行穷举？
4. **并发/异步边界**明确写入「不做」（`docs/cli-design.md:18`），但 `walkdir`/`fs::read_dir` 仍是同步 IO，大目录会阻塞；是否在 0.5.0 重新评估「串行但可中断」的遍历？
5. **`catch_unwind` 兜底后 `panic_message` 只识别 `&str`/`String`**（`lib.rs:57-65`），其余 panic payload 一律 `"unexpected panic"`，丢失类型信息——是否值得在诊断里附上 `payload.type_id()` 帮助定位？

---

*评审方式说明：本机无 1.88.0 工具链（仅 1.100.0），故未执行 `cargo +1.88.0 fmt/clippy/test --all-targets`；已完成 `cargo build --locked`（成功）、`cargo tree`（21 直接 / ~183 全树）、`unsafe` 全仓 grep（0）、`#[test]` 计数（90 单测，与文档 162/90 一致）、zip 内容清单比对。所有行号以当前 worktree 文件为准。*