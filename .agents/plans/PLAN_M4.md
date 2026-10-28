---
phase: M4
title: 远程源、语境辅助与缓存
depends_on: [M2, M3]
exit_gate: 非标准 API 可解析；JEV-like 选义项并在低置信度时生成可用的 AI 释义；远程/辅助失败不影响本地结果
estimate: 14-18 人日
status: planned
---

# M4 远程源、语境辅助与缓存

M4 把数据源扩展到 HTTP，完成两条解析路径、缓存、远程查询策略，并接入 JEV-like 义项选择与 LLM 兜底。远程源与语境辅助失败都必须静默降级。

## 全局依据

- 数据模型、配置、桥接、解析器、缓存 schema：[BACKEND_STRUCTURE](../../docs/BACKEND_STRUCTURE.md)
- 语境辅助与远程查询交互：[APP_FLOW](../../docs/APP_FLOW.md) §6、§6.6
- 设计主参考 [figma](../design/figma.md)，实现规则 [FRONTEND_GUIDELINES](../../docs/FRONTEND_GUIDELINES.md)
- 精确版本：[TECH_STACK](../../docs/TECH_STACK.md)
- 会话断点：[PROGRESS](PROGRESS.md)

请求/响应、脚本 API、选择器/LLM 契约、SQLite 表结构不在本计划复制。

## 1. 范围

**做**

- `RemoteHttpSource`：URL 模板、鉴权头、超时、重试、响应体上限、重定向限制。
- `JsonPathParser` 与 `ScriptParser`。
- rquickjs 沙箱：200ms、16MB、256KB 栈、无宿主能力。
- SQLite 缓存：24h、负缓存 5min、源失效、统计、清理。
- 三种策略的远程语义、事件、QueryMeta。
- 远程源管理界面、测试面板、缓存设置。
- JEV-like 选择器适配：输入候选义项 + 原句，输出义项/置信度；阈值状态机。
- LLM 兜底：OpenAI-compatible、低置信度触发、AI 卡、缓存、隐私门禁。
- 语境辅助设置分区与测试按钮。
- 自动化测试与性能记录。

**不做**

- OAuth/Cookie 会话。
- 脚本网络/文件/定时器。
- 脚本可视化调试器。
- 训练/微调选择器或 LLM。
- 把 AI 结果写回词典源。
- 分词、模糊搜索、发音服务。
- 跨设备同步。
- Anki 媒体上传增强。

## 2. 参考实现研究

读 `references/ODH`（MIT，commit `ce5e7da`）的 `src/dict/*.js` 与 `doc/development.md`：

- 每个词典脚本如何构造 URL、取响应、把 HTML 映射为结构化条目。
- 脚本失败时如何返回空结果，而不是抛给 UI。
- `src/bg/js/ankiconnect.js` 如何区分超时、HTTP 状态、Anki 错误。

ODH 在浏览器内用 DOM 解析网页；我们的 JSONPath/脚本只接收响应体与查询词，不执行 DOM。只借鉴“字段映射 + 失败降级”的思路，不复制代码或类结构。

选择器采用 JEV-like 本地服务的通用契约，不绑定某个具体模型实现；JEV 类模型的置信度可能过度自信，阈值与 UI 文案必须体现“模型自评”。

## 3. 任务分解

### T4.1 RemoteHttpSource

1. 校验模板恰好一个 `{word}`，scheme 只允许 http/https；鉴权头名/值拒绝 CRLF。
2. URL 编码后发送 GET；`Accept: application/json`、`User-Agent: LexiGrab/<version>`。
3. 连接超时 1.5s，总超时按配置（默认 3000ms，200–15000ms）。
4. 响应体最多 2MB；非 2xx 返回错误；连接失败/超时最多重试 1 次，间隔 150ms；4xx 不重试。
5. 跨主机重定向停止跟随；同主机最多 2 次；鉴权头不随跨主机发送。
6. 解析后填 `source_name`；HTML 走统一清洗；相对资源 URL 丢弃；单条 >512KB 跳过。
7. 最多 20 条安全上限，路由层再按显示上限截断。

**验收**：wiremock 覆盖成功、401/500、超时、跨主机重定向、超大响应、鉴权头断言。

### T4.2 JSONPathParser

- `jsonpath-rust` 表达式编译；`$.entries[*]` 为主。
- 支持 `$` 返回数组或含 `entries` 的对象。
- 缺 `headword` 用查询词；缺 `definition_html` 跳过；资源只收 http/https。
- 空数组视为无结果；语法/JSON/类型错误映射解析失败。
- 最多 20 条。

**验收**：标准/非标准、缺字段、坏 URL、空数组、语法错误、超限全部有用例。

### T4.3 ScriptParser

