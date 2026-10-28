---
phase: M5
title: 打磨、文档与发布
depends_on: [M2, M3, M4]
exit_gate: 仓库可作为 AGPL-3.0 项目发布，v0.1.0 安装包与 .xpi 可交付
estimate: 10-14 人日
status: planned
---

# M5 打磨与发布

M5 把功能齐全的原型变成可交付项目：设置补全、状态统一、文档与许可证齐备、CI 与发布可重复、性能与安全有证据。发布版本 `0.1.0`，平台只承诺 Windows。

## 全局依据

- 页面与状态：[APP_FLOW](../../docs/APP_FLOW.md)
- 设计主参考：[figma](../design/figma.md)；设计系统与主题：[FRONTEND_GUIDELINES](../../docs/FRONTEND_GUIDELINES.md)
- 产品范围与成功标准：[PRD](../../docs/PRD.md)
- 精确版本：[TECH_STACK](../../docs/TECH_STACK.md)
- 接口与存储：[BACKEND_STRUCTURE](../../docs/BACKEND_STRUCTURE.md)
- 会话断点：[PROGRESS](PROGRESS.md)

设置布局、字段、错误文案不在此复制，按 APP_FLOW 与 FRONTEND_GUIDELINES 实现。

## 1. 范围

**做**

- 设置六个分区补全：通用、Anki、语境辅助、浏览器扩展、缓存、关于。
- 三套浅色主题：figma 默认，apple/cursor 只换配色。
- 管理器、弹窗、扩展的空态/错误态/加载态与键盘可达。
- Anki 字段映射校验、同词多卡策略与连接引导。
- 语境辅助设置、隐私说明、AI 释义与推荐义项文案。
- README、用户指南、配置参考、扩展文档、开发文档、故障排查。
- 许可证、第三方清单、SPDX 头、`cargo-deny`。
- CI 全量检查、tag 发布工作流、版本同步。
- 性能复测、8 小时长跑、安全审查、干净虚拟机验收。
- v0.1.0 发布。

**不做**

- macOS/Linux 支持与验收。
- 代码签名、AMO 发布、自动更新。
- Anki 媒体上传、URL 字段、多语言界面。
- 云同步、账户、遥测。

## 2. 任务分解

### T5.1 设置补全

按 [APP_FLOW](../../docs/APP_FLOW.md) §7 与 [FRONTEND_GUIDELINES](../../docs/FRONTEND_GUIDELINES.md) 实现：

- 通用：热键录制、主题三选一、查询策略、条数、启动行为。
- Anki：连接状态、endpoint、牌组、模板、四字段映射、标签、同词多卡策略、刷新、三步安装引导。
- 语境辅助：选择器/LLM 开关、地址、阈值、超时、API key 环境变量、回环限制、测试、缓存清理、隐私说明。
- 浏览器扩展：启用、端口、令牌显示/复制/轮换、运行状态、配对步骤。
- 缓存：TTL、统计、清理；语境缓存单独清理。
- 关于：版本、构建信息、配置/日志/缓存目录、许可证、仓库链接、重开引导。

保存与回滚规则：

- 热键先应用后写盘；失败恢复旧热键。
- 端口变更重启桥接；失败保留输入并显示占用错误。
- 令牌轮换需二次确认，旧令牌立即失效。
- 主题即时生效；0.1.0 没有深色主题。
- 选择器/LLM 默认关闭；远程端点必须显式允许。

**验收**：六个分区全部可用；热键冲突不丢旧值；配对与轮换可完成；语境辅助测试请求不写缓存。

### T5.2 UI 打磨

- 三个窗口的 loading/empty/error 三态统一。
- 弹窗长词、长释义、图片溢出、音频控件、3+ 来源不破版。
- 管理器空态、源失败、查询测试空态。
- 键盘导航、焦点环、aria、对比度、reduced motion。
- 主题切换后布局不位移，不出现第二强调色。
- 无内联 style、无任意值 hex、无组件库。

**验收**：150% 缩放与最小窗口无截断；键盘可完成主要任务；三主题对比度达标。

### T5.3 Anki 与扩展文案

- 字段映射刷新与保存前校验；模板改名/牌组不存在有清晰动作。
- 未运行 AnkiConnect 显示安装/重启指引，不写测试卡片。
- 扩展 401、离线、端口占用、令牌轮换各自有可操作文案。
- 推荐义项、AI 释义、同语境提示在桌面端与扩展文案一致。
- 不显示原始堆栈。

