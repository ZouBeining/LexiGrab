---
name: repo-doc-curation
description: Audit and restructure repository documentation for two audiences. Keeps AGENTS.md and PROMPT.md precise and agent-facing, keeps docs/ and README human-facing, enforces entry-document line budgets, verifies claims against code and lockfiles, and builds a component ROADMAP with a single source of truth. Use when docs drift from code, AGENTS.md is bloated, onboarding is unclear, or a repo needs a documentation pass.
metadata:
  trigger: Reviewing or rewriting AGENTS.md, PROMPT.md, ROADMAP.md, README, or docs/; fixing stale documentation; onboarding agents to a repository
  scope: repository documentation, not product copywriting
---

# Repo Doc Curation

把仓库文档整理成两层：给人读的（产品、交互、架构、契约）和给 agent 读的（入口、实现路径、任务配方、陷阱）。目标是让新会话用最少的阅读量找到正确入口，并让文档能被代码验证。

## 何时使用

- `AGENTS.md` 太长、像流水账，或与代码脱节。
- 有 PROMPT、PRD、README、docs 多套文档，但读者和职责混在一起。
- 需要新增 ROADMAP、组件地图或架构说明。
- 文档里有旧库名、旧版本、旧命令，需要核对后修正。

## 核心判断

1. **读者分层。** 人类文档解释为什么与怎么用，agent 文档写准确路径、符号、顺序、边界条件。不要把字段表同时抄进三份文档。
2. **事实优先级。** 代码、清单与锁文件决定“实际是什么”；契约文档决定“应该是什么”；PROGRESS 决定“做到哪”。冲突时先判断哪一方过期，再统一。
3. **入口有预算。** 总纲只放不可违反的约束与入口，通常 ≤60 行。细则放实现文档，契约放 docs。预算不是排版偏好，是控制新会话的上下文成本。
4. **单一事实源。** 每个契约只有一个权威文件，其他位置用相对链接引用。复制即漂移。
5. **可验证。** 每条版本、命令、路径、端点都要能在代码或清单中找到依据。不要凭印象写“应该支持”。

## 重构流程

1. **盘点，但别全读。** 先列根目录、docs、计划、脚本与 CI；跳过 `node_modules`、`target`、`dist`、`.output`、automation artifacts。用标题和行数建立地图。
2. **给每份文档定角色。** 标记 human / agent / contract / status / design reference。角色重叠的文档拆开或合并。
3. **抽取事实。** 读清单（package.json、Cargo.toml、tauri.conf、wxt.config、CI）、命令注册、模块公开符号、测试目录。只读与文档声称相关的片段。
4. **重写入口。** 先写 AGENTS（硬约束与入口），再写 PROMPT（实现路径），再写 ROADMAP（组件位置与状态），最后补人读架构。
5. **清理旧文。** 删除重复契约、过时版本、失效命令、失效链接；把仍在用的细节挪到正确文档。
6. **自动校验。** 检查行数、相对链接、旧关键词（已替换的库名/端口/命令）、代码块命令是否存在对应脚本。
7. **记录经验。** 更新 PROGRESS；把可复用方法放回本 skill，而不是只留在本次对话里。

## 推荐骨架

### AGENTS.md（≤60 行）

1. 一句话定位 + 接手顺序。
2. 文档地图：哪份给谁看。
3. 会话启动：先读什么、禁止通读、跳过生成目录。
4. 不可违反：目录、技术栈、安全、编码红线。
5. 验证门禁：复制可执行的完整命令。
6. 变更与提交：模块拆分、commit 语言、活文档规则。

### PROMPT.md（agent 实现入口）

1. 使用方式与事实来源。
2. 当前范围与“不要做什么”。
3. 五分钟接手：环境、命令、文件位置、启动顺序。
4. 代码地图：模块 → 文件 → 关键符号与职责。
5. 关键链路：入口到存储的 numbered flow。
6. 契约与不变式：只摘要，细节链接 docs。
7. 任务配方：新增命令/源/解析器/UI/扩展/依赖/文档。
8. 验证矩阵：改动类型 → 最低测试。
9. 已知陷阱：环境、平台、历史偏差。

### ROADMAP.md（人读）

- 当前状态与版本边界。
- 组件表：位置、职责、状态。
- 架构简图与里程碑表。
- 近期方向与相关文档。不要把它写成详细计划；计划留在 `.agents/plans/`。

### docs/ARCHITECTURE.md（人读）

- 系统总览图、分层、一次主链路、存储位置、安全与范围边界。
- 不写版本号、端口常量、字段表；这些属于契约或实现文档。

## 事实核对清单

- 版本：清单与锁文件是否一致；文档中的锁定日期是否已更新。
- 依赖：是否还写着已放弃的库或计划中的库；是否有“计划值”被当成实际值。
- 命令：package scripts、CI step、Cargo bin 是否真的存在。
- 端点/事件/错误码：与路由注册、事件常量、错误枚举对照。
- 路径：目录是否真实存在；文档链接是否相对路径可解析。
- 状态：已完成/未开始的阶段是否与 PROGRESS 一致。
- 安全声明：是否夸大（例如声称校验扩展 ID，实际只校验 Origin 前缀）。

## 反模式

- 把 AGENTS 当 Wiki，越写越长。
- 把 BACKEND_STRUCTURE 的字段表复制进 PROMPT 和每个 PLAN。
- 为了“完整”保留两套互相矛盾的版本说明。
- 只改文档不改测试，或只改代码不回写契约。
- 用“最近更新”代替可验证的日期与证据。
- 扫描自动化产物目录导致 token 爆炸；先排除 artifacts。

## 验证

运行随附脚本检查相对链接与 AGENTS 行数：

```text
node .agents/skills/repo-doc-curation/scripts/check-doc-links.mjs .
```

更多命令与人工核对步骤见 [references/audit-checklist.md](references/audit-checklist.md)。