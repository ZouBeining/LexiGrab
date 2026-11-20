# PROGRESS

最后更新：2026-11-21。新会话先读本文件，再读当前阶段计划。

## 已完成

- 仓库规划文档：[PRD](../../docs/PRD.md)、[APP_FLOW](../../docs/APP_FLOW.md)、[TECH_STACK](../../docs/TECH_STACK.md)、[FRONTEND_GUIDELINES](../../docs/FRONTEND_GUIDELINES.md)、[BACKEND_STRUCTURE](../../docs/BACKEND_STRUCTURE.md)。
- 六个阶段的计划：[M0](PLAN_M0.md)、[M1](PLAN_M1.md)、[M2](PLAN_M2.md)、[M3](PLAN_M3.md)、[M4](PLAN_M4.md)、[M5](PLAN_M5.md)。
- 参考材料目录与 [references/README.md](../../references/README.md)；ODH 已浅克隆到 `references/ODH`（commit `ce5e7da`，MIT），被 `.gitignore` 排除。
- 设计主参考已确认：`.agents/design/figma.md`；Apple/Cursor 仅作可替换配色主题。
- M0 工程骨架与划词链路：[M0-selection-capture](../notes/M0-selection-capture.md)；`cargo test`、`pnpm typecheck`、`pnpm lint` 通过。
- **M1 核心链路（2026-10-29）**：
  - 配置：`Settings` 全量字段、schema v1→v2 迁移、校验、原子保存、损坏备份、首次生成 device_id/bridge token；`LEXIGRAB_CONFIG_DIR` 覆盖。
  - MDX：`mdictlib 0.2.8` 适配层 + 每文件 worker（15s 打开 / 500ms 查询超时、有界队列、`WorkerDead`/`Timeout`/`Busy`、关闭回收）。
  - HTML：lol_html 资源改写为 `mdx-res://{dict_id}/{path}` + ammonia 白名单清洗；512KB 上限。
  - 义项：`SenseCandidate` 提取（列表/段落/整条退化、词性、最多 20）。
  - 数据源与路由：`LocalFileSource`、`SourceRegistry`（priority/id 排序、reload 复用、坏文件隔离）、`QueryRouter`（first_hit/aggregate/local_first、失败隔离、条数上限与去重）。
  - Anki：AnkiConnect 阻塞客户端（1.5s/3s 超时、错误分类、恒 `allowDuplicate: true`）与四字段映射；wiremock 覆盖。
  - CLI：`lexigrab-dev` 的 lookup / sources / anki-decks / anki-models / add-note / bench；退出码 0/2/3/4。
  - 夹具：`tests/fixtures/mini.mdx`/`mini.mdd`/`broken.mdx`/`mini-config.toml`，Rust 生成器 `examples/gen_fixtures.rs`。
  - 报告：[M1-core-baseline](../notes/M1-core-baseline.md)；`cargo test` 125 项、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check` 全绿。
- **M2 桌面端 MVP（2026-10-29）**：
  - 命令层：`AppState`（配置/注册表/CardStore/QueryMeta 环形缓存/捕获诊断）+ 24 个 Tauri 命令；查询、Anki、建库统一 `spawn_blocking`，错误码沿用 BACKEND_STRUCTURE。
  - 资源协议：`mdx-res` 双形态解析（`mdx-res://` 与 `http://mdx-res.localhost/`）、路径穿越/编码/MIME 白名单、nosniff 与缓存头；CSP/devCsp 放行资源与 `ipc:`。
  - 弹窗：预创建 + 状态机（capturing/loading/results/empty/queryError/captureError/editing/saved/saveError）；快速模式 420×560 不抢焦点，编辑态 560×640，保存后不自动关闭。
  - 管理器：本地源增删改、启停、排序、状态与重试、同名 `.mdd` 提示、重复路径拦截、首次引导、查询测试。
  - 设置：通用（热键录制、主题、策略、条数）、Anki（探测、牌组、模板、字段映射）、关于；语境辅助/扩展/缓存为 M3/M4 说明块。
  - 生命周期：单实例插件、托盘、关闭隐藏、退出注销热键、启动后台加载词典与探测 Anki。
  - 同词多卡：`saved_cards`（SQLite/WAL/损坏隔离）、NFKC+小写+压缩空白+uuid v5 指纹、`warn|allow` 策略、始终 `allowDuplicate: true`；导出前剥离本地媒体。
  - 句子捕获：UIA 段落 120ms 预算，失败留空并异步补发选区事件。
  - 报告：[M2-acceptance](../notes/M2-acceptance.md)；Rust 193 项、前端 75 项测试，`clippy`/`fmt`/`typecheck`/`lint`/`vite build` 全绿。
