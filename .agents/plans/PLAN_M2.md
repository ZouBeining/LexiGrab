---
phase: M2
title: 桌面端 MVP
depends_on: [M1]
exit_gate: Windows 记事本或浏览器划词后弹窗显示释义并成功写入 Anki
estimate: 12-16 人日
status: planned
---

# M2 桌面端 MVP

M2 把 M0 的窗口与 M1 的核心库接起来，交付可日常使用的 Windows 应用：热键、弹窗查词、本地词典管理、编辑与制卡。远程源与桥接仍不启用。

## 全局依据

- 页面、导航、焦点流程：[APP_FLOW](../../docs/APP_FLOW.md)
- 命令、错误码、事件、桥接与存储契约：[BACKEND_STRUCTURE](../../docs/BACKEND_STRUCTURE.md)
- 设计主参考 [figma](../design/figma.md)，实现规则 [FRONTEND_GUIDELINES](../../docs/FRONTEND_GUIDELINES.md)
- 范围与成功标准：[PRD](../../docs/PRD.md)
- 会话断点：[PROGRESS](PROGRESS.md)

本计划只写阶段任务与验收；命令表、字段、CSP、布局细节不在本计划复制。

## 1. 范围

**做**

- Command 层、AppState、统一 IPC 错误。
- mdx-res 协议、资源渲染、CSP。
- 弹窗快速模式、加载/空/错误态、编辑模式、焦点恢复。
- 本地源增删改、启停、排序、状态、查询测试。
- 设置骨架：热键、主题、策略、Anki 连接与字段映射。
- 首次启动引导、AnkiConnect 引导。
- 托盘、单实例、关闭到托盘。
- 桌面端尽力而为的句子捕获。
- 同词多卡：`saved_cards` 语境指纹、Anki 始终允许重复、同语境提示。
- 前端测试、命令集成测试、性能测量。

**不做**

- 远程源、JSONPath、脚本、缓存。
- 本地桥接、令牌、扩展。
- Anki 媒体上传；导出时剥离本地媒体。
- 跨词条跳转、发音播放。
- macOS/Linux 验收。
- 深色主题；主题值只有 `figma`、`apple`、`cursor`。

## 2. 目录

```text
src-tauri/src/{lib,main,commands}.rs
src-tauri/src/core/{state,ipc,resources,runtime}.rs
src-tauri/src/anki/export.rs
src-tauri/src/platform/windows/{dismiss,uia}.rs
src/lib/{ipc,types,events,resource,format,theme}.ts
src/components/{Button,Card,Field,SourceBadge,Spinner,EmptyState}.tsx
src/windows/popup/{index.tsx,PopupApp,ResultView,EditView,state}
src/windows/manager/{index.tsx,ManagerApp,SourceList,SourceDialog,SourceStatus,QueryTester}
src/windows/settings/{index.tsx,SettingsApp,GeneralSection,AnkiSection,HotkeyInput}
```

## 3. 任务分解

### T2.1 AppState 与 Command 层

- 按 BACKEND_STRUCTURE §4 实现命令；`commands.rs` 只做薄封装，逻辑在 core。
- `AppState`：配置、Registry、Router、Anki、最近选区、QueryMeta、运行时状态。
- IPC 错误统一映射；日志保留完整错误链。
- 事件类型集中在 `src/lib/events.ts`，组件不手写字符串。
- `lookup`、Anki 调用放 `spawn_blocking`。

**验收**：命令可从前端调用；错误码稳定；空查询/禁用源/单源失败有测试。

### T2.2 mdx-res 与 CSP

- 注册 `mdx-res`；同时解析 `mdx-res://` 与 Windows 的 `http://mdx-res.localhost/...`。
- 先按 `/` 分段，再解码；拒绝 `..`、`.`、反斜杠、空段、超长、非法 UTF-8。
- 只允许 image/audio/video/font/text-css；其余 404；加 nosniff 与缓存头。
- 前端 `resource.ts` 把逻辑 URI 转成平台可渲染 URI；单测覆盖 Windows/非 Windows。
- CSP 与开发 `devCsp` 按 FRONTEND_GUIDELINES 与安全要求配置，不复制整段到本计划。
- 释义 HTML 只在 `DefinitionView` 中渲染；链接交系统浏览器打开。

