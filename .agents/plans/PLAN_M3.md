---
phase: M3
title: Firefox 扩展与本地桥接
depends_on: [M2]
exit_gate: Firefox 选中单词后扩展捕获词、原句、页面信息，收到桌面端结果并成功制卡；.xpi 可安装且重启后仍在
estimate: 10-14 人日
status: done
---

# M3 Firefox 扩展与本地桥接

M3 交付三件事：只监听回环地址的 HTTP 桥接、WXT/Firefox 扩展的划词与原句提取、可安装的 `.xpi`。对应 PROMPT 的 M3a、M3b、M3c。

## 全局依据

- 桥接端点、鉴权、CORS、限制、错误码：[BACKEND_STRUCTURE](../../docs/BACKEND_STRUCTURE.md) §5
- 扩展页面与流程：[APP_FLOW](../../docs/APP_FLOW.md) §8
- 设计主参考 [figma](../design/figma.md)，实现规则 [FRONTEND_GUIDELINES](../../docs/FRONTEND_GUIDELINES.md)
- 精确版本：[TECH_STACK](../../docs/TECH_STACK.md)
- 会话断点：[PROGRESS](PROGRESS.md)

桥接的请求/响应与安全规则不在本计划复制。

## 1. 范围

**做**

- 桥接：`127.0.0.1:12787`，令牌、Origin、CORS、限流、请求体上限。
- `GET /v1/health`、`POST /v1/selection`、`GET /v1/sense/{query_id}`、`POST /v1/anki`。
- 桌面端“浏览器扩展”设置分区：启用、端口、令牌、状态、配对步骤。
- `lookup_collect_all` 的本地版本（3.5s 截止）。
- WXT 扩展工程、manifest、background、content、options。
- 选区捕获、句子提取、页面元数据、浮动按钮、结果气泡、保存。
- 语境辅助展示：轮询 `sense_available`，显示推荐义项/低置信度 AI 释义（M4 提供数据）。
- 同词多卡：扩展连续保存不因首字段相同被阻断；同语境显示提示。
- `.xpi` 构建、安装、重启持久化验证。
- 桥接与扩展自动化测试、手工端到端。

**不做**

- Chrome/Edge/Safari 发布。
- Native Messaging。
- 扩展内自定义 Anki 字段。
- 把页面 URL 写入 Anki（四字段无 URL）。
- iframe 内选区、句子编辑、历史记录。
- AMO 发布与自动签名；M3c 只写限制与可选路径。

## 2. 目录

```text
src-tauri/src/bridge/{mod,server,auth,dto,error}.rs
src-tauri/src/platform/...（不新增桥接逻辑）
extension/{package.json,tsconfig.json,wxt.config.ts,vitest.config.ts}
extension/entrypoints/{background.ts,content/{index.tsx,Bubble.tsx,style.css},options/{index.html,main.tsx,App.tsx}}
extension/lib/{bridge,messages,sentence,settings,dom}.ts
extension/tests/{sentence,bridge,bubble}.test.*
pnpm-workspace.yaml（根目录，包含 extension）
```

## 3. 参考实现研究

M3 开始前读 `references/ODH`（MIT，commit `ce5e7da`）：

- `src/fg/js/text.js`：块级祖先、Range 偏移、句子切分与回退。
- `src/fg/js/range.js`：`caretRangeFromPoint`、单词边界。
- `src/fg/js/frontend.js`：选区事件与结果浮层生命周期。
- `src/bg/js/ankiconnect.js`：连接探测、超时、错误分类。

只提炼算法与失败处理，不复制代码、类结构、HTML/CSS。我们的实现使用 TypeScript、Shadow DOM、WXT、`Intl.Segmenter` 回退与本地桥接，形态不同。研究结论写入 `PROGRESS.md` 或本计划备注。

## 4. 任务分解

### T3.1 桥接服务

1. 按 BACKEND_STRUCTURE §5 实现路由、DTO、错误映射。
2. 只绑定 `127.0.0.1`；启动失败（端口占用）状态 failed，不崩溃、不静默换端口。
3. 令牌来自 `[bridge].token`；常量时间比较；轮换后旧令牌立即失效。
4. CORS 头加到所有响应，包括 401/403/413/429；预检不参与限流。
5. 请求体 1MB、20 次/秒、突发 40、处理超时 10s。
6. 实现 `lookup_collect_all` 本地版本：并发本地源，3.5s 截止，返回 `partial`。
7. 实现 `GET /v1/sense/{query_id}`：已完成 200，进行中 202，未知 404；不阻塞 selection 首屏。
8. 生命周期：随配置启动/停止；状态事件；`get_bridge_status`、`regenerate_bridge_token`。
9. 记录最近成功请求时间，用于设置页“已连接”。

