# 开发文档

本文件面向修改 LexiGrab 的开发者：模块位置、线程与阻塞边界、测试与夹具、验证命令、发布流程、调试方法和本机构建限制。产品与契约见 [PRD.md](PRD.md)、[APP_FLOW.md](APP_FLOW.md)、[BACKEND_STRUCTURE.md](BACKEND_STRUCTURE.md)；任务断点与计划见 [PROGRESS.md](../.agents/plans/PROGRESS.md) 与 [.agents/plans](../.agents/plans/)。

## 1. 环境

| 工具 | 版本 | 说明 |
| --- | --- | --- |
| Windows | 10 22H2 / 11 | 0.1.0 只承诺 Windows |
| Node.js | 24.22.1 | 前端、扩展与脚本 |
| pnpm | 11.28.5 | 唯一包管理器；仓库使用 `nodeLinker: hoisted` |
| Rust | 1.99.1 | 后端与 Tauri |
| MSVC | Visual Studio C++ 生成工具 | 编译 Tauri 与原生依赖 |
| WebView2 | 系统稳定版 | Tauri WebView |

依赖版本以 `package.json`、`extension/package.json`、`src-tauri/Cargo.toml` 和两份锁文件为准；版本表见 [TECH_STACK.md](TECH_STACK.md)。

```powershell
pnpm install
pnpm tauri dev
```

只启动 Vite 前端：

```powershell
pnpm dev
```

扩展开发：

```powershell
pnpm -C extension dev
```

## 2. 模块地图

| 区域 | 位置 | 职责 |
| --- | --- | --- |
| 启动与生命周期 | `src-tauri/src/lib.rs`、`main.rs` | 路径解析、日志、单实例、托盘、窗口预创建、资源协议、命令注册、退出清理 |
| 命令层 | `src-tauri/src/commands.rs` | Tauri Command；只做参数整理、`spawn_blocking` 与错误转换 |
| 状态中心 | `src-tauri/src/core/state.rs` | `AppState`、配置、源注册表、缓存、查询元数据、远程任务、语境结果 |
| 配置 | `src-tauri/src/core/config.rs` | `Paths`、`ConfigStore`、schema v2 迁移、校验、原子保存、损坏恢复 |
| 查询调度 | `src-tauri/src/core/query_router.rs` | `local_first` / `first_hit` / `aggregate`、缓存优先、失败隔离 |
| 源注册表 | `src-tauri/src/core/registry.rs` | 构建源、按优先级排序、重载与坏源隔离 |
| 数据源 | `src-tauri/src/sources/` | 本地多文件源、远程 HTTP 源、状态与失效 |
| 解析器 | `src-tauri/src/parsers/` | MDX/MDD、JSONPath、QuickJS 脚本、统一条目清洗 |
| 义项提取 | `src-tauri/src/parsers/sense.rs` | 从清洗后的 HTML 提取 `SenseCandidate` |
| HTML 与资源 | `src-tauri/src/core/html.rs`、`resources.rs` | 资源路径改写、ammonia 清洗、`mdx-res` 解析与 MIME/路径校验 |
| 缓存与指纹 | `src-tauri/src/db/` | `lookup_cache`、`sense_cache`、`saved_cards`、NFKC 指纹、WAL |
| 语境辅助 | `src-tauri/src/core/sense/` | 选择器 HTTP、LLM 兜底、阈值状态机 |
| Anki | `src-tauri/src/anki/` | AnkiConnect 客户端、字段映射、媒体剥离、同语境判定 |
| 本地桥接 | `src-tauri/src/bridge/` | axum 路由、令牌/Origin/限流、DTO 与错误映射 |
| 平台捕获 | `src-tauri/src/platform/` | Windows 选区、UIA 段落、光标锚点、非激活弹窗 |
| 窗口与热键 | `src-tauri/src/core/window.rs`、`shortcut.rs` | 三窗口显示/隐藏、全局热键注册与切换 |
| 开发 CLI | `src-tauri/src/bin/lexigrab-dev.rs` | `lookup`、`sources`、`anki-decks`、`anki-models`、`add-note`、`bench` |
| 前端共享层 | `src/lib/` | IPC 类型、命令封装、事件常量、主题、资源 URL、格式化 |
| 弹窗 | `src/windows/popup/` | `state.ts` 纯状态机、结果/编辑/语境/AI 卡 |
| 管理器 | `src/windows/manager/` | 源列表、源对话框、查询测试、首次引导、状态视图 |
| 设置 | `src/windows/settings/` | 通用、Anki、语境辅助、浏览器扩展、缓存、关于 |
| 扩展 | `extension/entrypoints/`、`extension/lib/` | content 划词与句子、background 持有令牌、options 配对、桥接客户端 |
| 测试 | `src-tauri/tests/`、`extension/tests/`、`src/**/*.test.*` | 集成测试、MDX 夹具、扩展与前端单元测试 |