**验收**：夹具图片/音频显示；路径穿越返回 400/404；非白名单 MIME 不返回；DevTools 无 CSP 报错。

### T2.3 弹窗快速模式

- 启动时预创建隐藏 popup；热键后定位并 `SW_SHOWNOACTIVATE`。
- 事件与状态拉取兜底，避免首次监听丢失。
- 状态：capturing → loading → results / empty / error。
- 布局按 [APP_FLOW](../../docs/APP_FLOW.md) 与 FRONTEND_GUIDELINES；420×560。
- 保存、编辑、关闭、打开管理器/设置。
- 弹窗关闭或新查询后丢弃旧 query_id 事件。
- 远程进度区域在 M2 隐藏，M4 启用。

**验收**：热键到可见 p95 ≤200ms；焦点保持；空选/无结果/查询失败有可操作界面。

### T2.4 编辑模式与制卡

- 点击编辑：窗口 560×640，清除 NOACTIVATE，激活并聚焦。
- 释义“预览/源码”tab；原句 textarea；单词/来源只读。
- `Ctrl+Enter` 保存，Esc 取消；保存失败保留输入。
- 按 BACKEND_STRUCTURE §2.4、§6 组装 payload；来源去重逗号连接，空写“未知来源”。
- 导出前再次清洗；本地媒体移除并插入占位文本。
- **始终 `allowDuplicate: true`**；不同原句/义项直接保存。
- 保存前查 `saved_cards` 同词同句同释义指纹；命中只提示，用户可“仍然保存”。
- 成功显示 note id 后保留弹窗，提供“再存一张”“关闭”；恢复焦点只在关闭时做。

**验收**：四字段正确；同词不同句/不同义项可连续保存；未运行/字段缺失有明确提示；编辑后可恢复焦点。

### T2.5 管理器

- 列表、空态、添加/编辑/删除、启停、排序、状态。
- 文件选择器多选 `.mdx`，提示同名 `.mdd`，拒绝重复路径。
- 保存后立即返回，后台打开；订阅 `dict://index-progress`。
- 查询测试页使用草稿配置，不写缓存。
- 大词典加载不冻结 UI。

**验收**：重启后源保持；删除只删配置；失败源可重试；查询测试展示 Entry 与耗时。

### T2.6 设置骨架

- 分区与字段按 [APP_FLOW](../../docs/APP_FLOW.md) §7。
- 热键录制、保存时应用，冲突回滚且不写盘。
- 主题三选一：figma / apple / cursor；只换 CSS 变量。
- Anki：连接探测、牌组/模板、字段映射、标签、同词多卡策略（提示/静默）。
- 扩展与缓存分区显示“M3/M4 提供”，不放无效控件。

**验收**：热键冲突不丢旧值；主题即时生效；字段映射保存前校验。

### T2.7 托盘、单实例、生命周期

- `single-instance` 第一个注册；二次启动显示 manager。
- 托盘：打开管理器、设置、退出。
- manager/settings 关闭隐藏；退出注销热键、停 worker。
- 启动顺序：配置 → 日志 → 单实例 → 托盘 → manager → 预创建 popup → 热键 → Anki 异步探测。
- 首次启动无源时显示三步引导。

**验收**：关闭窗口后热键可用；二次启动不双开；托盘退出干净。

### T2.8 桌面端句子捕获

- 文本捕获后并行尝试 UIA 段落，预算 120ms；失败留空，不等待。
- 仅 UIA，不模拟按键选句；结果只进 `SelectionContext.sentence`，不写日志。
- 用户可在编辑模式修改；空句不阻塞制卡。

**验收**：支持应用可取到句；不支持时弹窗不被拖慢；`sentence` 为空仍可保存。

### T2.9 同词多卡与语境指纹

- 按 BACKEND_STRUCTURE §3 建 `saved_cards`；指纹用 NFKC + 小写 + 压缩空白 + uuid v5。
- 保存前 `check_card_context`；不同原句或不同释义不提示。
- 命中时展示“此语境已存过”，主动作“仍然保存”，次动作“换义项”。
- 保存成功后写入 note_id；写入失败不阻断 Anki 结果。
- 设置项 `duplicate_policy = warn | allow`，默认 warn。
- 不在 Anki 字段中追加指纹/URL，保持四字段契约。