**验收**：无令牌 401；非 moz-extension Origin 403；CORS 预检正确；限流 429；成功查询与制卡；桥接测试不启真实端口时用 oneshot。

### T3.2 WXT 工程与 workspace

- 根 `pnpm-workspace.yaml` 包含 `extension`；根锁文件唯一。
- 依赖按 TECH_STACK；`web-ext` 为 devDependency。
- manifest：`storage`、`http://127.0.0.1/*`、gecko id `lexigrab@localhost`、`strict_min_version 109.0`、content script `<all_urls>`。
- WXT 默认 MV2；构建后检查产物 manifest 的权限与 id。
- content 入口 `cssInjectionMode: "ui"`，Tailwind 注入 Shadow Root。
- 不用 `tabs`/`history`/`bookmarks` 权限。

**验收**：`pnpm -C extension build:firefox` 成功；产物 manifest 含正确 id/权限；`web-ext lint` 无阻断错误。

### T3.3 选区与句子提取

实现 `lib/sentence.ts` 纯函数，基于 `window.getSelection()`：

1. 只处理非折叠、1–64 字符、含字母/CJK、无换行、非密码输入。
2. 从 `startContainer` 上游找块级祖先：p/li/td/th/blockquote/dd/dt/figcaption/article/section/main/div/h1–h6/pre；到 body 停止。
3. 用 Range 从块首到选区起点算原始偏移；遍历可见文本节点，折叠空白并记录偏移映射。
4. 优先 `Intl.Segmenter` sentence；不可用时用 `[.!?。！？]` 与换行回退；按偏移取句。
5. 取不到时退化为包含选词且距偏移最近的片段；结果 ≤600 字符。
6. 块文本 ≤300 字符时直接用整块。
7. 快照在 mouseup/keyup 后立即生成，包含词、句、URL、标题、域名；按钮点击只发送快照。
8. iframe 内不支持，记录为已知限制。

**验收**：普通段落、嵌套 span、链接、中文标点、英文缩写、长段落、跨节点、无标点、隐藏元素用例通过；至少 90% 固定网页样本包含选词。

### T3.4 气泡与查询/保存

- Shadow DOM 挂载 React；宽度 360，最大高度 480，空间不足翻到上方。
- 浮动按钮 28×28，出现在选区附近；滚动/点击外部/Escape 隐藏。
- content → background 消息；background fetch 桥接，AbortController 5s。
- 错误映射：`NOT_CONFIGURED`、`DESKTOP_OFFLINE`、`AUTH_FAILED`、`ORIGIN_REJECTED`、`TIMEOUT`、`SERVER_ERROR`、`INVALID_RESPONSE`。
- 结果最多 3 条，首条展开；DOMPurify 二次清洗；来源显示域名。
- 若 `sense_available`，轮询 `/v1/sense/{query_id}`（1s 间隔、最多 5 次）：显示推荐义项、置信度或 AI 卡；失败不阻塞释义。
- 保存：`POST /v1/anki`，成功显示“已保存”与“再存一张”；**不因同词阻断**，仅当 `exact_context_seen` 显示提示。
- 不把令牌、完整响应体写入页面或 console。

**验收**：Firefox 中查询/保存成功；401 与离线有不同提示；同词连续保存不阻断；sense 轮询在 disabled/pending 时不破坏气泡。

### T3.5 选项页与配对

- `browser.storage.local` 存端口与令牌，不用 sync。
- 安装后打开 options；端口 12787、令牌、测试连接、保存。
- 测试连接调用 `/v1/health`，区分 401/连接失败/超时。
- 令牌密码框 + 显示切换；不在页面打印。
- 桌面端设置提供令牌复制、轮换、状态、配对三步。

**验收**：配对成功；令牌轮换后旧令牌 401；端口错误提示可操作。

### T3.6 `.xpi` 打包与安装

```powershell
pnpm install
pnpm -C extension build:firefox
pnpm -C extension lint:firefox
pnpm -C extension package:firefox
```