约束：Rust 业务代码放在 `core/sources/parsers/anki/db/bridge/platform`；前端组件按窗口目录组织；IPC 类型只从 `src/lib` 导入；扩展网络请求只在 background。

## 3. 线程与阻塞边界

| 执行位置 | 内容 | 边界 |
| --- | --- | --- |
| Tauri 主线程 | 窗口创建/显示、托盘、全局热键 | 不在命令线程创建 WebView；热键切换回到主线程 |
| Tokio 异步运行时 | Tauri 命令、桥接、远程 HTTP 调度 | 命令只做薄封装 |
| `spawn_blocking` | 本地查询、AnkiConnect、远程 HTTP、SQLite | 不阻塞异步运行时 |
| 标准线程 | 每个 `.mdx` 一个 worker | 打开超时 15s，查询超时 500ms，队列有界，满载返回 `DICT_BUSY` |
| 桥接任务 | axum 监听 `127.0.0.1` | 请求体 1MB，处理超时 10s，限流 20 次/秒、突发 40 |
| QuickJS 调用 | 脚本解析 | 独立 Runtime，默认 200ms（50–1000ms），内存 16MB，栈 256KB |

一次桌面查词：

1. 热键触发捕获，写入 `AppState` 并显示弹窗。
2. 前端生成 `query_id`，调用 `lookup`。
3. 命令在阻塞边界内调用 `AppState::lookup_planned`。
4. 同步返回本地与缓存结果；远程任务逐个执行，结果通过 `lookup://remote-result` 追加，失败通过 `lookup://remote-error` 通知。
5. 有原句且启用语境辅助时，后台运行 `run_sense_resolution`，结果通过 `lookup://sense-result` 推送，同时供扩展轮询。

不变式：

- 所有异步结果必须匹配当前 `query_id`，迟到结果丢弃。
- 远程结果可早于 `lookup` promise 到达；合并时按 `(source_name, headword)` 去重，不覆盖已有条目。
- 用户手动选择义项后，迟到的推荐不得覆盖。
- AI 文本只作为候选，不写回 `Entry`；用户采用后才进入编辑区。

## 4. 测试与夹具

### Rust

```powershell
cd src-tauri
cargo test
```

集成测试按领域分组：

| 文件 | 覆盖 |
| --- | --- |
| `config_tests.rs` | 配置加载、相对路径、原子保存、损坏备份 |
| `mdx_tests.rs` | MDX 打开、大小写与标点归一化、资源、损坏文件、worker 退出 |
| `router_tests.rs` | 三种查询策略、条数截断、去重、失败隔离 |
| `remote_tests.rs` | 模板、鉴权、超时、重试、重定向、响应体上限 |
| `parsers_tests.rs` | JSONPath、脚本映射、超时/OOM/错误结构、HTML 上限 |
| `html_tests.rs` | 脚本与事件属性清除、危险链接、资源改写、路径穿越 |
| `resource_tests.rs` | `mdx-res` 双形态、MIME、路径编码与拒绝规则 |
| `cache_tests.rs` | 正/负缓存、TTL、按源失效、统计、损坏重建 |
| `card_tests.rs` | 同词同句同释义指纹、媒体剥离、数据库隔离 |
| `anki_tests.rs` | 字段映射、转义、请求结构、错误分类 |
| `bridge_tests.rs` | 鉴权、Origin、CORS、限流、请求体、错误码 |
| `sense_tests.rs`、`sense_assist_tests.rs` | 候选提取、阈值状态机、选择器/LLM 失败隔离 |
| `perf_tests.rs` | 缓存、JSONPath、远程重试、选择器、聚合不等待远程的性能探针 |

