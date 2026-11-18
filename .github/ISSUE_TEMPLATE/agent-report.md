---
name: Agent 报告（工具问题）
about: Agent 使用 ux 时遇到的可复现缺陷（崩溃、错误退出码、与契约不符、静默丢数据）
title: '[agent] '
labels: ['agent-report']
---

<!--
优先用 skill 里的脚本自动填报，它会采集环境、预检并盖上 agent-report 标签：

  python <skill-directory>/scripts/ux_feedback.py report --kind bug \
      --title "..." --command "ux ..." --expected "..." --actual "..." --probe

本模板用于手写或补交报告。请删除不适用的小节，不要粘贴密钥、客户数据或完整业务日志。
-->

## 概要

<!-- 一句话说清哪里不对、影响什么 -->

## 环境

- ux 版本（`ux --version`）：
- 可执行文件路径：
- OS / 架构：
- Agent 客户端（如 Codex / Claude Code / CodeBuddy）：

## 复现

```powershell
# 完整命令：保留 -e 原文与本轮相关参数（-f / --path-spec / --error-format json）
```

最小输入样例（结构即可，勿用真实数据）：

```jsonl
```

## 期望

<!-- 依据 references/cli-contract.md 或 --help 的哪一句 -->

## 实际

- 退出码：
- stdout：
- stderr（`--error-format json` 原文）：

## 预检

```json
// python <skill-directory>/scripts/ux_probe.py
```

## 已尝试的规避

<!-- 绕行方案是什么；若绕过会改变结果，请说明影响 -->
