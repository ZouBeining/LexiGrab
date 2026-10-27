# AGENTS.md

本文件是 AI 编码代理的操作性约束。设计目标、架构、API 契约、里程碑见 [PROMPT.md](PROMPT.md)，此处不重复。

## 仓库落位

新增代码必须落入既有层级：Rust 侧 `src-tauri/src/{core,sources,parsers,anki,platform,bridge}`；前端 `src/windows/{popup,manager,settings}` 与 `src/lib`；扩展 `extension/entrypoints`。不得在根目录随意新建目录。

## 前端

前端设计须参考 [DESIGN.md](DESIGN.md)。

## 常用命令

- 安装依赖：`pnpm install`
- 桌面开发：`pnpm tauri dev`
- 扩展开发：`cd extension && pnpm wxt -b firefox`
- Rust 测试：`cd src-tauri && cargo test`
- 前端检查：`pnpm typecheck && pnpm lint`
- 打包扩展：`cd extension && pnpm wxt build -b firefox && web-ext build`

提交前必须通过 `cargo test` 与 `pnpm typecheck && pnpm lint`。

## 包管理

前端与扩展统一使用 **pnpm**。添加开发依赖用 `pnpm add -D <pkg>`；workspace 根目录加 `-w`。不要混用 npm/yarn，不要提交非 pnpm 的锁文件。

## 版本控制

`Cargo.lock` 必须提交。不要提交 `.env`、`node_modules/`、`target/`、`.wxt/`、`.output/`、`dist/`。不要手动编辑 `src-tauri/gen/`、`.wxt/`、`.output/` 等生成目录。

## 编码细则

- Rust 非测试代码禁用 `unwrap()` / `expect()`。
- TypeScript 禁用 `any`；IPC 边界类型只从 `src/lib` 导入。
- 样式只用 Tailwind 工具类，不写内联 `style`。
- 前端与扩展不得直接调用 AnkiConnect 或远程词典，一律经 Tauri Command 或本地桥接。
- `FormatParser::lookup(&self)` 如需缓存，用 `Mutex`/`RwLock` 内部可变性，不改 trait 签名。
- `Entry.source_name` 由 `RemoteHttpSource` 统一填充，解析器不返回该字段。

## 提交

Conventional Commits；一个 commit 只动一个模块，Rust 与前端改动分开提交。

## 禁止

- 不得偏离 `PROMPT.md` 已确定的技术栈与许可证（AGPL-3.0）。
- 引入新依赖前检查许可证兼容性，不引入不兼容 copyleft 依赖。
- 不得把本地 API 绑定到 `0.0.0.0` 或省略 `X-Auth-Token` 校验。
- 不得在远程源失败时抛出阻断性错误或阻塞 UI。
- 禁止修改 `PROMPT.md`。

## 文档

项目中应在 docs/ 目录下应当维护如下文档。

1. **PRD.md（产品需求文档）**：完整规格。什么在范围内、什么明确不在。用户故事、成功标准。

1. **APP_FLOW.md**：每个页面和用户导航路径。逐步序列与决策点、成功/错误状态、屏幕清单与路由。防止 AI 猜测用户如何移动。

1. **TECH_STACK.md**：每个包和依赖锁定到确切版本。不写"使用 React"，写"Next.js 14.1.0", "React 18.2.0", "TypeScript 5.3.3"，诸如此类。一行一个，消除幻觉依赖。这不是项目真正的依赖管理（真正的管理应该靠 `package.json`，`Cargo.toml` 等配置文件），只是一份给人看的速查文档。

1. **FRONTEND_GUIDELINES.md**：完整设计系统。对 `.agents/design` 下的 DESIGN.md 进行补充，已有内容不许复制，而应链接。若 `.agents/design` 目录下有不止一个 DESIGN.md 文档，则须让用户选择其中一个作为 UI 设计的主参考，界面布局、响应式断点等均按照该主参考设计；其他 DESIGN.md 主要参考其配色，做成可替换的主题，

1. **BACKEND_STRUCTURE.md**：数据库模式（每张表、每列、类型、关系）、认证逻辑、API 端点合约、存储规则、边缘情况。

在项目根目录下维护如下文档。

1. **AGENTS.md（≤60 行，精简！）**：AI 每次会话自动首先读取。规则、约束、模式。技术栈摘要、命名约定、组件模式、设计令牌。允许、禁止。AGENTS.md 是活文档——每次用户纠正 AI 时更新它。

在 .agents/plans/ 目录下维护如下文档。

1. **PLAN_Mx.md（详细计划）**（`x` 为数字）：范围与不做项、目录落位、任务分解、接口/数据结构、测试与手工验收矩阵、性能目标、风险对策、Conventional Commits 拆分、出口检查清单和下一阶段交接。对于前述各个文档中的已有内容不许复制，而应链接。在真正开始工作前编写好，此后慎重更改。

1. **PROGRESS.md（会话桥梁）**：已完成/进行中/接下来/已知 Bug。每次功能完成更新。新会话 AI 首先读这个——没有它，每个新会话从零上下文开始。有它，AI 精确从断点继续。