**验收**：手工走完 Anki 正常/未运行/字段缺失；同词不同句连续保存；低置信度 AI 卡片；扩展 401/离线/成功。

### T5.4 文档

- 重写根 `README.md`：定位、功能、系统要求、安装、AnkiConnect、扩展、架构图、配置示例、隐私、构建、许可证、已知限制。
- 用户指南：`docs/user-guide.md`。
- 配置参考：`docs/configuration.md`，字段与默认值必须与 BACKEND_STRUCTURE 一致。
- 扩展文档：`docs/extension.md`，`.xpi` 构建/安装/未签名限制。
- 开发文档：`docs/development.md`，模块、线程、测试、发布。
- 故障排查：`docs/troubleshooting.md`，覆盖热键、捕获、MDX、图片、Anki、扩展、远程、脚本、语境选择器、LLM、同词多卡。
- 文档写清：AI 结果不是词典原文；同词多卡是默认行为；同语境提示可覆盖；LLM 原句外发与回环默认。
- 现有 PRD/APP_FLOW/TECH_STACK/FRONTEND_GUIDELINES/BACKEND_STRUCTURE 随实现更新，不另起重复文档。

**验收**：按 README 从干净克隆可构建；文档命令可复制执行；链接无死链。

### T5.5 许可证与合规

- 保留 AGPL-3.0 `LICENSE`；README 与关于页声明。
- 新增源文件加 `SPDX-License-Identifier: AGPL-3.0-only`。
- `THIRD_PARTY_LICENSES.md` 覆盖直接依赖；libmdx AGPL、WXT/React MIT、web-ext MPL（构建期）、DOMPurify 双许可、scraper ISC。
- `deny.toml` 允许 AGPL-3.0-only、MIT、Apache-2.0、BSD、ISC、Zlib、Unicode-3.0、CC0；拒绝未知。
- `[sources] allow-git` 只放 libmdx；其余未知 git 拒绝。
- libmdx 的 license 字段不是合法 SPDX：先提上游 PR，合并前用只改该字段的 fork/patch 并固定 rev，不得在 deny 配置忽略。
- 前端 `pnpm licenses list` 人工核对；不打包字体。

**验收**：`cargo deny check licenses bans sources` 通过；清单无缺项。

### T5.6 CI 与发布

- CI：前端 typecheck/lint/test/build；扩展 test/lint/build；Rust fmt/clippy/test；`cargo deny`；Ubuntu 只跑平台无关 core 检查。
- 发布：tag `v0.1.0` 触发 Windows 安装包（NSIS/MSI）、`.xpi`、SHA256SUMS、GitHub Release 草稿。
- 版本同步：`package.json`、`src-tauri/Cargo.toml`、`tauri.conf.json`、`extension/package.json`；改完 `cargo check` 更新锁文件。
- 不签名；发布说明写 SmartScreen 与 Firefox 未签名限制。
- 发布前干净克隆按 README 构建一次。

**验收**：工作流可产出可安装产物；版本四处一致；SHA256 与发布页一致。

### T5.7 性能与可靠性

目标：

| 指标 | 目标 |
| --- | --- |
| 热键到弹窗可见 | p95 ≤200ms |
| 本地查询 | p95 <100ms（热，10 万条级） |
| 首结果 | p95 ≤350ms |
| 启动到托盘 | ≤1.5s |
| 空闲内存 | ≤150MB |
| 500MB 词典后内存 | ≤500MB |
| 远程超时 | 配置+200ms 内 |
| 语境选择器 | p95 ≤800ms，不阻塞词典首屏 |
| LLM 兜底 | 低置信度时才调用，超时 15s 内静默 |
| 8 小时内存增长 | <10% |

- 记录硬件、系统、词典、方法、原始数据到 `docs/performance.md`。
- 配置损坏备份并默认启动；缓存损坏重建；worker 崩溃可读错误 + 一次重启；端口冲突可改。
- 8 小时长跑，每 10 分钟查询一次，记录内存/句柄。

**验收**：数据可复现；长跑无崩溃与明显泄漏。

### T5.8 安全审查

逐项记录结论：

