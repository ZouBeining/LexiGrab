# PROMPT

本文件是 agent 接手 LexiGrab 的实现入口。层级关系：[AGENTS.md](AGENTS.md) 是总纲与硬约束；本文件是代码地图、实现路径、任务配方和验证方式；[docs/](docs/) 是产品与接口契约，[.agents/plans/](.agents/plans/) 是阶段计划与断点。

阅读顺序固定：AGENTS → 本文件 → PROGRESS → 当前 PLAN → 与任务相关的 docs 小节。不要从第一页开始通读整个仓库。

## 0. 接手清单

1. 读 [PROGRESS.md](.agents/plans/PROGRESS.md)，确认已完成、进行中与已知问题。
2. 读当前阶段 [PLAN_Mx](.agents/plans/)，只执行其中未完成的出口项。
3. 用搜索工具定位任务涉及的命令、类型、事件与测试，再定点读文件；先读测试再读实现。
4. 找到对应契约：数据/IPC/桥接看 [BACKEND_STRUCTURE](docs/BACKEND_STRUCTURE.md)，页面与状态看 [APP_FLOW](docs/APP_FLOW.md)，视觉看 [FRONTEND_GUIDELINES](docs/FRONTEND_GUIDELINES.md)。
5. 改完实现后同步测试、docs 与 PROGRESS；文档改动单独提交。

当用户没有明确要求时，不要开始 M5，不要补写根目录 README，不要扩展 macOS/Linux 或 Chrome/Edge。

## 1. 事实来源与冲突处理

| 问题 | 权威来源 |
| --- | --- |
| 实际行为、实际版本 | 代码、`src-tauri/Cargo.toml`、`package.json`、`extension/package.json` 与锁文件 |
| IPC、数据模型、存储、桥接、错误码 | [docs/BACKEND_STRUCTURE.md](docs/BACKEND_STRUCTURE.md) |
| 页面路径、弹窗状态、交互文案 | [docs/APP_FLOW.md](docs/APP_FLOW.md) |
| 产品范围与成功标准 | [docs/PRD.md](docs/PRD.md) |
| 组件位置与阶段状态 | [ROADMAP.md](ROADMAP.md)、[PROGRESS.md](.agents/plans/PROGRESS.md) |
| 实现路径与陷阱 | 本文件 |

本文件不是契约。发现代码与 docs 不一致时：先判断是文档过期还是实现偏差；改契约先改 docs，再改代码与测试，并在 PROGRESS 记录。不要把 docs 的字段表复制进本文件或计划。

## 2. 当前范围与状态

- 版本 0.1.0，平台只承诺 Windows 10/11；浏览器只验收 Firefox 109+。
- M0–M4 主体已完成：桌面端、本地/远程词典、缓存、语境辅助、Anki、桥接与扩展；M0 的手工应用矩阵仍可补测。
- M5 尚未开始：视觉与文案收尾、许可证清单、总 README、发布流程、长跑与安全审查都还未做。
- 根目录 `README.md` 目前是占位文件，这是刻意状态，不要擅自补写。
- CI 覆盖前端检查、Rust 检查与根目录测试；扩展测试目前只在本地跑，发布前要补进 CI 或留下手工证据。

完成定义：功能有实现、有测试、有错误路径、docs 已同步、PROGRESS 已更新，且下面的门禁全部通过。

## 3. 五分钟接手

环境：Windows、Node 24、pnpm 11.28.5、Rust 1.99.1、WebView2。依赖以锁文件为准。

```text
pnpm install
pnpm tauri dev                # 桌面端开发
pnpm dev                      # 只起 Vite 前端
pnpm typecheck && pnpm lint && pnpm test
pnpm -C extension dev         # Firefox 扩展开发
pnpm -C extension typecheck && pnpm -C extension test
pnpm -C extension build:firefox
pnpm -C extension lint:firefox
pnpm -C extension package:firefox
cd src-tauri && cargo test
cd src-tauri && cargo run --bin lexigrab-dev -- lookup --config <config.toml> --word bank --json
```

文件位置：

- 配置默认在 `%APPDATA%/LexiGrab/config.toml`；`LEXIGRAB_HOME` 可把配置、日志、缓存整体移到便携目录，`LEXIGRAB_CONFIG_DIR` 只覆盖配置目录。
- 日志默认在 `%LOCALAPPDATA%/LexiGrab/logs`，SQLite 在 `%LOCALAPPDATA%/LexiGrab/cache.db`。
- 配置 schema 当前为 v2；加载旧配置时迁移，损坏时备份并以默认值启动。缓存库 `user_version` 也为 2。