- `web-ext build` 产物重命名 `lexigrab-firefox-<version>.xpi`；不提交仓库；生成 sha256。
- Developer Edition/Nightly：`about:addons` 从文件安装，重启后仍在。
- 正式版：`about:debugging` 临时加载，重启失效。
- 记录未签名 `.xpi` 在正式版的失败文案；文档写明目标浏览器版本。

**验收**：Dev/Nightly 安装成功、ID 为 `lexigrab@localhost`、重启保留设置；正式版临时加载可用。

### T3.7 测试

- Rust：oneshot 测鉴权、Origin、CORS、参数、413、429、成功/部分结果/同词多卡/Anki 不可用；`/v1/sense` 的 200/202/404；真实端口测试绑定回环。
- 扩展：句子提取 happy-dom 用例、bridge 错误映射、气泡状态、sense 轮询与 AI 展示。
- 手工：Wikipedia、新闻、中文页面各 3 次；令牌轮换、端口冲突、桌面端关闭、Anki 关闭、同词多句保存。
- 结果写 `.agents/notes/M3-extension-verification.md`。

## 5. 阶段决策

| 决策 | 结论 |
| --- | --- |
| 分发 | 只做 Firefox；未签名 `.xpi` |
| 桥接端口 | 默认 12787，失败不自动换端口 |
| Origin | 只允许 `moz-extension://` 前缀；缺失 Origin 仍需令牌 |
| 扩展 UI | Shadow DOM + Tailwind；不用页面 CSS |
| 句子 | Intl.Segmenter 优先，标点回退；不追求完美 |
| 语境辅助 | 扩展只展示桌面端结果；不在扩展内直连选择器/LLM |
| 多卡片 | 扩展保存不因同词阻断；同语境提示一次 |
| URL | 只展示，不写入 Anki |

## 6. 风险

| 风险 | 对策 |
| --- | --- |
| Firefox CORS/host permission 配置错误 | 构建后检查 manifest；真机验证 |
| 127.0.0.1 与 localhost 不一致 | 全部统一 127.0.0.1 |
| 端口占用 | 设置页报错，不静默换端口 |
| Origin 误伤 | 只查前缀，测试覆盖 |
| 令牌泄漏 | storage.local、不写日志、限流 |
| Shadow DOM 样式受页面影响 | WXT Shadow Root UI + 编译 CSS |
| 句子偏差 | 纯函数测试 + 站点矩阵 |
| 未签名安装失败 | 文档给 Dev/Nightly 与临时加载 |
| 扩展 ID 变更丢设置 | ID 固定，发布前不换 |
| sense 轮询拖慢气泡 | 1s 间隔、最多 5 次；失败静默；首屏不等 |
| 同词多卡被误判 | 桥接返回 exact_context_seen，仅提示 |

## 7. 提交拆分

```text
feat(bridge): add loopback http server with token auth
feat(bridge): add selection and anki endpoints
feat(settings): add browser extension bridge section
test(bridge): cover auth cors limits and endpoints
feat(extension): scaffold wxt firefox mv2 project
feat(extension): capture selection and extract sentence
feat(extension): add result bubble and anki save
feat(extension): show sense hint and multi-card save
feat(extension): add options page and token pairing
build(extension): package unsigned xpi with web-ext
docs(notes): record firefox install and e2e results
```

## 8. 出口检查清单

- [ ] 桥接只监听 `127.0.0.1:12787`，无令牌 401。
- [ ] Origin 只允许 `moz-extension://`；CORS 错误响应可读。
- [ ] 设置页可查看、复制、轮换令牌，端口修改生效。
- [ ] 扩展配对成功，测试连接可用。
- [ ] 词、原句、标题、URL、域名正确。
- [ ] 查询/保存成功；401/离线/超时有区分。
- [ ] `/v1/sense/{query_id}` 在未启用/进行中返回 disabled/pending，M4 接入后返回推荐/AI；扩展不阻塞首屏。
- [ ] 同一单词连续保存多张卡不被阻断；同语境只提示。
- [ ] `.xpi` 在 Dev/Nightly 安装并重启保留。
- [ ] 正式版临时加载路径与限制已记录。
- [ ] 自动化测试与手工矩阵通过，`cargo test`、typecheck、lint 全绿。
- [ ] 扩展不直接访问 AnkiConnect 或远程词典。

## 9. 交接给 M4/M5

M4 复用 `lookup_collect_all`、`/v1/sense` 与桥接协议；启用选择器/LLM 后扩展无需改端点。M5 完成扩展设置文案、`.xpi` 文档、令牌存储安全说明。