- **M2 手工矩阵自动化（2026-11-07）**：
  - 用 uv 管理的 Python + pywinauto 驱动真实应用跑完 9 项矩阵（源管理、划词/焦点、真实 Anki 制卡、托盘/单实例、mdx-res 渲染、时延），脚本与产物在 [.agents/automation/m2-manual](../automation/m2-manual/)，结论写入 [M2-acceptance](../notes/M2-acceptance.md) 第 3 节。
  - 时延：弹窗可见 p95 84.3ms；首结果 p95 296.4ms；41MB 词典加载中导航 137ms。真实 Anki 26.9.3 落库字段/标签核对通过。
  - 新发现 3 个待修复项（已于 2026-11-15 修复，见下）：mdx-res 图片在弹窗内破图、同语境提示对含本地媒体释义失效、托盘图标无 tooltip/可访问名称。
- **M3 Firefox 扩展与本地桥接（2026-11-15）**：
  - 桥接：`src-tauri/src/bridge/{mod,auth,dto,error,server}.rs`，axum 只绑定 `127.0.0.1`；守卫中间件按“预检→Origin→令牌桶→`X-Auth-Token` 常量时间比较”顺序，并给所有响应（含 401/403/413/429）补 CORS。请求体 1MB、20 次/秒突发 40、处理 10s 超时。
  - 端点：`GET /v1/health`、`POST /v1/selection`（并发本地源 3.5s 截止、`partial`、剥离本地媒体）、`GET /v1/sense/{query_id}`（disabled 200 / pending 202 / 未知 404）、`POST /v1/anki`（`exact_context_seen`）。
  - 生命周期：`BridgeService` 随配置启停，端口占用状态 failed 不静默换端口；`get_bridge_status`、`regenerate_bridge_token`（旧令牌立即 401）；`update_settings` 变更端口/开关即重启。
  - 设置页：`ExtensionSection.tsx` 的启用、端口、状态与最近请求、令牌显示/复制/二次确认轮换、配对三步。
  - 扩展：`extension/` 使用 WXT 0.21.5 + React 19 + Tailwind 4；`lib/sentence.ts` 纯函数句子提取（Intl.Segmenter 优先、标点回退、隐藏元素与密码框过滤），`lib/bridge.ts` 固定错误码客户端，Shadow DOM 浮动按钮 + 360×480 结果气泡（DOMPurify 二次清洗、语境轮询、同词连续保存不阻断），options 页配对与测试连接。
  - 产物：`pnpm -C extension build:firefox` + `web-ext lint`（0 error / 7 warning）+ `web-ext build` 生成未签名 `lexigrab-firefox-0.1.0.xpi`（sha256 见报告）。
  - 报告：[M3-extension-verification](../notes/M3-extension-verification.md)；Rust 228 项、扩展 34 项、前端 81 项测试，`clippy`/`fmt`/`typecheck`/`lint`/`vite build` 全绿。