启动顺序：解析路径 → 初始化日志 → `AppState::new` → 单实例/热键/对话框插件 → 注册 `mdx-res` 协议与命令 → 创建托盘 → 显示管理器并预创建 popup/settings → 注册热键 → 后台加载词典与探测 Anki → 启动桥接。

三个窗口标签固定为 `manager`、`popup`、`settings`；popup 与 settings 启动时预创建并隐藏，不要改成懒创建。

## 4. 代码地图

| 区域 | 文件 | 关键符号与职责 |
| --- | --- | --- |
| 启动与生命周期 | `src-tauri/src/lib.rs`、`main.rs` | `run`、命令注册、`mdx-res` 处理、托盘、单实例、退出清理 |
| 命令层 | `src-tauri/src/commands.rs` | 33 个 `#[tauri::command]`；只做参数整理、`spawn_blocking` 与错误转换 |
| 状态中心 | `core/state.rs` | `AppState`、`lookup_planned`、`run_remote_job`、`lookup_collect_all`、`run_sense_resolution`、`QueryMeta` |
| 配置 | `core/config.rs` | `Paths::resolve`、`ConfigStore`、`Settings`、schema v2 迁移与校验 |
| 查询调度 | `core/query_router.rs` | `QueryRouter::{lookup, lookup_planned, run_remote_job}`、三种策略、失败隔离 |
| 源注册表 | `core/registry.rs` | `SourceRegistry`、`build_source`、按 priority/id 排序、重载与坏源隔离 |
| 数据源 | `sources/local_file.rs`、`remote_http.rs` | `DictionarySource` 实现；本地多文件合并；远程模板/鉴权/重试/大小上限 |
| MDX 解析 | `parsers/mdx.rs` | `FormatParser` 实现；`mdictlib` 适配、每文件 worker、归一化变体 |
| 响应解析 | `parsers/response.rs`、`jsonpath.rs`、`script.rs` | `ResponseParser`、`finalize_entries`、JSONPath、QuickJS 沙箱 |
| 义项提取 | `parsers/sense.rs` | 从清洗后的 HTML 提取 `SenseCandidate` |
| HTML 与资源 | `core/html.rs`、`core/resources.rs` | 资源路径改写、ammonia 白名单、`mdx-res` 解析与 MIME/路径校验 |
| 缓存与指纹 | `db/mod.rs`、`db/cache.rs`、`db/fingerprint.rs` | `saved_cards`、`lookup_cache`、`sense_cache`、NFKC 指纹、WAL |
| 语境辅助 | `core/sense/{selector,llm,resolution}.rs` | 选择器 HTTP、LLM 兜底、阈值状态机、`SenseResolution` |
| Anki | `anki/{client,mapping,export}.rs` | AnkiConnect 客户端、四字段映射、媒体剥离、同语境判定 |
| 桥接 | `bridge/{mod,server,auth,dto,error}.rs` | `BridgeService`、axum 路由、令牌/Origin/限流、DTO 与错误映射 |
| 平台捕获 | `platform/selection.rs`、`platform/windows/{capture,uia,popup}.rs` | 选区捕获、UIA 段落、光标锚点、非激活显示 |
| 窗口与热键 | `core/window.rs`、`core/shortcut.rs` | 标签常量、`show_popup`、`hide_popup`、热键注册/注销 |
| 前端共享层 | `src/lib/{types,ipc,events,theme,resource,format}.ts` | 类型、命令封装、事件常量、主题与资源 URL 处理 |
| 弹窗 | `src/windows/popup/` | `state.ts` 纯状态机；`PopupApp` 执行副作用；结果、编辑、语境、AI 卡 |
| 管理器 | `src/windows/manager/` | 源列表、源对话框、查询测试、首次引导、状态视图 |
| 设置 | `src/windows/settings/` | 通用、Anki、扩展、缓存、语境、关于六个分区 |
| 扩展 | `extension/entrypoints/`、`extension/lib/` | background 持令牌、content 划词气泡、options 配对；句子与桥接客户端 |
| 开发 CLI | `src-tauri/src/bin/lexigrab-dev.rs` | `lookup`、`sources`、`anki-decks`、`anki-models`、`add-note`、`bench` |
| 测试 | `src-tauri/tests/`、`extension/tests/`、`src/**/*.test.*` | 集成测试、MDX 夹具、扩展纯函数与组件测试 |

