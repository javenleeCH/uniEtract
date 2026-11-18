# 反馈与提问：把工具问题交回上游

`ux` 由维护者按 Issue / Discussion 里的可复现报告演进。Agent 在使用过程中遇到「工具本身不对劲」或「拿不准是不是预期行为」时，**应主动提交一份报告，而不是自己绕过**——绕过会让同一个坑在别人的会话里重复出现，而报告能让它被修掉、被写进契约文档、并随下一个版本回到 skill 里。

## 先自查，再报告

按顺序排除「不是 ux 的问题」，避免低质量报告：

1. `python scripts/ux_probe.py` 的 `ok` 为 `false`：先修环境（`ux` 未安装、版本过旧、PATH 不对）。
2. `ux --help`、`references/cli-contract.md`、`references/recipes.md` 是否已覆盖该用法。
3. 用 `--list` 确认输入集：路径、格式、`--path-spec` 捕获是否符合预期。
4. 加 `--error-format json` 再看一次：`code` / `exit_code` 是否已经指出问题（`usage`、`query`、`runtime`、`io`、`eval`、`limit_exceeded`）。

自查后仍无法解释，或确认是工具缺陷：提交报告。

## 两条通道

| 现象 | 通道 | 标签 / 分类 |
|---|---|---|
| 崩溃、错误退出码、输出与文档不符、数据被静默跳过、契约自相矛盾 | **Issue**（bug） | `agent-report` + `bug` |
| 语义/边界/扩展性的疑问，或想确认「这样做对不对」 | **Q&A Discussion**（疑问） | Q&A 分类 |

不要让疑问沉在会话里：Discussion 可被其他 Agent 检索，维护者回答一次就服务所有后来者。

## 提交

```powershell
python <skill-directory>/scripts/ux_feedback.py report `
  --kind bug `
  --title 're() named group returns only the first match on a log file' `
  --summary '在 multi.log 上命名捕获组只产出 1 条，rc=0 且 stderr 为空' `
  --command 'ux --error-format json -f text -O text -e "re(r"user=(?<id>\d+)")" multi.log' `
  --expected '每个匹配行一条记录（3 条）' `
  --actual '只输出 1 条，无诊断' `
  --include-output captured-stderr.txt `
  --probe
```

- `--kind question` 会把报告投递到 Q&A Discussion。
- `--dry-run` 只打印报告、不提交：拿不准措辞或含敏感信息时先过一遍。
- 提交前脚本会尽量创建 `agent-report` 标签；仓库没有该标签时自动退回不带标签的提交。
- 报告里的环境（`ux --version`、可执行文件路径、OS、Python）由脚本自动采集；`--probe` 附上预检 JSON。

### 写清楚这四件事

1. **命令**：完整、可粘贴，含 `-e` 原文与关键参数（`-f`、`--path-spec`、`--error-format json`）。
2. **期望**：依据契约文档的哪一句、哪条退出码。
3. **实际**：`rc`、stdout/stderr 原文（截断到能复现的部分）。
4. **数据形态**：几百字节的最小样例，说明结构即可——**不要**粘贴真实业务数据。

## 先搜重

```powershell
python <skill-directory>/scripts/ux_feedback.py search --query "split jsonl records"
```

输出 JSON 列出相似 Issue 与 Discussion。已有同类报告时**补充复现细节**（评论/跟帖），不要另开重复条目。

## 没有网络或没有 `gh` 时

脚本会把完整报告写到 `.ux-feedback/outbox/<时间>-<kind>-<标题>.md`，并打印提交命令或网页入口（`gh issue create ...`，或 `https://github.com/javenleeCH/uniEtract/discussions/new?category=q-a`）。把该文件交给人类提交即可；退出码 `3` 表示「已生成、未提交」，不是失败。

## 隐私红线

- 不粘贴密钥、令牌、Cookie、客户数据、完整业务日志。
- 只用最小可复现样例；把路径、字段名改成占位符（`<user>`、`/path/to/file.jsonl`）。
- 报告是公开的：提交前假设全世界都能看到。

## 提交之后

- 维护者用 `.github/scripts/feedback_inbox.py` 增量检索新 Issue / Discussion（见仓库 `docs/feedback-loop.md`），复现、修复并回复关闭。
- 修复随版本发布后，重新安装/更新 skill；再次运行 `ux_probe.py` 确认版本与契约一致。
- 维护者在报告中要求补充信息时，按 `gh issue comment <number> --body-file reply.md` 回复；不要另开新 Issue 续聊同一问题。