- 每次查询新建 Runtime；脚本路径相对配置目录，≤64KB，UTF-8；缓存源码与 mtime。
- 内存 16MB、栈 256KB、中断处理器；截止默认 200ms，范围 50–1000ms。
- 关闭 `rust-alloc`，验证 `set_memory_limit` 生效。
- 只暴露 QuickJS 标准对象；无 fetch/require/process/setTimeout。
- 执行脚本，调用全局 `parse({ responseBody, query })`；返回值序列化为 JSON 再 serde 校验。
- 错误分类：Compile/Runtime/Timeout/Memory/Shape；超时或 OOM 丢弃 Runtime。
- UI 必须显示“只运行可信脚本”。

**验收**：正常映射、语法错、运行错、死循环 400ms 内终止、OOM 不崩、错误结构、fetch 未定义、Promise 返回均有测试。

### T4.4 CacheManager

- 按 BACKEND_STRUCTURE §3 建库；WAL、busy_timeout、user_version。
- 查询键 trim+NFKC+小写；正结果 24h，空 5min；错误不缓存。
- 缓存命中在 TTL 内不发网络；`first_hit` 正命中停止，负命中跳过该源继续下一个。
- 源修改/删除失效；过期异步清理；损坏重建。
- 统计与清理由设置页调用。

**验收**：TTL、负缓存、失效、并发、损坏恢复、可注入时钟测试通过。

### T4.5 路由、事件与 QueryMeta

- `local_first`：本地 + 缓存同步返回，未命中远程后台任务；事件追加。
- `first_hit`：按优先级；正缓存命中停止；负缓存继续。
- `aggregate`：本地 + 缓存同步；未命中远程并发；事件追加。
- 远程失败只发 `lookup://remote-error`，不改主结果、不弹模态框。
- `QueryMeta` 最近 8 条，记录缓存源与 pending 数；`get_query_meta` 返回。
- 桥接的 `lookup_collect_all` 扩展远程源，3.5s 截止。

**验收**：三种策略时序、缓存命中/负缓存、过期事件、失败隔离、桥接 partial 均有测试。

### T4.6 远程源与缓存 UI

- manager 添加/编辑远程源：名称、模板、鉴权头/值、超时、JSONPath/脚本、脚本路径、模板生成、测试。
- 测试面板：查询词、Entry 预览、耗时、错误、原始响应 4KB 预览；草稿配置不写盘。
- 状态：就绪/上次失败；无索引概念。
- popup：远程进度条、缓存徽标、错误诊断默认隐藏。
- 设置缓存：TTL、统计、清除、二次确认。

**验收**：JSONPath 与脚本各配一个源成功；失败有可读错误；缓存可清理。

### T4.7 JEV-like 选择器

- 按 BACKEND_STRUCTURE §7.4 调用本地 HTTP 选择器；只允许 http/https。
- 输入 `word`、`sentence`、`senses`；输出 `selected_id`、`confidence`、`model`。
- 校验 `selected_id` 属于候选；非法/缺失按低置信度。
- 阈值状态机：`>= high` 选中；`low..high` 模糊；`<= low` 低置信度；无原句不调用。
- 选择器失败 `unavailable`，不自动调 LLM；UI 保留全部义项。
- 默认仅回环；远程需 `selector_allow_remote` 与二次确认。
- 事件 `lookup://sense-result`；`get_sense_resolution` 兜底。

**验收**：高/中/低/无匹配/非法 id/超时/远程阻止均有测试；推荐不删除其他义项。

### T4.8 LLM 兜底

- 仅低置信度或无候选、且有原句、且用户启用时触发。
- OpenAI-compatible `POST`；`temperature=0`；超时 15s；单次调用。
- 要求 JSON：`definition`、`part_of_speech`、`confidence`、`explanation`；坏 JSON 丢弃。
- 定义纯文本 ≤600 字符；不解析 HTML；`AiDefinition` 标注模型。
- 远程端点需 `allow_remote`；API key 只读环境变量；不写日志。
- 结果写 `sense_cache` 7 天；提示词版本进 key。
- UI 显示“AI 生成 · 模型 · 可能不准确”，用户点“使用此释义”才生效。

**验收**：低置信度生成 AI 卡；超时/坏 JSON/远程阻止有可读状态；原文/原句不写日志；高置信度不调用 LLM。

### T4.9 语境辅助 UI 与设置

- popup 推荐条、置信度、推荐义项高亮、AI 卡、切换义项；按 APP_FLOW §6.6。
- 设置新增“语境辅助”：选择器地址/超时/阈值、LLM 地址/模型/超时/key 环境变量、回环限制、测试按钮、缓存清理。
- 无原句显示“缺少原句”；选择器不可用不阻塞结果。
- 保存使用当前义项；未选择时保存整条 Entry；AI 卡使用后来源写 `AI · model`。
- 扩展通过桥接 `sense_available` + `/v1/sense` 展示，不直连服务。