- **M4 远程源、语境辅助与缓存（2026-11-20）**：
  - 远程源：`RemoteHttpSource`（模板/鉴权/超时/重试/跨主机重定向停止/2MB/状态“上次失败”）、`build_source` 与 `DictionarySource::is_remote`；`test_query` 返回条目与前 4KB 原始响应。
  - 解析器：`ResponseParser` 契约 + `finalize_entries`（清洗、512KB、20 条、绝对 http(s) 资源）；`JsonPathParser`（jsonpath-rust 1.0.6）；`ScriptParser`（rquickjs 0.14，16MB/256KB 栈/50–1000ms 中断器、源码 mtime 缓存、无 fetch/require/timer）。
  - 缓存：`db/cache.rs` 的 `CacheStore`（`lookup_cache`/`sense_cache`、WAL、busy_timeout、`user_version=2`、可注入时钟、正/负缓存、按源失效、统计与清理、损坏隔离）。
  - 路由：`QueryRouter::lookup_planned` 支持三种策略；`first_hit` 远程同步、`local_first`/`aggregate` 远程后台任务；`lookup://remote-result`/`remote-error`；`QueryMeta` 记录缓存来源与 pending；桥接 `lookup_collect_all` 纳入远程并沿用 3.5s 截止。
  - 语境辅助：`core/sense/{selector,llm,resolution}`；阈值状态机；`sense_cache`（key 含 model 与 prompt version，7 天）；默认回环 + 远程显式允许；失败不自动调 LLM；命令 `test_sense_selector`/`test_llm`/`get_sense_resolution`/`clear_sense_cache`。
  - 前端：管理器远程源编辑与测试面板；设置页“缓存”“语境辅助”分区；弹窗推荐条/义项切换/AI 卡/远程进度/缓存徽标；AI 卡保存 `AI · <model>` + `ai-generated`。
  - 验收：`.agents/automation/m4-mock/` 用 Python mock + `lexigrab-dev` 跑通 JSONPath/脚本/失败隔离/空结果（8/8）；`cargo test` 328 通过（3 个性能探针默认忽略）、`clippy`/`fmt` 全绿；`pnpm typecheck`/`lint`/`test`（121）/`build` 全绿。
  - 性能：缓存 `get` p95 0.016ms、JSONPath 100 条 p95 1.865ms、选择器额外 2.3ms、本地首屏不等待远程。报告：[M4-remote-baseline](../notes/M4-remote-baseline.md)、[M4-sense-baseline](../notes/M4-sense-baseline.md)。
- **M5 打磨、文档与发布准备（2026-11-21）**：
  - 设置/UI：关于分区补构建信息（应用/Tauri/WebView）、配置/日志/缓存目录复制与打开（新命令 `open_log_dir`、`open_cache_dir`）、LICENSE 与仓库链接、重开引导；空态/保存提示补 `role=status`/`aria-live`，错误块统一 `role=alert`；扩展与桌面端 401/离线/端口/令牌文案对齐，AI 标注统一为“AI 生成 · <model> · 可能不准确”。前端测试 121→140。
  - 文档：重写 [README](../../README.md)；新增 [user-guide](../../docs/user-guide.md)、[configuration](../../docs/configuration.md)、[development](../../docs/development.md)、[extension](../../docs/extension.md)、[troubleshooting](../../docs/troubleshooting.md)、[performance](../../docs/performance.md)、[security](../../docs/security.md) 与 [CHANGELOG](../../CHANGELOG.md)；同步 APP_FLOW §7.3 端口失败行为、TECH_STACK 锁文件版本。
  - 合规：许可证标识统一 `AGPL-3.0-only`（`Cargo.toml`、两个 `package.json`、SPDX 头）；141 个源文件 SPDX 头（Rust/扩展 81 + 前端 60）；[THIRD_PARTY_LICENSES](../../THIRD_PARTY_LICENSES.md) 覆盖 59 个直接依赖；`src-tauri/deny.toml`；cargo-deny 0.22.0 实跑 `bans ok, licenses ok, sources ok`。
  - CI/发布：`ci.yml` 增加扩展 typecheck/test/build/lint、`pnpm build`、cargo-deny 与五处版本同步；新增 `release.yml`（tag 触发 NSIS、`.xpi`、SHA256SUMS、草稿 release、未签名说明）；两文件经 actionlint 校验。
  - 性能：`cache.get` p95 0.021ms、JSONPath 100 条 1.587ms、100k 合成词典本地查询 p95 0.07ms/峰值 13.5MB、选择器额外 4.1ms、远程超时 783.5ms（契约重试）；20 分钟长跑 21 次采样 0 失败，RSS 46.4→46.8MB（+0.79%），句柄 401→400。脚本与数据：[m5-soak](../automation/m5-soak/)、[performance](../../docs/performance.md)。
  - 安全：十项审查 0 高危；远程/选择器/LLM 请求失败统一 `reqwest::Error::without_url()`，避免完整 URL 进日志；令牌明文、脚本沙箱、未签名等残余风险记录在 [security](../../docs/security.md)。
  - 发布产物：release exe 13,841,920B、`.xpi` 167,973B（SHA256 见 [M5-release](../notes/M5-release.md)）；NSIS 打包因本机无法访问 GitHub 二进制下载而失败，交给 release 工作流；8 小时长跑、干净 VM 验收与打 tag 仍待执行。