## 5. 关键链路

### 5.1 桌面查词

1. 热键触发 `core/shortcut.rs`，捕获并写入 `AppState`，显示 popup，发送 `popup://selection`。
2. 前端生成 `query_id`，调用 `lookup(query, source_ids?, query_id?, sentence?)`；命令在 `spawn_blocking` 中调用 `AppState::lookup_planned`。
3. 同步返回本地与缓存条目；远程任务列表逐个交给标准线程执行，结果发 `lookup://remote-result`，失败发 `lookup://remote-error`。
4. 前端再取 `get_query_meta(query_id)`，得到 `pending_remote` 与 `cached_sources`，用于进度条和缓存徽标。
5. 有原句且启用语境辅助时，命令后台启动 `run_sense_resolution`，结果发 `lookup://sense-result`，并写入 `SenseResolution` 供扩展轮询。

### 5.2 查询策略

- `local_first`：先返回本地与缓存；远程一律后台，首屏不等待。
- `first_hit`：按优先级同步走到首个命中；远程参与同步路径，可能增加首屏延迟。
- `aggregate`：本地同步、远程后台聚合，结果按源与词去重后追加。
- 缓存只覆盖远程源；键复用 NFKC + 小写归一化；负缓存只缓存“未命中”，失败不缓存。
- 远程失败不得变成阻断错误；弹窗只折叠提示，主结果保留。

### 5.3 制卡

1. 前端 `buildAnkiPayload` 只组装词、释义 HTML、原句、来源与可选标签；牌组、模板、字段映射来自设置。
2. 保存前调用 `check_card_context`；命中“同词 + 规范原句 + 规范释义”时提示，用户可继续。
3. `export_to_anki` 恒发 `allowDuplicate: true`；Anki 名、字段与模板校验在 `anki/mapping.rs`。
4. 本地媒体在导出前剥离；AI 释义来源写 `AI · <model>`，标签加 `ai-generated`，只由前端在用户采用后设置。

### 5.4 扩展与桥接

1. content script 捕获词、句子、URL、标题、域名，渲染 Shadow DOM 气泡。
2. 所有网络请求经 `browser.runtime.sendMessage` 交给 background；令牌只在 background 内存中按需从扩展存储读取。
3. background 调 `/v1/selection`，拿 `query_id` 与首屏结果；需要时轮询 `/v1/sense/{query_id}`；保存调 `/v1/anki`。
4. 桥接由 `BridgeService` 随配置启停；端口占用只记 `failed`，不自动换端口。
5. 桥接的错误码按白名单透出，未知码回落 `SERVER_ERROR`；扩展保存失败要显示标题、建议动作和错误码。

## 6. 契约与不变式

### 6.1 IPC

命令注册在 `lib.rs` 的 `invoke_handler`。前端封装在 `src/lib/ipc.ts`，类型在 `src/lib/types.ts`。分组如下：

- 窗口与捕获：`show_window`、`open_window`、`hide_popup`、`set_popup_interactive`、`get_current_selection`、`debug_trigger_capture`、`get_capture_stats`、`get_last_capture`、`get_runtime_status`。
- 查询：`lookup`、`get_query_meta`、`get_sense_resolution`。
- 源：`list_sources`、`get_source_statuses`、`add_source`、`update_source`、`remove_source`、`test_source_query`。
- Anki：`export_to_anki`、`check_card_context`、`get_anki_decks`、`get_anki_models`、`probe_anki`。
- 设置与桥接：`get_settings`、`update_settings`、`open_config_dir`、`get_bridge_status`、`regenerate_bridge_token`。
- 缓存与语境测试：`get_cache_stats`、`clear_lookup_cache`、`clear_sense_cache`、`test_sense_selector`、`test_llm`。

新增命令必须同步五处：`BACKEND_STRUCTURE` 命令表、`commands.rs`、`lib.rs` 注册、`src/lib/{ipc,types}.ts`、调用方测试。命令里不得直接做重活；本地查询、Anki、远程 HTTP 与 SQLite 都走阻塞边界。

### 6.2 事件

事件名只在 `src/lib/events.ts` 定义，组件不得手写字符串：