**验收**：高/中/低三种状态可手工复现；AI 未启用时只显示词典；设置保存后重启仍生效。

### T4.10 测试与性能

- wiremock：标准契约、鉴权、超时、500、重定向、超大响应、编码。
- 选择器 mock：高/中/低/非法 id/超时/远程阻止；LLM mock：正常/坏 JSON/超时/401。
- JSONPath/脚本/缓存/路由用例。
- 手工：mock 服务配置两种解析；配置 JEV-like mock 与本地 LLM；关闭服务看本地结果；断网看缓存。
- 性能：缓存 p95 <5ms；JSONPath 100 条 <10ms；选择器额外 ≤800ms；LLM 不阻塞首屏；远程超时 ≤配置+200ms。
- 写 `.agents/notes/M4-remote-baseline.md` 与 `.agents/notes/M4-sense-baseline.md`。

## 4. 阶段决策

| 决策 | 结论 |
| --- | --- |
| 缓存刷新 | 命中即不请求，TTL 就是刷新周期 |
| 负缓存 | 5min；first_hit 跳过该源但继续下一个 |
| 脚本隔离 | QuickJS 沙箱，非强安全边界；信任警告 |
| 重定向 | 跨主机停止；同主机最多 2 次 |
| TLS | rustls + 系统信任；不提供忽略证书开关 |
| 相对资源 | 丢弃，remote 契约要求绝对 URL |
| 义项选择 | JEV-like 本地 HTTP；置信度是模型自评，阈值可调 |
| LLM 边界 | 只做低置信度兜底，纯文本、人工采用、默认关闭 |
| 隐私 | 选择器默认回环；LLM 默认回环；远程需显式允许 |
| 进程边界 | 不直接启动 JEV CLI；由用户包装成本地 HTTP，避免命令注入 |

## 5. 风险

| 风险 | 对策 |
| --- | --- |
| 内存限制失效 | 关闭 rust-alloc + OOM 测试 |
| 死循环 | 中断处理器 + 墙钟兜底 |
| JSONPath 语义差异 | 只承诺标准写法 + 测试 |
| 响应过大 | 2MB/512KB/20 条上限 |
| SQLite 锁 | WAL + timeout + 单连接 |
| 远程拖慢 first_hit | 优先级、每源超时、总截止 |
| 令牌进日志 | 日志白名单 + 测试断言 |
| 自签名证书 | 文档说明系统信任或 HTTP |
| 选择器过度自信 | 阈值可调；UI 标“模型自评”；低置信度走 LLM |
| 义项切分不准 | M1 提取尽力而为；无法拆分时整条候选；不承诺精确 |
| LLM 幻觉 | 标注 AI、用户确认、不覆盖词典、可编辑 |
| 原句外发 | 默认回环；远程二次确认；日志不记录 |
| sense 事件拖慢 UI | 异步事件；首屏不等；超时静默 |

## 6. 提交拆分

```text
feat(parsers): add jsonpath response parser
feat(sources): add remote http source
feat(parsers): add rquickjs script sandbox
feat(core): add sqlite cache manager
feat(core): implement remote query strategies
feat(manager): add remote source editor and tester
feat(settings): add cache settings
feat(popup): append remote results and cache badges
feat(core): add jev-like sense selector
feat(core): add llm fallback with privacy gate
feat(settings): add sense assist section
feat(popup): show sense recommendation and ai card
test: cover remote jsonpath script cache routing
test: cover sense selector and llm fallback
docs(notes): record m4 sandbox and latency
```

## 7. 出口检查清单

- [ ] 非标准 API 可经 JSONPath 与脚本解析。
- [ ] 死循环 400ms 内终止，OOM 不崩。
- [ ] 远程超时/500 时本地结果正常。
- [ ] local_first/first_hit/aggregate 语义正确。
- [ ] 缓存命中断网可用，标记正确。
- [ ] 远程源可增删改测，缓存可清理。
- [ ] JEV-like 高/中/低置信度状态正确，低置信度触发 LLM。
- [ ] AI 释义标注模型、默认不选中、用户确认后才可保存。
- [ ] 选择器/LLM 失败不影响词典结果。
- [ ] 远程选择器/LLM 在未允许时被阻止，API key 不进日志。
- [ ] 鉴权头不进日志、不跨主机发送。
- [ ] 自动化测试与性能记录完成。

## 8. 交接给 M5

M5 完成远程/缓存/语境辅助设置视觉与文案、许可证清单、README 与脚本文档、性能复测；若沙箱或 AI 隔离不足，文档明确边界。