- **M3 Firefox 真机验收（2026-11-19）**：
  - 本机 Firefox Developer Edition 161.0（`D:\Firefox Developer Edition`）完成 29/29 场景：未签名 `.xpi` 安装、选项页配对、真实划词→查询→Anki 制卡、同词同句/不同句、中文页、Anki 关闭与重试、令牌轮换、重启保留、Wikipedia 英/中与 BBC 各 3 次划词、端口冲突、桥接鉴权/CORS。
  - 自动化在 [.agents/automation/m3-firefox](../automation/m3-firefox/)：自带 Marionette 客户端（无 geckodriver/selenium）+ 真实桌面端/Anki/testpage，`python -u run.py` 输出 `artifacts/results.json` 与截图；环境与结果见 [M3-extension-verification](../notes/M3-extension-verification.md) 第 5 节。
  - 修复真机发现的问题：扩展非 2xx 一律映射 `SERVER_ERROR`，现透出 `ANKI_UNREACHABLE` 等服务端错误码并在保存失败时显示标题/建议动作（扩展测试 34 → 42）；Tailwind 4 的 `@property` 默认值在 Firefox Shadow DOM 内不生效导致气泡无边框/阴影，`content/style.css` 在浮层作用域补齐 `--tw-border-style`/`--tw-*shadow`。
  - 产物更新：`lexigrab-firefox-0.1.0.xpi`（167,848 字节，sha256 `CBCD1245…3B362`），`web-ext lint` 仍为 0 error / 7 warning。
- **M2 验收缺陷修复（2026-11-15）**：
  - `mdx-res` 破图：Wry 在 Windows 把 `http://mdx-res.localhost/{id}/...` 还原为 `mdx-res://localhost/{id}/...` 再交给协议处理器，`localhost` 被误读为 dict_id。`core/resources.rs` 跳过该 authority，并补 `mdx-res://localhost/...` 解析与端到端测试；`p5` 复跑 `apple` 正常显示 Fruit 插图。
  - 同语境提示失效：预检与导出统一走 `anki::export::context_definition_text`（先剥离本地媒体再转纯文本），含图片/音频的释义第二次保存会命中已存语境；`card_tests` 覆盖预检与落库指纹一致。
  - 托盘可访问名称：`TrayIconBuilder` 加 `.tooltip("LexiGrab")`，UIA Name 复验为 `LexiGrab`。
  - Rust 测试 193 项全绿；`clippy`/`fmt`/`typecheck`/`lint`/`build` 通过。

