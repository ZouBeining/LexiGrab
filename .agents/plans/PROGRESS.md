# PROGRESS

最后更新：2026-11-15。新会话先读本文件，再读当前阶段计划。

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
  - 用 uv 管理的 Python + pywinauto 驱动真实应用跑完 9 项矩阵（源管理、划词/焦点、真实 Anki 制卡、托盘/单实例、mdx-res 渲染、时延），脚本与产物在 [.agents/automation/m2-manual](../../automation/m2-manual/)，结论写入 [M2-acceptance](../notes/M2-acceptance.md) 第 3 节。
  - 时延：弹窗可见 p95 84.3ms；首结果 p95 296.4ms；41MB 词典加载中导航 137ms。真实 Anki 26.9.3 落库字段/标签核对通过。
  - 新发现 3 个待修复项（已于 2026-11-15 修复，见下）：mdx-res 图片在弹窗内破图、同语境提示对含本地媒体释义失效、托盘图标无 tooltip/可访问名称。
- **M2 验收缺陷修复（2026-11-15）**：
  - `mdx-res` 破图：Wry 在 Windows 把 `http://mdx-res.localhost/{id}/...` 还原为 `mdx-res://localhost/{id}/...` 再交给协议处理器，`localhost` 被误读为 dict_id。`core/resources.rs` 跳过该 authority，并补 `mdx-res://localhost/...` 解析与端到端测试；`p5` 复跑 `apple` 正常显示 Fruit 插图。
  - 同语境提示失效：预检与导出统一走 `anki::export::context_definition_text`（先剥离本地媒体再转纯文本），含图片/音频的释义第二次保存会命中已存语境；`card_tests` 覆盖预检与落库指纹一致。
  - 托盘可访问名称：`TrayIconBuilder` 加 `.tooltip("LexiGrab")`，UIA Name 复验为 `LexiGrab`。
  - Rust 测试 193 项全绿；`clippy`/`fmt`/`typecheck`/`lint`/`build` 通过。

## 进行中

- M0 收尾：T0.7 手工应用矩阵（Notepad / Chrome / Firefox / VS Code 等）待用户在真实桌面补测，写入 M0 报告第 6 节。
- CI 文件已提交，GitHub Actions 尚未实跑。

## 接下来

1. 进入 [PLAN_M3.md](PLAN_M3.md)：Firefox 扩展、本地桥接（令牌/Origin/限流）、扩展配对与气泡。
2. M2 验收缺陷已修复并复验：`p5` 已复跑通过（划词 + 弹窗内 `mdx-res` 图片渲染），托盘可访问名称经 UIA 实测为 `LexiGrab`；含媒体释义的同语境指纹由 `card_tests` 覆盖（未重跑会写真实 Anki 的 `p3`）。

## 代码影响检查（M2 已落地，M3/M4 仍需改）

- `src-tauri/src/core/`：M1 `entry/config/error/html/registry/query_router/runtime`；M2 加 `resources`，`state` 扩展为应用状态中心。M4 在这里接 `EventSink` 远程事件与缓存。
- `src-tauri/src/db/`：M2 新增 `saved_cards` + 指纹；M4 加 `lookup_cache`/`sense_cache` 与清理命令。
- `src-tauri/src/anki/export.rs`：M2 新增导出（剥离媒体、同语境检查）；M4 追加 AI 标签与 `AI · <model>` 来源。
- `src-tauri/src/commands.rs`、`lib.rs`、`shortcut.rs`、`window.rs`：M2 定型命令表与生命周期；M3 加桥接命令，M4 加语境命令。
- `src/lib/{types,ipc,events,resource,format}.ts`：M2 定稿 DTO 与事件；M4 增远程/语境类型。`src/components` 为共享原语。
- `src/windows/{popup,manager,settings}`：M2 已实现；M4 在设置页填语境辅助/缓存，在弹窗填推荐条/AI 卡。
- 扩展目录尚不存在；M3 实现 Firefox 扩展与桥接轮询。
- 数据库已有 `saved_cards`；M4 加 `lookup_cache`、`sense_cache` 表与迁移。
- `src-tauri/tauri.conf.json`/`capabilities/default.json`：M3 加桥接相关权限时同步更新。

## 已知问题与阻塞

- **依赖偏差**：计划中的 libmdx git `111cab6c` 在 GitHub/crates.io 均不存在；M1 改用 `mdictlib 0.2.8`（MIT）。详见 [M1-core-baseline](../notes/M1-core-baseline.md) 第 2 节。
- **工作区低完整性标签**：本机仓库根目录带 `Mandatory Label\Low Mandatory Level`，从仓库内构建/启动的 `lexigrab.exe` 也以 Low 运行，无法向普通窗口 `SendInput`（划词超时）且不能写 `%APPDATA%`。自动化把 exe 复制到 `%TEMP%` 运行；此前归因 Defender 受控文件夹访问的判断已修正。详见 [M2-acceptance](../notes/M2-acceptance.md) §3.3。
- M2 验收（2026-11-07）发现的 3 项缺陷已于 2026-11-15 修复：`mdx-res` 破图（Windows `localhost` authority 被误读为 dict_id）、含媒体释义的同语境指纹不一致、托盘图标无 tooltip/UIA 名称。详见 [M2-acceptance](../notes/M2-acceptance.md) 第 4 节。
- 本机 MSVC 编译 `aws-lc-sys`（rustls 默认后端）失败（cl D8050）；reqwest 改 `native-tls`（Schannel）。
- 本机 MSVC 14.51 编译 bundled SQLite（`libsqlite3-sys`）的 C 代码同样报 `D8050`；`src-tauri/Cargo.toml` 设 `[profile.dev] debug = 0` 规避，Rust 侧回溯仍可用。若换机器可去掉该限制。
- **依赖偏差**：`rusqlite` 实际锁定 `0.32.1`（`bundled`），计划为 `0.33`；契约行为一致，记录在 [TECH_STACK](../../docs/TECH_STACK.md) 第 5 节。
- M2 手工矩阵已用 pywinauto 自动化补测（结论见 M2-acceptance 第 3 节）；自动测试、CLI 回归与启动冒烟此前已过。
- 工作区沙箱下 cargo 走 Schannel 会报 `SEC_E_NO_CREDENTIALS`；需要在可联网的普通终端 `cargo fetch`/`build`。`git` 用 `-c http.sslBackend=openssl` 可访问 GitHub。
- pnpm 11 在 Windows 默认 junction 布局会让 ESLint 解析不到传递依赖；仓库用 `pnpm-workspace.yaml` 的 `nodeLinker: hoisted` 规避。
- Git 直接使用 Schannel 克隆 GitHub 会报 `SEC_E_NO_CREDENTIALS`；使用 `git -c http.sslBackend=openssl` 可克隆。

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
