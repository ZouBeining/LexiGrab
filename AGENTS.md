# AGENTS.md

本文件是 LexiGrab 的总纲：入口、硬约束与验证门禁。接手顺序：本文件 → [PROMPT.md](PROMPT.md)（实现路径与细则）→ [PROGRESS.md](.agents/plans/PROGRESS.md)（断点）→ 当前阶段 [PLAN_Mx](.agents/plans/)。产品与交互以 [docs/](docs/) 为准；版本与技术事实以代码、`Cargo.toml` 和锁文件为准。

技术栈：Tauri 2 + Rust + React 19 + TypeScript + Tailwind 4；Firefox 扩展用 WXT + MV2。许可证 AGPL-3.0。

## 文档地图

- 给人看：[PRD](docs/PRD.md)、[APP_FLOW](docs/APP_FLOW.md)、[ARCHITECTURE](docs/ARCHITECTURE.md)、[BACKEND_STRUCTURE](docs/BACKEND_STRUCTURE.md)、[TECH_STACK](docs/TECH_STACK.md)、[ROADMAP](ROADMAP.md)。
- 给 agent 看：本文件、[PROMPT.md](PROMPT.md)、`.agents/plans/`、`.agents/notes/`、`.agents/design/`。
- 字段级契约只写在 docs；PROMPT 只摘要命令名、事件名与不变式，计划只链接。改契约先改 docs，再改代码与测试。

## 会话启动

1. 读 PROMPT.md 的“接手清单”，确认当前阶段、门禁与已知陷阱。
2. 读 PROGRESS.md 的“进行中 / 已知问题”，再读当前 PLAN。
3. 先用搜索工具定位再定点读代码，禁止全仓库通读；不要扫描 `.agents/automation/*/artifacts`。

## 不可违反

- 目录：Rust 进 `src-tauri/src/{core,sources,parsers,anki,db,bridge,platform}`；前端进 `src/windows/{popup,manager,settings}`、`src/lib`、`src/components`；扩展进 `extension/entrypoints`、`extension/lib`。根目录不新建目录。
- 前端：只用 Tailwind 工具类；不写内联 `style`；不引入组件库/图标包；IPC 类型只从 `src/lib` 导入，组件不直接 `invoke`；页面必须有 loading/empty/error 三态；主题只换 CSS 变量。
- Rust/TS：非测试 Rust 禁 `unwrap()/expect()`；TS 禁 `any`；IPC 错误统一经 `IpcError` 边界。
- 安全：桥接只绑 `127.0.0.1`，`X-Auth-Token`、Origin 与限流不可绕过；API key 只读环境变量；日志不记令牌、完整原句或 API key。
- 失败不得阻断：远程源与语境辅助失败只影响自身；AI 释义必须标注模型并由用户确认；推荐不得静默替换词典义项。
- 同词多卡：Anki 恒 `allowDuplicate: true`；应用层只提示“同词同句同释义”。
- 前端与扩展不得直连 AnkiConnect 或远程词典，一律经 Tauri Command / 本地桥接。
- `references/*` 只有 README 入库；ODH 只研究，不复制代码。

## 验证门禁

提交前必须通过：

```text
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cd src-tauri && cargo deny check licenses bans sources
pnpm typecheck && pnpm lint && pnpm test
pnpm -C extension typecheck && pnpm -C extension test
```

- 远程源、缓存、语境：另跑对应 `src-tauri/tests/*` 集成测试。
- 打包：`pnpm -C extension build:firefox && pnpm -C extension lint:firefox`。

## 变更与提交

- 一个 commit 只动一个模块（Rust / 前端 / 扩展 / 文档分开）；Conventional Commits，message 全英文。
- 不引入偏离技术栈或 AGPL-3.0 的依赖；直接依赖升级单独提交并同步 TECH_STACK。
- 功能完成同步更新 PROGRESS 与相关 docs；用户纠正一次就更新本文件，保持 ≤60 行，细节放 PROMPT。