- **文档体系重构（2026-11-20）**：
  - [AGENTS.md](../../AGENTS.md) 重写为 47 行总纲：文档地图、会话启动、硬约束、验证门禁与提交规则，入口指向 PROMPT。
  - [PROMPT.md](../../PROMPT.md) 重写为 agent 实现路径：代码地图、关键链路、IPC/事件/桥接/存储不变式、任务配方、验证矩阵与已知陷阱。
  - 新增 [ROADMAP.md](../../ROADMAP.md)（组件位置、职责与阶段状态）与 [docs/ARCHITECTURE.md](../../docs/ARCHITECTURE.md)（人读的分层与数据流）；修正 PRD、BACKEND_STRUCTURE、TECH_STACK 的交叉引用与过时事实（mdictlib、cargo-deny、运行时图）。
  - 新增 `.agents/skills/repo-doc-curation/`，沉淀双读者文档重构与事实核对方法。

## 进行中

- **M5 发布门禁（仓库侧已完成，以下需真实环境）**：
  - 8 小时长跑：`.agents/automation/m5-soak/` 已就绪，本次只跑 20 分钟冒烟；发布前跑 `--minutes 480` 并把 `artifacts/summary.json` 补进 [performance](../../docs/performance.md) §3.3。
  - 干净 Windows 虚拟机验收：安装 Anki+AnkiConnect、真实 MDX/MDD、记事本与 Firefox 划词、制卡、扩展配对、远程 mock、断网缓存、重启、卸载；对照 PRD 成功标准与 APP_FLOW 主路径。
  - 打 `v0.1.0` tag 触发 release 工作流并检查草稿 release；本次未 push、未打 tag。
- M0 收尾：T0.7 手工应用矩阵（Notepad / Chrome / Firefox / VS Code 等）待用户在真实桌面补测，写入 M0 报告第 6 节。
- GitHub Actions 尚未实跑（CI 与 release 工作流已提交，版本同步脚本本地实跑通过）。

## 接下来

1. 执行上面的 M5 发布门禁：8 小时长跑 → 干净 VM 验收 → 打 tag 发布。
2. 发布后把长跑数据、安装包与 `.xpi` 的 SHA256、release 链接补进 PROGRESS 与 [performance](../../docs/performance.md)。
3. M4 已把 `/v1/sense` 接成真实 `SenseResolution`；若后续让扩展采用 AI 释义，需要新增桥接字段与扩展交互。

## 代码影响检查（M5 已落地）

- `src-tauri/src/core/`：M1 `entry/config/error/html/registry/query_router/runtime`；M2 加 `resources`，`state` 成为状态中心；M3 加 `lookup_collect_all` 与语境环形缓存；M4 加 `events`、`sense/` 与 `CacheStore`；M5 只改错误文案（请求失败用 `reqwest::Error::without_url()` 去 URL），契约未变。
- `src-tauri/src/bridge/`：M3 新增；M4 让 `/v1/selection` 后台解析语境；M5 未改鉴权与路由。
- `src-tauri/src/db/`：M2 `saved_cards`；M4 加 `lookup_cache`/`sense_cache`（同一 `cache.db`，`user_version=2`）；M5 未改结构。
- `src-tauri/src/anki/`：M2 导出；M4 支持 `AnkiPayload.tags`（AI 卡 `ai-generated`）与 `AI · <model>` 来源；M5 未改。
- `src-tauri/src/commands.rs`、`lib.rs`：M4 加缓存/语境命令；M5 加 `open_log_dir`、`open_cache_dir`（与 `open_config_dir` 共用打开目录逻辑），并已同步 BACKEND_STRUCTURE 与 `src/lib/ipc.ts`。
- `src/lib/{types,ipc,events}.ts`：M4 增远程源 union、`CacheStats`、`TestResult.raw_preview/error`、`AnkiPayload.tags`、`SenseResolution.candidates`、`lookup.sentence`；M5 加 `openLogDir`/`openCacheDir` 封装。
- `src/windows/{popup,manager,settings}`：M4 完成远程源编辑/测试、缓存与语境设置、弹窗推荐/AI/远程状态；M5 补 About 构建信息与目录操作、aria/live region、错误文案，新增 `AboutSection.test.tsx`。
- `extension/`：M3 完整工程；M4 未改代码；M5 统一 options 与气泡的 401/离线/令牌文案与 AI 标注，网络逻辑未变。
- `.github/workflows/`：M5 重写 `ci.yml`（扩展、build、cargo deny、版本五处同步）并新增 `release.yml`（NSIS、`.xpi`、SHA256、草稿 release）。
- `src-tauri/tauri.conf.json`/`capabilities/default.json`：M5 未新增 Tauri 权限。


