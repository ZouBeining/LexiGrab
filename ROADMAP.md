# ROADMAP

LexiGrab 0.1.0 是 Windows 优先的桌面划词词典：任一应用中选中单词，按一次快捷键查词，并把词、释义、原句和来源写入 Anki。

本文件回答“有哪些组件、分别在哪、做到什么程度”。产品范围见 [PRD](docs/PRD.md)，架构说明见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)，接口契约见 [docs/BACKEND_STRUCTURE.md](docs/BACKEND_STRUCTURE.md)，实现路径见 [PROMPT.md](PROMPT.md)。

## 当前状态

- M0–M4 主体已完成并留有验收记录，M0 的手工应用矩阵仍可补测；M5（打磨与发布）尚未开始。
- 已可用：本地 MDX/MDD 词典、远程 HTTP 词典、三种查询策略、SQLite 缓存、语境义项推荐与可选 AI 释义、AnkiConnect 制卡、Firefox 扩展与本地桥接。
- 未完成：M5 的视觉与文案收尾、许可证清单、总 README、发布流程。本文不展开 M5 任务。

## 组件地图

| 组件 | 位置 | 职责 | 状态 |
| --- | --- | --- | --- |
| 桌面壳与生命周期 | `src-tauri/src/lib.rs`、`main.rs`、`src-tauri/tauri.conf.json` | 单实例、托盘、窗口预创建、资源协议注册、CSP、启动后台任务 | 完成 |
| 窗口、热键与捕获 | `src-tauri/src/core/window.rs`、`core/shortcut.rs`、`src-tauri/src/platform/` | 三窗口显示/隐藏、全局热键、Windows UI Automation 选区与光标定位 | 完成（Windows） |
| 配置与运行时状态 | `src-tauri/src/core/config.rs`、`core/state.rs`、`core/runtime.rs`、`core/logging.rs` | 配置 schema 与迁移、原子保存、`AppState` 状态中心、日志 | 完成 |
| 词典源 | `src-tauri/src/sources/`、`core/registry.rs` | 本地文件源与远程 HTTP 源、加载/隔离/重载、源状态 | 完成 |
| 解析器 | `src-tauri/src/parsers/`、`core/html.rs`、`core/resources.rs` | MDX/MDD、JSONPath、QuickJS 脚本、HTML 清洗、义项提取、媒体改写 | 完成 |
| 查询调度 | `src-tauri/src/core/query_router.rs` | `local_first` / `first_hit` / `aggregate`、缓存优先、远程后台任务 | 完成 |
| 缓存与卡片指纹 | `src-tauri/src/db/` | `lookup_cache`、`sense_cache`、`saved_cards`；WAL 与 schema 版本 | 完成 |
| 语境辅助 | `src-tauri/src/core/sense/`、`core/events.rs` | 候选义项选择、置信度阈值、LLM 兜底、异步事件 | 完成 |
| Anki 集成 | `src-tauri/src/anki/` | AnkiConnect 客户端、字段映射、媒体剥离、同语境判定 | 完成 |
| 本地桥接 | `src-tauri/src/bridge/` | axum `127.0.0.1` 服务、令牌/Origin/限流、扩展端点 | 完成 |
| 桌面前端 | `src/windows/`、`src/components/`、`src/lib/`、`src/styles/` | 弹窗、管理器、设置三窗口；IPC 类型、事件、主题 | 完成 |
| Firefox 扩展 | `extension/entrypoints/`、`extension/lib/` | content 划词与句子提取、background 持有令牌、options 配对 | 完成（未签名 `.xpi`） |
| 开发 CLI | `src-tauri/src/bin/lexigrab-dev.rs` | 无 UI 地查词、列源、Anki 探测与制卡、基准 | 完成 |
| 测试与夹具 | `src-tauri/tests/`、`src-tauri/tests/fixtures/`、`.agents/automation/` | 集成测试、MDX 夹具、Python 验收与 mock 服务 | 完成，按阶段维护 |
| 文档与规划 | `docs/`、`.agents/{plans,notes,design,skills}`、根目录 `AGENTS.md`/`PROMPT.md` | 产品契约、阶段计划、验收记录、设计参考、agent 技能 | 持续维护 |

## 架构简图

```text
热键 → 选区捕获 → 弹窗 → lookup 命令 → QueryRouter → 词典源 → 解析器 → Entry[]
                                             ↘ 缓存           ↘ 远程事件追加
弹窗 → 语境辅助 → 用户确认 → export_to_anki → AnkiConnect

Firefox content script → background（持令牌）→ 本地桥接 → AppState → 查询 / 语境 / 制卡
```

## 里程碑

| 阶段 | 交付 | 状态 |
| --- | --- | --- |
| M0 | Tauri 窗口、非激活弹窗、选区捕获 | 完成；手工矩阵仍可补测 |
| M1 | 无 UI 的 MDX 查询、义项提取、AnkiConnect 制卡、开发 CLI | 完成 |
| M2 | 桌面 MVP：弹窗、源管理、设置、同词多卡 | 完成；缺陷已修复 |
| M3 | Firefox 扩展、本地桥接、令牌与配对 | 完成；真机 29/29 |
| M4 | 远程源、JSONPath/脚本解析、缓存、语境辅助 | 完成 |
| M5 | 视觉与文案收尾、许可证与文档、发布流程 | 未开始 |

阶段细节与出口条件见 [.agents/plans/](.agents/plans/)，当前断点见 [PROGRESS.md](.agents/plans/PROGRESS.md)。

## 近期方向

1. 先完成 M5 的发布门槛，不提前扩张平台或功能范围。
2. 保持 Windows 与 Firefox 的验收；其他平台只保留抽象，不承诺支持。
3. 新增格式、数据源或 UI 功能时，先改 docs 契约，再改实现与测试。
4. 扩展测试目前不在 CI 中，发布前需要纳入或补手工证据。

## 相关文档

- 产品范围与成功标准：[docs/PRD.md](docs/PRD.md)
- 页面与状态：[docs/APP_FLOW.md](docs/APP_FLOW.md)
- 架构与数据流：[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- 数据、IPC、桥接契约：[docs/BACKEND_STRUCTURE.md](docs/BACKEND_STRUCTURE.md)
- 版本与依赖：[docs/TECH_STACK.md](docs/TECH_STACK.md)
- agent 实现路径：[PROMPT.md](PROMPT.md)