两个性能探针默认标记为 ignored，发布或排查性能时单独运行：

```powershell
cargo test --test perf_tests -- --ignored
```

### MDX 夹具

`src-tauri/tests/fixtures/` 内是自拟数据，不含受版权保护的词典内容：

| 文件 | 用途 |
| --- | --- |
| `mini.mdx` | MDX v2.0，3 条词条，含列表释义、图片与音频 |
| `mini.mdd` | 同名资源：1×1 PNG 与占位音频 |
| `broken.mdx` | 截断文件，用于打开失败 |
| `mini-config.toml` | 指向 `mini.mdx` 的完整测试配置 |

重新生成：

```powershell
cd src-tauri
cargo run --example gen_fixtures
```

生成器会重写二进制夹具并自检。CI 不运行生成器，只使用提交的夹具；修改后同步更新 `tests/fixtures/README.md` 的大小与 SHA256。

### 前端与扩展

```powershell
pnpm test
pnpm -C extension test
```

- 前端测试使用 Vitest + happy-dom + Testing Library，覆盖状态机、设置分区、弹窗与源对话框。
- 扩展测试覆盖句子提取、桥接错误码、气泡渲染与保存流程。
- 扩展没有 Playwright 端到端测试；真机验收脚本在 `.agents/automation/m3-firefox/`，需要真实 Firefox、桌面端与 Anki。

## 5. 验证矩阵

最低要求：

```powershell
pnpm typecheck
pnpm lint
pnpm test

pnpm -C extension typecheck
pnpm -C extension test
pnpm -C extension build:firefox
pnpm -C extension lint:firefox

cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

按改动追加：

| 改动 | 追加验证 |
| --- | --- |
| 查询、源、解析 | `cargo test --test router_tests --test remote_tests --test parsers_tests --test mdx_tests` |
| 缓存、指纹 | `cargo test --test cache_tests --test card_tests` |
| 桥接 | `cargo test --test bridge_tests`；必要时用 curl 手测 health/selection |
| 语境辅助 | `cargo test --test sense_tests --test sense_assist_tests` |
| 前端 | 除命令外，手工走三个窗口的 loading/empty/error |
| 扩展 | 除命令外，在 Firefox 手工验证划词、配对、保存、重启保留 |
| 发布前 | 全量门禁；干净克隆按 README 构建；`.xpi` 可安装 |

## 6. 发布流程

0.1.0 不签名、不发布到 AMO、不做自动更新。仓库提供 [release.yml](../.github/workflows/release.yml)：推送 `v*` 标签或手动触发后，工作流先校验版本一致，再构建 Windows NSIS 安装包与 Firefox `.xpi`，生成 `SHA256SUMS`，创建草稿 GitHub Release。

发布前本地仍需完成：

1. 版本同步：修改 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`、`extension/package.json`、`extension/wxt.config.ts` 的版本号。工作流会校验这五处一致，且标签等于 `v<version>`。
2. 更新锁文件：`cd src-tauri && cargo check`；前端与扩展在 `pnpm install` 后确认锁文件稳定。
3. 跑全量门禁与扩展打包检查。
4. 在干净克隆中执行 `pnpm tauri build`，确认 `src-tauri/target/release/bundle/nsis/` 有安装包。
5. 执行 `pnpm -C extension build:firefox && pnpm -C extension package:firefox`，确认 `extension/dist/lexigrab-firefox-<version>.xpi` 可安装。
6. 计算校验和：`Get-FileHash -Algorithm SHA256 <文件>`，与发布页的 `SHA256SUMS` 核对。
7. 更新 [PROGRESS.md](../.agents/plans/PROGRESS.md) 与发布说明，写明未签名、SmartScreen、Firefox 临时加载和 README 的已知限制。
8. 在干净 Windows 环境按 README 走一遍安装、添加词典、划词、制卡、扩展配对与卸载。