**验收**：同一单词 3 个不同句子连续保存 3 张卡不出现阻断；同词同句同释义提示一次且可覆盖；删除/重建缓存不影响历史提示。

### T2.10 测试与性能

- Rust：资源协议（合法/404/穿越/MIME/非 GET）、命令、导出媒体剥离、句子捕获失败、配置回滚、Registry 重载、同语境指纹。
- 前端：resource 转换、popup reducer、payload 组装、query_id 过期、多卡片提示。
- 手工矩阵：记事本、Chrome、空选区、无结果、制卡成功、Anki 未运行、同词不同句、同词同句不同义项、编辑、字段错误、添加词典、大词典、重启、关闭窗口、单实例。
- 性能：热键→可见 p95 ≤200ms；热键→首结果 p95 ≤350ms；本地查询 p95 <100ms。
- 数据写入 `.agents/notes/M2-acceptance.md`。

## 4. 阶段决策

| 决策 | 结论 |
| --- | --- |
| 资源 URI | 逻辑 `mdx-res://`，Windows 渲染为 `http://mdx-res.localhost/` |
| 弹窗编辑 | 不新增第四窗口，popup 切换编辑态 |
| 预创建 | 启动时创建隐藏 popup |
| 媒体导出 | 移除 img/audio/video/source，插入占位文本 |
| 同词多卡 | Anki `allowDuplicate: true`；应用层指纹只提示同语境 |
| 主题 | figma 默认，apple/cursor 只换色；0.1.0 不做深色 |

## 5. 风险

| 风险 | 对策 |
| --- | --- |
| 自定义协议平台差异 | 第一天真机验证两种 URI |
| 事件早于监听 | 状态 + 拉取命令兜底 |
| 编辑抢焦点失败 | 重试 AttachThreadInput；失败按 D0-1 记录 |
| 大词典打开慢 | 后台打开 + 状态事件 |
| CSP 阻断资源 | 同时放行两种 URI，DevTools 验证 |
| 链接导航走 WebView | 统一拦截交系统浏览器 |
| 用户误以为关窗即退出 | 首次关闭提示 + 托盘菜单 |
| 同词多卡被误判重复 | 始终 allowDuplicate；指纹只做提示；不同语境不阻断 |
| 指纹碰撞/漏判 | 规范化 + uuid v5；提示可覆盖；不作为唯一性约束 |

## 6. 提交拆分

```text
feat(core): add app state and ipc error boundary
feat(core): register mdx-res protocol and resource handler
feat(commands): add lookup source and settings commands
feat(popup): implement capture to result flow
feat(popup): implement edit mode and anki export
feat(manager): implement local source management
feat(settings): implement general and anki settings
feat(core): add saved cards context fingerprint
feat(app): add tray single instance and lifecycle
test: cover resource handler and popup state
test: cover same-word multi-card flow
perf: precreate popup and measure latency
docs(notes): record m2 acceptance results
```

## 7. 出口检查清单

- [ ] 记事本/浏览器划词 → 弹窗 → 制卡成功。
- [ ] 快速模式不抢焦点，编辑模式可输入并恢复。
- [ ] MDX 图片/音频显示，Windows URI 正确。
- [ ] 本地源增删改、启停、排序、重启保持。
- [ ] 大词典不冻结 UI。
- [ ] 四字段正确；同词不同句/不同义项可连续保存。
- [ ] 同词同句同释义只提示一次，可覆盖；不同语境不阻断。
- [ ] 设置热键、主题、策略、Anki 可保存并生效。
- [ ] 托盘、单实例、关闭到托盘正确。
- [ ] 自动化测试与手工矩阵通过。
- [ ] 性能达标或有记录。
- [ ] 前端不直接调用 AnkiConnect 或远程词典。

## 8. 交接给 M3/M4

M3 复用 lookup 的 query_id、导出、配置 `[bridge]`；M4 复用 EventSink、RemoteHttpSource 类型与设置入口。CSP、资源协议、弹窗状态机在 M2 稳定后再扩展远程。