```text
popup://selection          SelectionContext
popup://capture-error      CaptureError
popup://edit-ready         无载荷
dict://index-progress      { source_id, path, phase, entry_count?, message? }
runtime://hotkey-error     { message }
runtime://anki-status      AnkiStatus
bridge://status            BridgeStatus
lookup://remote-result     { query_id, source_id, source_name, entries, cached, elapsed_ms }
lookup://remote-error      { query_id, source_id, code, message }
lookup://sense-result      SenseResolution
```

### 6.3 弹窗状态机

`src/windows/popup/state.ts` 是纯 reducer，副作用只在 `PopupApp`。稳定内容态：`results`、`empty`、`queryError`、`captureError`；瞬时态：`capturing`、`loading`、`editing`、`saving`、`saved`、`saveError`。

不变式：

- 所有异步结果必须匹配当前 `queryId`，迟到结果丢弃。
- 远程结果可早于 `lookup` promise 到达；合并时按 `(source_name, headword)` 去重，不能覆盖已有条目。
- 用户手动选义项或条目后 `senseChoiceMade = true`，迟到的推荐不得覆盖。
- 高置信度才自动选义项；中/低置信度保留全部义项，主按钮按当前选择切换文案。
- AI 文本必须先转义再包进单个 `<div>`，采用后仍进入编辑态可改。

### 6.4 桥接安全

- 只绑 `127.0.0.1`；端口、开关随配置应用，失败只更新状态。
- 校验顺序：预检放行 → Origin（存在时必须以 `moz-extension://` 开头）→ 令牌桶（20/秒，突发 40）→ `X-Auth-Token` 常量时间比较。
- 所有响应（含 401/403/413/429）都带 CORS；请求体上限 1MB；处理超时 10s。
- 令牌轮换即时生效，无需重启；旧令牌立即 401。
- 日志不得出现令牌、完整选中文本或完整 URL。

### 6.5 存储

`cache.db` 使用 WAL、`busy_timeout`、`user_version = 2`，三张表：`lookup_cache`、`sense_cache`、`saved_cards`。`clear_lookup_cache` 不清 `saved_cards` 与 `sense_cache`。指纹为词 + 规范化原句 + 规范化释义，规范化含 NFKC、小写与空白压缩。

### 6.6 资源协议

逻辑 URI 为 `mdx-res://{dict_id}/{path}`；Windows 渲染为 `http://mdx-res.localhost/...`，处理器同时接受两种形态，并跳过 `localhost` authority。只允许 GET/HEAD、白名单 MIME、防路径穿越；前端不要在 CSP 外新增资源域。

### 6.7 语境辅助

- 选择器与 LLM 都是可选增强；默认选择器回环、LLM 关闭，远程端点需显式允许。
- 失败不自动降级到 LLM；只有低置信度或词典外含义才考虑 LLM。
- 成功状态才写 `sense_cache`；key 包含 model 与 prompt version，失败不缓存成空结果。
- AI 输出不得写回 `Entry`，不得冒充词典源；卡片必须可追溯到模型。

## 7. 扩展实现要点

- `background.ts` 是唯一持有桥接令牌的内存路径；content 只发四类消息：health、selection、anki、sense。
- `lib/sentence.ts` 是纯函数：块级祖先、隐藏元素与密码框过滤、`Intl.Segmenter` 优先、标点回退、整块 ≤300 字符直用、句子上限 600 字符。
- 气泡渲染在 Shadow DOM，释义再用 DOMPurify 清洗；Tailwind 在浮层作用域补齐 `--tw-border-style` 与阴影变量，规避 Firefox 的 `@property` 限制。
- options 页负责端口、令牌与连接测试；配对信息存 `storage.local`，不要在页面上下文暴露令牌。
- 扩展不直接访问 AnkiConnect、远程词典或 `127.0.0.1` 以外的宿主权限。

## 8. 任务配方

**新增 IPC 命令**：docs 命令表 → 类型 → `commands.rs` 薄封装（阻塞边界）→ `lib.rs` 注册 → `src/lib/ipc.ts` + `types.ts` → 组件/测试。

**新增词典格式**：docs 解析器契约 → 实现 `FormatParser` → `sources/local_file.rs` 接入 → 夹具与集成测试 → 管理器文案。不要改查询调度或 UI 结构。

**新增远程解析模式**：docs 响应契约 → 实现 `ResponseParser` → `finalize_entries` 复用清洗/截断 → mock 服务测试 → 管理器编辑面板。

