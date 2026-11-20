# M4 语境辅助基线（JEV-like 选择器与 LLM 兜底）

最后更新：2026-11-20。对应 [PLAN_M4](../plans/PLAN_M4.md) T4.7–T4.9、T4.10。
契约来源：[BACKEND_STRUCTURE](../../docs/BACKEND_STRUCTURE.md) §2.6、§7.4–§7.7。

## 1. 实现落位

| 能力 | 代码 | 说明 |
| --- | --- | --- |
| 选择器 | `src-tauri/src/core/sense/selector.rs` | `SenseSelector` trait、`HttpSelector`；POST `{word, sentence, locale, senses}`；只接受 200+JSON，不重试 |
| LLM | `src-tauri/src/core/sense/llm.rs` | `LlmGenerator` trait、`HttpLlm`；OpenAI-compatible，`temperature=0`、单次调用、`PROMPT_VERSION="v1"` |
| 状态机 | `src-tauri/src/core/sense/resolution.rs` | 候选提取、阈值分档、LLM 触发、`sense_cache` 读写、错误分类 |
| 命令 | `commands.rs` | `test_sense_selector`、`test_llm`、`get_sense_resolution`、`clear_sense_cache` |
| 触发 | `commands.rs::lookup`、`bridge/mod.rs::selection` | 有原句且已启用时后台解析；首屏不等待；结果写 8 条环形缓存并广播 `lookup://sense-result` |
| 弹窗 | `src/windows/popup/SenseAssistant.tsx`、`state.ts` | 推荐条、置信度进度条（“模型自评”）、义项切换、AI 卡、保存来源 |
| 设置 | `src/windows/settings/SenseSection.tsx` | 选择器/LLM 配置、回环与远程门禁、测试按钮、语境缓存清理 |
| 扩展 | `extension/entrypoints/content/Bubble.tsx`（M3 已实现） | 经桥接 `sense_available` + `/v1/sense` 轮询展示，不直连服务；M4 未改端点 |

## 2. 状态与阈值

`disabled → no_context → selected / ambiguous / low_confidence / unavailable / failed`：

- 高置信度（`>= high_confidence`，默认 0.72）：`selected`，推荐义项并默认选中，保存默认使用；
- `low < confidence < high`：`ambiguous`，高亮 + “建议确认”，不自动改用户选择；
- `<= low`（默认 0.55）或返回的 `selected_id` 不属于候选：`low_confidence`，触发 LLM（若启用且失败/未启用则保留全部义项）；
- 选择器不可达/超时/非 200：`unavailable`，**不自动调用 LLM**，用户可手动点“使用此释义”；
- 无原句：`no_context`，不调用选择器；
- 置信度是模型自评；UI 用文字 + 进度条显示，不只靠颜色。

其他规则：

- `SenseResolution` 始终带 `candidates` 快照；推荐不删除其他义项，用户手动选择后不被后到事件覆盖。
- AI 释义为纯文本（拒绝 HTML、≤600 字符），标注模型与“可能不准确”，默认不采用；用户点“使用此释义”后才进入编辑区。
- AI 卡保存：`definition_html` 用 `<div>` 包裹转义文本，`Source` 写 `AI · <model>`，追加标签 `ai-generated`（`AnkiPayload.tags`）。
- 默认只允许回环选择器/LLM；非回环需 `selector_allow_remote` / `allow_remote` 且在 UI 二次确认；未允许时直接返回 `SENSE_SELECTOR_FAILED` / `LLM_REMOTE_BLOCKED`，**不发请求**。
- API key 只读 `api_key_env` 环境变量，不写配置、不写日志；日志不记录单词与原句。

## 3. 缓存

- 仅成功结果（`selected`/`ambiguous`/`low_confidence`）写入 `sense_cache`，失败与未启用状态不缓存。
- key = uuid v5（word | sentence | 候选 id+文本 | model | prompt version）；TTL 取 `llm.cache_ttl_hours`（默认 168h）。
- 命中后保留当前候选，替换缓存的 status/selected/ai/query_id/headword，避免缓存与当前查询错配。

## 4. 自动化验证

`src-tauri/tests/sense_assist_tests.rs`：**32 通过**。

- selector：请求体契约、高/中/低置信度、非法 id 透传、超时、非 200、远程端点阻止且 mock 收到 **0 请求**。
- LLM：OpenAI 请求形状（model / temperature=0 / max_tokens / messages）、正常 JSON、坏 JSON、超时、401、含 HTML 的定义拒绝、远程阻止零请求、无 key 时不带 Authorization。
- 状态机：高置信度不调 LLM；低置信度生成 AI；`unavailable` 不自动兜底；无原句不调用；无候选走 `low_confidence`；缓存命中不重复请求；key 随 model 变化；TTL 过期重查；失败/无上下文不写缓存。

前端：`pnpm test` 121 通过（popup state 43、PopupApp 13 等），覆盖远程追加/去重/错误隔离、高/中/低置信度、用户选择不被覆盖、AI 默认不选中与保存 `source/tags/转义`、无原句不请求语境。

## 5. 手工/联调记录

- Mock 端到端（`.agents/automation/m4-mock/run.py`）验证远程链路；选择器/LLM 的 mock 契约由 wiremock 测试覆盖，未启动真实 JEV 或本地 LLM（本机无对应服务）。
- 弹窗手工验证建议（M5 视觉阶段复跑）：启动应用 → 设置启用语境辅助（选择器指向 mock `/v1/select`，LLM 指向 mock `/v1/chat/completions`）→ 在文章中划词并带原句 → 高/中/低三种置信度分别检查推荐条与主按钮文案 → 关闭 mock 确认“语境模型不可用”不影响词典结果 → 点“使用此释义”确认 AI 卡来源与标签。
- 已知限制：弹窗的“推荐”是条目级徽标 + 义项切换列表（不对任意 HTML 做子串高亮，避免破坏标记）；扩展端展示语境建议但不可采用 AI 释义（M5 视觉/文案阶段评估）。
- 已知限制：迟到的原句补写不会重新触发一次独立语境请求；当前流程要求划词时已捕获原句。