## 已知问题与阻塞

- **依赖偏差**：计划中的 libmdx git `111cab6c` 在 GitHub/crates.io 均不存在；M1 改用 `mdictlib 0.2.8`（MIT）。详见 [M1-core-baseline](../notes/M1-core-baseline.md) 第 2 节。
- **工作区低完整性标签**：本机仓库根目录带 `Mandatory Label\Low Mandatory Level`，从仓库内构建/启动的 `lexigrab.exe` 也以 Low 运行，无法向普通窗口 `SendInput`（划词超时）且不能写 `%APPDATA%`。自动化把 exe 复制到 `%TEMP%` 运行；此前归因 Defender 受控文件夹访问的判断已修正。详见 [M2-acceptance](../notes/M2-acceptance.md) §3.3。
- M2 验收（2026-11-07）发现的 3 项缺陷已于 2026-11-15 修复：`mdx-res` 破图（Windows `localhost` authority 被误读为 dict_id）、含媒体释义的同语境指纹不一致、托盘图标无 tooltip/UIA 名称。详见 [M2-acceptance](../notes/M2-acceptance.md) 第 4 节。
- 本机 MSVC 编译 `aws-lc-sys`（rustls 默认后端）失败（cl D8050）；reqwest 改 `native-tls`（Schannel）。
- 本机 MSVC 14.51 编译 bundled SQLite（`libsqlite3-sys`）的 C 代码同样报 `D8050`；`src-tauri/Cargo.toml` 设 `[profile.dev] debug = 0` 规避，Rust 侧回溯仍可用。若换机器可去掉该限制。
- **依赖偏差**：`rusqlite` 实际锁定 `0.32.1`（`bundled`），计划为 `0.33`；契约行为一致，记录在 [TECH_STACK](../../docs/TECH_STACK.md) 第 5 节。
- **依赖偏差（M3）**：`axum` 解析到 `0.8.9`（声明 0.8.1）；桥接未引入计划中的 `tower-http`/`subtle`/`tokio-util`：CORS、令牌桶与常量时间比较手写在 `bridge/`，请求体上限用 axum `DefaultBodyLimit`，关停用 `tokio::sync::oneshot`。记录在 TECH_STACK 第 5 节。
- M3 真机验收已于 2026-11-19 在 Firefox Developer Edition 161.0 完成（29/29）；本机浏览器访问境外站点需走系统代理。`.xpi`、sha256、安装步骤与 `web-ext lint` 结果见 [M3-extension-verification](../notes/M3-extension-verification.md)。
- 本轮修复的扩展问题（2026-11-19）：桥接非 2xx 错误码被统一成 `SERVER_ERROR`（现白名单透出）；Tailwind `@property` 默认值在 Firefox Shadow DOM 内不生效，导致气泡缺边框/阴影。详见 M3 报告 §5.3。
- `web-ext lint` 对扩展有 7 条 warning（0 error）：AMO 2025-11 的 `data_collection_permissions` 声明（自托管未签名分发不适用，且补键会与 `strict_min_version=109.0` 冲突）与 React/DOMPurify 打包产物里的 `innerHTML`。
- M2 手工矩阵已用 pywinauto 自动化补测（结论见 M2-acceptance 第 3 节）；自动测试、CLI 回归与启动冒烟此前已过。
- 工作区沙箱下 cargo 走 Schannel 会报 `SEC_E_NO_CREDENTIALS`；需要在可联网的普通终端 `cargo fetch`/`build`。`git` 用 `-c http.sslBackend=openssl` 可访问 GitHub。
- pnpm 11 在 Windows 默认 junction 布局会让 ESLint 解析不到传递依赖；仓库用 `pnpm-workspace.yaml` 的 `nodeLinker: hoisted` 规避。
- Git 直接使用 Schannel 克隆 GitHub 会报 `SEC_E_NO_CREDENTIALS`；使用 `git -c http.sslBackend=openssl` 可克隆。
- **M4 性能偏差**：计划 T4.10 写“远程超时 ≤配置+200ms”，但契约要求超时最多重试 1 次 + 150ms 间隔，实测 300ms 配置为 762.5ms（≈2×+150）。实现优先契约；`local_first`/`aggregate` 远程在后台，不拖首屏，只有 `first_hit` 同步等待。详见 [M4-remote-baseline](../notes/M4-remote-baseline.md) §4。
- **M4 已知限制**：弹窗推荐为“条目级徽标 + 义项切换列表”，不对整段词典 HTML 做子串高亮；扩展读取语境建议但不可采用 AI 释义；迟到的原句补写不会重新触发语境请求。详见 [M4-sense-baseline](../notes/M4-sense-baseline.md) §5。
- **M4 依赖新增**：`jsonpath-rust =1.0.6`、`rquickjs 0.14.0`（`default-features=false`，关闭 rust-alloc 以保证 `set_memory_limit` 生效）。已写入 [TECH_STACK](../../docs/TECH_STACK.md)。
- **M5 未执行的环境门禁**：8 小时长跑只做了 20 分钟冒烟（脚本已就绪）；干净 Windows 虚拟机验收、tag 发布与 GitHub Actions 实跑未做。原因：本次会话无真实 MDX/MDD 样本（`references/dict` 为空），且发布需要 push/tag。
- **M5 性能未覆盖**：500MB 级词典内存、真实 LLM 端到端时延、桌面端 E2E 时延未在本次复测；[performance](../../docs/performance.md) 已标注“沿用 M2 证据”或“未验证”，没有把旧数据说成 M5 复测。
- **M5 端口行为**：端口保存后桥接启动失败不会回滚配置，状态显示占用错误；用户改端口再保存即可。已同步 [APP_FLOW](../../docs/APP_FLOW.md) §7.3 与用户指南。
- **M5 版本点**：`extension/wxt.config.ts` 的 `manifest.version` 是第五个版本点，CI/release 已纳入断言；改版本时五处一起改。