发布说明必须包含：

- 安装包与 `.xpi` 均未签名；Firefox 正式版只能临时加载。
- 只支持 Windows 10/11 与 Firefox 109+。
- 不提供自动更新，升级需要重新安装。
- 本地媒体不写入 Anki，四字段没有 URL。
- AI 释义不是词典原文，同词多卡是默认行为。

## 7. 调试

### 日志

- 默认目录：`%LOCALAPPDATA%\LexiGrab\logs\`；便携模式在 `<LEXIGRAB_HOME>\logs\`。
- 日志按天滚动。默认级别 `info`，用 `RUST_LOG` 覆盖，例如 `RUST_LOG=debug` 后启动。
- 日志不记录令牌、完整原句、完整 URL 或 API key；排查时也不要手动粘贴这些内容。

### 开发 CLI

```powershell
cd src-tauri
cargo run --bin lexigrab-dev -- sources --config tests/fixtures/mini-config.toml
cargo run --bin lexigrab-dev -- lookup --config tests/fixtures/mini-config.toml --word hello --json
cargo run --bin lexigrab-dev -- bench --config tests/fixtures/mini-config.toml --word hello --iterations 100
```

退出码：`0` 成功，`2` 配置/源错误，`3` Anki 错误，`4` 查询无结果。

### 其他入口

- 管理器“查询测试”使用当前源，不写缓存、不触发弹窗。
- `debug_trigger_capture` 可以绕过热键触发一次捕获；`get_capture_stats` 与 `get_last_capture` 用于诊断捕获失败。
- 设置页的“测试选择器”和“测试 LLM”不写缓存、不写卡片。
- 桥接问题先看“关于”的配置目录与设置页的桥接状态；端口占用只更新状态，不自动换端口。

## 8. 本机构建限制

- **D8050**：本机 MSVC 在自定义 debug 信息下编译 C 代码会失败（aws-lc、bundled SQLite）。项目用 `reqwest` 的 `native-tls`，并在 `src-tauri/Cargo.toml` 设置 `[profile.dev] debug = 0`。不要随手删除。
- **Schannel**：本机 cargo/git 走 Schannel 时可能报 `SEC_E_NO_CREDENTIALS`。git 可加 `-c http.sslBackend=openssl`；cargo 需要先在可联网的普通终端完成 fetch/build。
- **pnpm hoisted**：pnpm 11 在 Windows 默认 junction 布局会让 ESLint 等工具解析不到传递依赖。仓库用 `pnpm-workspace.yaml` 的 `nodeLinker: hoisted` 规避，不要改成默认布局。
- **低完整性标签**：本机仓库目录带 Low Mandatory Level 时，从仓库内构建/启动的 `lexigrab.exe` 无法向普通窗口 `SendInput`，也可能无法写 `%APPDATA%`。自动化把 exe 复制到 `%TEMP%` 运行；普通开发机通常不需要。
- **Wry 资源 URL**：Windows 会把 `http://mdx-res.localhost/...` 还原为 `mdx-res://localhost/...`，协议处理器必须跳过 `localhost` authority。改资源协议时先看 `core/resources.rs` 的测试。

## 9. 提交前检查

- 非测试 Rust 不使用 `unwrap()` / `expect()`；TS 不使用 `any`；组件不直接 `invoke`。
- 前端只用 Tailwind 工具类，不写内联 `style`，不引入组件库或图标包。
- 桥接只绑定 `127.0.0.1`，不绕过令牌、Origin 与限流。
- Anki 请求始终 `allowDuplicate: true`；应用层只提示同词同句同释义。
- 新增命令同步 `BACKEND_STRUCTURE`、`commands.rs`、`lib.rs`、`src/lib/{ipc,types}.ts` 与测试。
- 文档与实现不一致时先判断哪边过期；改契约先改 docs，再改代码与测试。
