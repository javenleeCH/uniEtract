# 反馈闭环：Agent 报告 → 增量检索 → 修复 → 回到 skill

`ux` 的稳定契约只写在文档与 `--help` 里，但契约的**漏洞**往往由使用方 Agent 最先撞上：某条路径没有诊断、某个组合的退出码和文档不一致、某个边界没人写清楚。这条闭环把「撞上」变成「修复」，并让修复随版本回到 skill，下一次会话不再重复踩坑。

## 角色

| 角色 | 职责 |
|---|---|
| 使用方 Agent | 装 `ux-extract` skill；自查后仍无法解释时，提交 Issue（缺陷）或 Q&A Discussion（疑问） |
| 维护方 Agent | 运行 `feedback_inbox.py` 增量检索新报告，复现、修复、回复并关闭 |
| 人类 | 无网络/无 `gh` 时代为提交；对语义取舍拍板；发布版本 |

## 通道

| 现象 | 通道 | 标签 / 分类 |
|---|---|---|
| 崩溃、错误退出码、与文档不符、数据被静默跳过、契约自相矛盾 | Issue | `agent-report` + `bug` |
| 语义、边界、扩展性等「这样做对不对」的疑问 | Q&A Discussion | `q-a` 分类 |

Issue 模板：`.github/ISSUE_TEMPLATE/agent-report.md`（仓库已创建 `agent-report` 标签；提交时自动带上，标签不可用时退回无标签提交）。Discussion 模板：`.github/DISCUSSION_TEMPLATE/q-a.yml`。`config.yml` 把空白 Issue 入口指向 Q&A，避免疑问散落成低质量 Issue。

## 使用方：怎么提交

skill 自带 `scripts/ux_feedback.py`，`SKILL.md` 与 `references/feedback.md` 已经要求 Agent 主动使用：

```powershell
# 先搜重，再提交
python <skill-directory>/scripts/ux_feedback.py search --query "split jsonl records"

python <skill-directory>/scripts/ux_feedback.py report --kind bug `
  --title 're() named group returns only the first match on a log file' `
  --summary '在 multi.log 上命名捕获组只产出 1 条，rc=0 且 stderr 为空' `
  --command 'ux --error-format json -f text -O text -e "re(r"user=(?<id>\d+)")" multi.log' `
  --expected '每个匹配行一条记录（3 条）' `
  --actual '只输出 1 条，无诊断' `
  --include-output captured-stderr.txt --probe
```

- `--kind bug` → Issue；`--kind question` → Q&A Discussion。
- 报告自动带环境（`ux --version`、可执行文件、OS、Python）；`--probe` 附 `ux_probe.py` 的 JSON。
- 已经写好整段 markdown 时用 `--raw-body -` 一步提交（body 整段送入、环境自动追加）；`--body-file -` 读 stdin 补充上下文。
- `--dry-run` 先看再发；`search` 先查重（`errors` 非空表示检索没跑完，不代表没有重复）；`--timeout` 控制单次 `gh` 调用超时。
- 没有 `gh` 或没有网络时，报告落到 `.ux-feedback/outbox/`（退出码 `3`），脚本打印 `gh issue create ...` 或 Discussion 网页入口，交给人类提交。
- 报告是公开的：只放最小合成样例，不放密钥、客户数据或完整业务日志。

一份合格报告必须写清四件事：**完整命令、期望依依据、实际退出码与 stderr、最小输入形态**。缺其中任一项，维护方通常只能回问，闭环就多一轮。

## 维护方：增量检索

状态文件 `.ux-feedback/state.json` 记录 Issue 与 Discussion 各自的 `updated_at` 水位，已加入 `.gitignore`。`updated_at` 会被评论与重开刷新，所以「旧报告上的新回复」同样会被捞出。

```powershell
# 第一次：看当前全部（不写水位）
python .github/scripts/feedback_inbox.py --all --limit 20

# 日常：只看上次之后新增/有更新的条目
python .github/scripts/feedback_inbox.py

# 分派完把水位推进；下一次就只剩真正的新反馈
python .github/scripts/feedback_inbox.py --mark-seen

# 机器可读：给子 Agent 或脚本消费
python .github/scripts/feedback_inbox.py --format json
```

输出按 `updated_at` 排序，逐条给出 `kind`、编号、状态/标签或分类、作者、评论数、标题与 URL，并附带 `fetched` 计数（开放 Issue、带 `agent-report` 的条目、未被回答的 Q&A）。`--no-issues` / `--no-discussions` 可只取一路；`--since ISO8601` 覆盖水位做重放。

### 处理一次报告的固定动作

1. 复现：按报告里的最小样例与命令跑一遍，确认退出码与 stderr。
2. 定性：
   - 可复现缺陷 → 修复代码 + 回归测试；文档与契约同步更新。
   - 疑问 → 在 Discussion 里回答；若答案改变了契约，落进 `docs/cli-design.md` 或 `references/cli-contract.md`。
   - 环境问题（`ux` 未安装、shell 吞了 `//a`）→ 回复指向 `INSTALL.md` / 契约里的陷阱清单。
3. 回复并关闭：`gh issue comment <number> --repo javenleeCH/uniEtract --body-file reply.md`、`gh issue close <number> ...`；Discussion 选一条答案后标记 answered。
4. 发布后回填：CHANGELOG 记录修复，skill 与 zip 重新打包，下次 Agent 跑 `ux_probe.py` 就能对上版本。

关闭标准只有一条：**报告里的命令现在给出期望结果，或文档明确写出「这是预期行为」并说明原因**。两者都不满足时不要关。

## 门禁与自检

- `feedback_inbox.py --selftest` 在 CI 的 verify job 里离线跑（纯函数 + 状态文件读写，不碰网络）。
- `ux_feedback.py selftest` 同样在 CI 里跑：报告分区、标题落盘、路径 slug、离线落盘。
- `check_contract_docs.py` 把本文件、两个脚本、Issue/Discussion 模板列为必需文件，缺一即失败。
- `check_skill_zip.py` 保证 `dist/ux-extract.zip` 与 `.agents/skills/ux-extract/` 逐文件一致，新增脚本不会只在仓库里存在。

## 隐私与信任边界

报告进入公开仓库前按「全世界可见」处理：不贴密钥、令牌、Cookie、客户数据；路径、字段名用占位符；只保留能复现缺陷的最小结构。维护方在回复中同样不引用报告人未公开的数据。