## 关键决策

- UI 主参考 figma；0.1.0 只做三套浅色主题（figma/apple/cursor），不做深色。
- 主题只换 CSS 变量，布局与组件结构固定按 figma。
- 浏览器扩展只做 Firefox，`.xpi` 未签名分发。
- 远程解析同时支持 JSONPath 与脚本；脚本 200ms/16MB 沙箱。
- 语境辅助：JEV-like 只做推荐，置信度是模型自评；LLM 只在低置信度兜底，默认关闭、默认回环、必须人工采用。
- 同词多卡：Anki 始终 `allowDuplicate: true`；应用层用词+句+释义指纹只提示同语境。
- ODH 只研究，不复制代码。
- 三个窗口启动时预创建（popup/settings 隐藏）：Tauri 在命令线程里构建 WebView 会阻塞事件循环，懒创建不可用。
- 快速模式用纯 Win32 `ShowWindow(SW_SHOWNOACTIVATE)` 显示，不走 tao `show()`；弹窗隐藏用 `SW_HIDE`，不然 tao 状态不一致导致空操作。
- M1 起 MDX 读取统一走 `mdictlib`；查询归一化在适配层做（NFKC、去首尾标点、四种大小写变体），不改库。
- M1 配置以完整文件路径寻址（`ConfigStore::with_config_path`）；CLI `--config` 指向任意文件名，桌面端默认 `config.toml`。
- M2 起 `AppState` 是唯一状态中心；命令只做参数整理与阻塞边界，重活走 `spawn_blocking`，词典重载在后台线程完成后再替换注册表。
- M2 只建 `saved_cards`（同语境提示）；`lookup_cache`/`sense_cache` 留给 M4，避免 M2 引入未验证的缓存失效路径。
- `mdx-res` 逻辑 URI 不变；Windows 渲染为 `http://mdx-res.localhost/...`，协议处理器同时接受两种形态，只允许 GET/HEAD 与白名单 MIME。
- 弹窗编辑不新增第四窗口：同一 popup 窗口在快速/编辑两态间切换，尺寸由后端切换。
- 词典内链接统一交系统浏览器，WebView 不导航；语法高亮/外部工具不上前。
- 同语境提示可被“仍然保存”覆盖；Anki 永远允许重复，应用层不阻断不同语境。
- 桥接用 axum 在应用进程内监听 `127.0.0.1`，随配置启停；端口占用只进状态不换端口；令牌每次请求现读配置，轮换无需重启。
- 扩展的 content script 不直接 fetch：网络请求与令牌只在 background；气泡渲染在 Shadow DOM，释义用 DOMPurify 二次清洗。
- `/v1/selection` 返回的释义先剥离本地媒体再给扩展，浏览器无法访问 `mdx-res`。
- 扩展句子提取优先 `Intl.Segmenter`，缺失时按中英文标点回退；块文本 ≤300 字符直接用整块，避免过度切分。
- 扩展按 BACKEND_STRUCTURE §5.2 白名单透出桥接错误码（Anki/限流/体积/请求类），未知码回落 `SERVER_ERROR`；保存失败同时显示标题、建议动作、服务端文案与码。
- content script 的 Tailwind 浮层在 `.lg-interactive` 作用域自行声明 `--tw-border-style`/`--tw-*shadow` 默认值，规避 Firefox Shadow DOM 不应用 `@property` 的问题。
- M4 缓存只覆盖远程源（网络慢），本地 MDX 不缓存；查询键复用 NFKC+小写归一化，正/负缓存共用一张表。
- M4 `first_hit` 的远程源同步查询以保持“首个命中即停止”；`local_first`/`aggregate` 的远程源一律后台任务，首屏只等本地与缓存。
- M4 语境结果只在成功状态（selected/ambiguous/low_confidence）写 `sense_cache`；key 含 model 与 prompt version，拒绝把失败缓存成空结果。
- M4 的 `AnkiPayload` 增加可选 `tags`：AI 卡由前端写 `AI · <model>` 与 `ai-generated`，后端只做标签合并与转义，不推断来源。
- M4 的 `SenseResolution` 携带 `candidates` 快照，UI 才有义项切换与推荐依据；`Entry[]` 始终不被修改。
- M5 统一许可证标识为 `AGPL-3.0-only`（`Cargo.toml`、两个 `package.json`、SPDX 头、README/计划），`deny.toml` 同时允许 `-or-later` 以兼容未来依赖。
- M5 的长期性能证据用回环桥接驱动真实应用，不依赖 UI 点击；`%TEMP%` 不可写时 exe 回退到 `%LOCALAPPDATA%\Temp`，`LEXIGRAB_HOME` 配置固定在 `<home>/config/config.toml`。
- M5 长跑与 100k bench 脚本放在 `.agents/automation/m5-soak/`，产物不入库；`docs/performance.md` 只写实测与明确的未覆盖项。