**改 UI 状态**：先读 APP_FLOW 对应小节 → 改 `state.ts` 的 action/reducer → 改组件 → 补 reducer/组件测试 → 检查 loading/empty/error 与键盘可达。

**改扩展**：先改 `lib` 纯函数与测试 → 再接 entrypoint → 跑 typecheck/test/build → 用 Firefox 手工验证划词、配对与保存。

**升级依赖**：单独提交；核对许可证兼容 AGPL-3.0；同步 TECH_STACK；Rust 跑 clippy/fmt/test，前端与扩展跑 typecheck/lint/test。

**改文档**：先判断读者是 human 还是 agent；human 文档不写实现常量，agent 文档写准确路径与不变量；改完检查行预算、相对链接与过时版本。

## 9. 验证矩阵

| 改动 | 最低验证 |
| --- | --- |
| 任意 Rust | `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` |
| 查询/源/解析 | 另跑 `router_tests`、`remote_tests`、`parsers_tests`、`mdx_tests` |
| 缓存/指纹 | 另跑 `cache_tests`、`card_tests` |
| 桥接 | 另跑 `bridge_tests`；必要时用 curl 手测 health/selection |
| 语境 | 另跑 `sense_tests`、`sense_assist_tests` |
| 前端 | `pnpm typecheck && pnpm lint && pnpm test`；三窗口至少手工过 loading/empty/error |
| 扩展 | `pnpm -C extension typecheck && pnpm -C extension test`，再 `build:firefox` + `lint:firefox` |
| 发布前 | CI 全绿；扩展测试入 CI；`package:firefox` 产物可安装 |

验收资产：`.agents/automation/m2-manual/`（pywinauto 桌面矩阵）、`m3-firefox/`（Marionette 真机）、`m4-mock/`（远程 mock）。它们需要真实桌面/Anki/Firefox，按脚本 README 运行，产物在各自 `artifacts/`，不要提交。

## 10. 已知陷阱

- 计划中的 libmdx 不可达，MDX 实际用 `mdictlib 0.2.8`；不要按旧计划改回。
- 本机仓库带低完整性标签时，从仓库内启动的 exe 无法 `SendInput` 或写 `%APPDATA%`；自动化把 exe 复制到 `%TEMP%` 运行。
- MSVC 编译 bundled SQLite 与 aws-lc 会报 D8050；因此 HTTP 用 native-tls，`[profile.dev] debug = 0`，不要随手删。
- Git/cargo 走 Schannel 可能报 `SEC_E_NO_CREDENTIALS`；git 可加 `-c http.sslBackend=openssl`，cargo 需要可联网的普通终端先 fetch。
- pnpm 11 在 Windows 默认 junction 布局会让 ESLint 解析失败；仓库用 `nodeLinker: hoisted`。
- Wry 会把 `http://mdx-res.localhost/...` 还原成 `mdx-res://localhost/...`；协议处理器必须跳过该 authority。
- Firefox Shadow DOM 不应用 Tailwind `@property` 默认值；浮层要自己声明边框与阴影变量。
- 桥接曾把非 2xx 统一成 `SERVER_ERROR`；新增错误码时同步扩展白名单与文案。
- `web-ext lint` 现有 7 条 warning（0 error）：`data_collection_permissions` 与打包产物的 `innerHTML`，不要为此改 MV3。
- `first_hit` 的远程同步查询受超时与重试影响，可能远超单次超时；这是契约选择，不是 bug。
- 弹窗推荐只到条目/义项级，不对整段 HTML 做子串高亮；扩展可读语境建议但不能采用 AI 释义；迟到的原句不会重触发语境请求。
- M4 文档中的“远程超时 +200ms”目标与重试契约冲突；以 `BACKEND_STRUCTURE` 的重试规则为准。

## 11. 文档维护

- `docs/` 给人看：讲产品、交互、架构与契约，避免实现常量堆砌；`AGENTS.md`、本文件、`.agents/` 给 agent 看：写准确路径、符号、顺序与陷阱。
- 入口文档有预算：AGENTS ≤60 行；超出的细则放本文件或 docs，不要塞回 AGENTS。
- 契约只保留一个事实源；其他文档用相对链接引用，不复制字段表。
- 每次功能完成更新 PROGRESS，每次契约变更更新 docs 与测试，每次用户纠正更新 AGENTS。
- 文档重构方法沉淀在 [.agents/skills/repo-doc-curation/SKILL.md](.agents/skills/repo-doc-curation/SKILL.md)。