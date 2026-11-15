# 安装 ux-extract skill

`ux-extract` 是一个标准 `SKILL.md` 格式的 Agent 技能包，教 Agent 用 `ux` CLI 做本地信息抽取。本文件随分发包一起提供。

## 前置条件

- Python 3.8+（安装脚本与预检脚本只用标准库，不联网、不改配置文件）。
- `ux` CLI 本身**不是安装前置**：skill 可以先装好，`ux_probe.py` 会报告它是否可用；真正执行抽取任务前需要安装或构建 `ux`（在 PATH 上，或仓库的 `target/release/ux` / `ux.exe`）。

## 一键安装

### 情况 A：只拿到了 `ux-extract.zip`

PowerShell（Windows）：

```powershell
Expand-Archive -Path .\ux-extract.zip -DestinationPath .
python ux-extract/scripts/install.py
```

bash / macOS / Linux：

```bash
unzip ux-extract.zip
python3 ux-extract/scripts/install.py
```

默认装到用户级 `~/.codebuddy/skills/ux-extract/`，所有项目都能用。

### 情况 B：clone 了本仓库

```powershell
python .codebuddy/skills/ux-extract/scripts/install.py
# 或从分发包安装（等价，便于验证 zip 内容）
python .codebuddy/skills/ux-extract/scripts/install.py --from-zip dist/ux-extract.zip
```

### 安装到当前项目而非全局

```powershell
python ux-extract/scripts/install.py --project
```

结果：`<当前目录>/.codebuddy/skills/ux-extract/`，随仓库共享给协作者。

### 安装到其他 Agent

```powershell
python ux-extract/scripts/install.py --target <该 Agent 的 skills 根目录>
```

只要目标 Agent 识别 `SKILL.md` 格式即可。例如 Claude Code 常见路径为 `~/.claude/skills` 或 `<project>/.claude/skills`：

```bash
python3 ux-extract/scripts/install.py --target ~/.claude/skills
```

`--target` 与 `--user` / `--project` 互斥；三者都不传时等价于 `--user`。

## 验证

1. 重启或刷新 Agent，让技能列表重新加载。
2. 运行预检，期望返回 `{"ok": true, ...}`：

   ```powershell
   python <安装目录>/scripts/ux_probe.py
   ```

   它会检查 `ux` 是否可用、长帮助是否含 `STAGES` / `AGENT CONTRACT` / `PATH SPEC` / `ROADMAP` / `EXAMPLES`，以及最小调用的 stdout / stderr 分离。
3. 对 Agent 说「用 ux 从这个日志目录里抽出所有 ERROR 消息」触发技能。

## 覆盖与卸载

- 目标已存在时安装脚本默认拒绝覆盖；加 `--force` 替换：

  ```powershell
  python ux-extract/scripts/install.py --force
  ```

- 卸载（同样支持 `--user` / `--project` / `--target`）：

  ```powershell
  python ux-extract/scripts/install.py --uninstall
  ```

## 安装脚本做了什么

1. 校验 `SKILL.md` 的 frontmatter `name` 与目录名一致；若从 zip 安装，先拒绝 `..` 与绝对路径成员。
2. 把整个 `ux-extract/` 复制到目标 skills 根目录下。
3. 运行 `ux_probe.py` 做预检；`ux` 不可用只提示，不算安装失败。
4. 不修改 PATH、不写配置文件、不访问网络。

## 手动安装（无 Python 时）

把 `ux-extract/` 整个目录复制到 Agent 的 skills 根目录即可，目录名必须保持 `ux-extract`。

## 分发

- 分发包：`dist/ux-extract.zip`（仓库内），或 GitHub 仓库 `javenleeCH/uniEtract` 的 release / 仓库文件。
- 仓库内的 skill 源：`.codebuddy/skills/ux-extract/`。
- 重新打包（维护者）：

  ```powershell
  $env:PYTHONIOENCODING='utf-8'
  python <skill-creator>/scripts/package_skill.py .codebuddy/skills/ux-extract dist
  ```