1. CSP 只允许本地脚本；生产不放行 `ws:`。
2. 所有词典 HTML 经 ammonia；脚本/事件属性/危险链接测试通过。
3. mdx-res 拒绝路径穿越与非白名单 MIME。
4. 桥接只绑回环，令牌、Origin、限流、请求体限制生效。
5. 日志不含令牌、完整 URL、选中文本。
6. 令牌明文存储于用户配置，文档说明；不声称加密。
7. 脚本沙箱无宿主能力，UI 有信任警告。
8. 依赖审计通过；无不兼容 copyleft。
9. 无遥测/CDN/远程字体；外部 https 图片属于词典内容，文档说明。
10. 安装包未签名，发布页明确。

**验收**：十项均有结论；高危为零。

### T5.9 验收与发布

- 干净 Windows 虚拟机：安装 Anki+AnkiConnect、安装应用、加真实 MDX/MDD、记事本与 Firefox 划词、编辑、制卡、扩展配对、远程 mock、断网缓存、重启、卸载。
- 对照 [PRD](../../docs/PRD.md) 成功标准与 [APP_FLOW](../../docs/APP_FLOW.md) 主路径逐项打勾。
- 代码审查：无 `unwrap`（非测试）、无 `any`、无内联 style、IPC 类型只从 `src/lib`、无 `0.0.0.0`、Anki 请求 `allowDuplicate: true`、锁文件已提交、生成目录未提交。
- 发布 0.1.0，更新 PROGRESS 与 CHANGELOG。

## 3. 已知限制（写入 README）

1. 仅 Windows 10/11；macOS/Linux 未验收。
2. 仅 MDX/MDD，不保证全部私有变体。
3. 桌面句子捕获尽力而为。
4. 扩展未签名；正式版需临时加载或自行签名。
5. Anki 导出不含本地媒体，只留占位文本。
6. 四字段没有 URL；页面来源不写入卡片。
7. QuickJS 沙箱不是强隔离，只运行可信脚本。
8. 安装包未签名，SmartScreen 可能拦截。
9. 配置/缓存明文存本机，不提供云同步。
10. 不提供自动更新。
11. 义项切分依赖词典 HTML 结构；无法拆分时整条释义作为一个候选。
12. JEV-like 置信度是模型自评，可能过度自信；阈值可调但仍需人工确认。
13. AI 释义由 LLM 生成，可能不准确；默认关闭，必须手动采用。
14. 同一单词允许重复卡片；应用层只提示“同词同句同释义”，不保证 Anki 侧历史完全一致。

## 4. 风险

| 风险 | 对策 |
| --- | --- |
| 文档与实现漂移 | 发布前按文档走一遍；配置表逐项核对 |
| 未签名安装被拦截 | 发布说明写清，不伪造签名 |
| 扩展安装限制 | Dev/Nightly 与临时加载两条路径 |
| 真实词典差异 | 记录失败样本，不承诺全格式 |
| 大词典内存 | 限制 worker、记录目标、文档推荐配置 |
| 脚本沙箱误解 | UI 与文档明确边界 |
| 平台扩展成本 | 0.1.0 只承诺 Windows |
| 许可证遗漏 | deny + 人工复核 |
| 性能未达 | 记录差距，不删指标 |

## 5. 提交拆分

```text
feat(settings): complete general anki sense bridge and cache
feat(ui): unify empty loading and error states
fix(ui): improve keyboard navigation and accessibility
docs: rewrite readme and add user guide troubleshooting
docs(sense): document selector llm and multi-card boundaries
docs(extension): add build install and signing limits
chore(license): add spdx headers and third party list
ci: add deny extension and release workflow
perf: record final performance and soak results
chore(release): prepare v0.1.0
```

## 6. 出口检查清单

- [ ] 设置六个分区可用，保存/回滚一致。
- [ ] 热键、主题、Anki 映射、扩展配对、缓存、语境辅助通过手工测试。
- [ ] 同词不同句连续保存多张卡；同语境提示可覆盖。
- [ ] AI 释义标注模型、默认不选中；选择器/LLM 失败不影响词典。
- [ ] README 与 docs 完成，命令可执行，配置表与实现一致。
- [ ] 语境辅助隐私说明与远程端点警告写入文档。
- [ ] LICENSE、第三方清单、SPDX、deny 配置齐全。
- [ ] CI 与发布工作流可产出安装包与 `.xpi`。
- [ ] 性能目标有数据，差距有记录。
- [ ] 8 小时长跑通过。
- [ ] 安全十项有结论。
- [ ] 干净 Windows 虚拟机验收通过。
- [ ] v0.1.0 release 含安装包、`.xpi`、SHA256、变更说明、已知限制。
- [ ] PROGRESS 更新到发布状态。
