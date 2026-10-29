# PROGRESS

最后更新：2026-10-29。新会话先读本文件，再读当前阶段计划。

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

## 进行中

- M0 收尾：T0.7 手工应用矩阵（Notepad / Chrome / Firefox / VS Code 等）待用户在真实桌面补测，写入 M0 报告第 6 节。
- CI 文件已提交，GitHub Actions 尚未实跑。

## 接下来

1. 进入 [PLAN_M2.md](PLAN_M2.md)：Tauri Command、事件与 `mdx-res` 协议、SQLite 缓存与 saved_cards、多卡 UI。

## 代码影响检查（M1 已落地，M2 仍需改）

- `src-tauri/src/core/`：M1 已加 `entry/config/error/html/registry/query_router/runtime`；M2 需加 `get_query_meta` 环形缓存与 Tauri 事件转发。
- `src-tauri/src/parsers/mdx.rs`、`sources/local_file.rs`、`anki/*`、`bin/lexigrab-dev.rs`：M1 新模块；M2 复用并把阻塞调用放进 `spawn_blocking`。
- `src-tauri/src/commands.rs`、`lib.rs`、`platform/selection.rs`：M2 加 lookup/Anki/设置命令、`SelectionContext.sentence/url/title/domain` 与 `mdx-res` 协议。
- `src-tauri/src/core/config.rs`：`[sense]`/`[llm]` 已存在但未接 UI；M4 使用。
- `src/lib/types.ts`、`events.ts`、popup/settings：仍是 M0 占位，M2 接 Entry/SenseResolution/AiDefinition。
- 扩展目录尚不存在；M3 实现 Firefox 扩展与桥接轮询。
- 数据库尚不存在；M2 加 `saved_cards`，M4 加 `sense_cache`。

## 已知问题与阻塞

- **依赖偏差**：计划中的 libmdx git `111cab6c` 在 GitHub/crates.io 均不存在；M1 改用 `mdictlib 0.2.8`（MIT）。详见 [M1-core-baseline](../notes/M1-core-baseline.md) 第 2 节。
- 本机 Windows Defender 开启“受控文件夹访问”：未签名调试程序无法写 `%APPDATA%`/`%LOCALAPPDATA%`；测试临时目录改用 `src-tauri/target/test-tmp`，运行应用时用 `LEXIGRAB_HOME` 指向工作区内目录，或把 `lexigrab.exe` 加入白名单。
- 本机 MSVC 编译 `aws-lc-sys`（rustls 默认后端）失败（cl D8050）；reqwest 改 `native-tls`（Schannel）。
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
