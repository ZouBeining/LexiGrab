# 文档审查清单

本文件是 [repo-doc-curation](../SKILL.md) 的操作附录。命令以仓库根目录为工作目录；输出前先限定路径，避免扫描生成目录。

## 1. 盘点

```powershell
Get-ChildItem -Force -Name
Get-ChildItem docs -Recurse -File -Name
Get-ChildItem .agents/plans -File -Name
```

只看文件名与层级，不读内容。排除 `node_modules`、`target`、`dist`、`.output`、`.wxt`、`.agents/automation/*/artifacts`。

## 2. 行数与标题地图

```powershell
Get-ChildItem AGENTS.md,PROMPT.md,README.md,ROADMAP.md,docs/*.md,.agents/plans/PROGRESS.md |
  ForEach-Object { "{0,6} {1}" -f (Get-Content $_.FullName).Count, $_.FullName }

Select-String -Path docs/*.md -Pattern "^#{1,3} " | ForEach-Object { $_.Line }
```

行数用来判断入口文档是否超预算（AGENTS ≤60 行）。标题用来判断文档角色是否重叠。

## 3. 事实核对

```powershell
# 清单与锁文件中的版本
Get-Content package.json
Get-Content extension/package.json
Get-Content src-tauri/Cargo.toml

# 命令是否存在
Get-Content .github/workflows/ci.yml
```

用编辑器或搜索工具查旧关键词：已放弃的库名、计划中的库名、`cargo deny`、旧端口、旧命令、引用已改语义的文档名。

重点检查：

- 实际依赖与文档表是否一致；计划值是否被写成锁定值。
- CI 是否真的执行了文档声称的门禁。
- 入口文档中的相对链接是否指向存在的文件。
- 端点、事件、错误码是否与路由、事件常量、错误枚举一致。
- 安全描述是否与实际校验强度一致。

## 4. 链接与预算自动检查

```powershell
node .agents/skills/repo-doc-curation/scripts/check-doc-links.mjs .
```

脚本会扫描除生成目录外的 Markdown，忽略外链、锚点与代码块，报告失效相对链接，并在 AGENTS.md 超过 60 行时失败。

## 5. 人工复核

- 新会话只读 AGENTS + PROMPT + PROGRESS，能否说出下一步做什么？
- 同一契约是否只在一处定义？其他位置是否只链接？
- 人类文档是否混入了只有实现者需要的常量与内部符号？
- agent 文档是否写清了路径、符号、顺序、失败行为与验证命令？
- 已完成的阶段是否与 PROGRESS 一致，未开始的阶段是否没有被写成已完成？

## 6. 收尾

- 更新 PROGRESS 的“已完成/已知问题”。
- 新增的文档至少被一个入口链接到，否则等于不存在。
- 改动单独提交，message 说明是文档重构而不是功能变更。