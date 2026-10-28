# PROGRESS

最后更新：2026-10-28。新会话先读本文件，再读当前阶段计划。

## 已完成

- 仓库规划文档：[PRD](../../docs/PRD.md)、[APP_FLOW](../../docs/APP_FLOW.md)、[TECH_STACK](../../docs/TECH_STACK.md)、[FRONTEND_GUIDELINES](../../docs/FRONTEND_GUIDELINES.md)、[BACKEND_STRUCTURE](../../docs/BACKEND_STRUCTURE.md)。
- 六个阶段的计划：[M0](PLAN_M0.md)、[M1](PLAN_M1.md)、[M2](PLAN_M2.md)、[M3](PLAN_M3.md)、[M4](PLAN_M4.md)、[M5](PLAN_M5.md)。
- 参考材料目录与 [references/README.md](../../references/README.md)；ODH 已浅克隆到 `references/ODH`（commit `ce5e7da`，MIT），被 `.gitignore` 排除。
- 设计主参考已确认：`.agents/design/figma.md`；Apple/Cursor 仅作可替换配色主题。
- M0 工程骨架：Tauri 2 + React 19 + TypeScript + Tailwind 4，三个窗口入口（manager/popup/settings），Figma 三主题 token，ESLint 禁 `any` 与内联 style，CI 工作流。
- M0 划词链路：全局热键 + 250ms 去抖、Windows 剪贴板选区捕获（文本/锚点/前台进程，180ms 预算）、非激活置顶弹窗（NOACTIVATE + TOOLWINDOW，点击不抢焦点）、编辑模式激活、多屏 clamp、文件日志与诊断统计。
- M0 报告：[M0-selection-capture](../notes/M0-selection-capture.md)。
- 新增需求文档化：JEV-like 义项选择 + 低置信度 LLM 兜底；同词多卡（2026-10-28，仅改 docs/plans，未改代码）。
- 当前代码检查：`cargo test` 17 项通过；`pnpm typecheck`、`pnpm lint` 通过。M0 代码不涉及 MDX/Anki/语境辅助，因此本轮无需改代码。

## 进行中

- 新增需求的文档与计划已更新：PRD、APP_FLOW、BACKEND_STRUCTURE、TECH_STACK、FRONTEND_GUIDELINES、AGENTS、PLAN_M1–M5。
- M0 收尾：T0.7 手工应用矩阵（Notepad / Chrome / Firefox / VS Code 等）待用户在真实桌面补测，写入报告第 6 节。
- CI 文件已提交，GitHub Actions 尚未实跑。

## 接下来

1. 提交本轮文档变更，保持单模块原子提交。
2. 补 T0.7 手工矩阵与双屏/150% 实机检查，回填 M0 报告。
3. 进入 [PLAN_M1.md](PLAN_M1.md)：配置持久化、MDX 读取、义项提取与 Anki 四字段；多卡片在 M2 落地。
4. M1 开始前把 `docs/` 与 `src/lib` 的 IPC 类型对齐一次（M0 新增的 `get_last_capture` / `popup://capture-error` 是临时诊断入口）。

## 代码影响检查（本轮未修改代码）

当前已有 M0 代码，尚未实现 MDX、Anki、设置持久化、桥接或扩展。新需求暂时不需要改动 M0 代码，但后续实现必须改这些位置：

- `src-tauri/src/platform/selection.rs`：`SelectionContext` 目前没有 `sentence` / `url` / `title` / `domain`；M2 需加可选字段并保持 serde 向后兼容。
- `src-tauri/src/core/config.rs`：目前只有 `Paths`；M1 需加完整 `Settings`，并新增 `[sense]`、`[llm]`、`duplicate_policy`。
- `src-tauri/src/core/state.rs`、`commands.rs`、`lib.rs`：M2/M4 需加 lookup、sense、LLM、saved_cards 相关命令与状态；目前只有 M0 捕获命令。
- `src-tauri/Cargo.toml`：M1 加 `scraper`、MDX/Anki 依赖；M4 加选择器/LLM HTTP 与缓存依赖。
- `src/lib/types.ts`、`events.ts`、`src/windows/popup/App.tsx`、`src/windows/settings/App.tsx`：需加 `SenseResolution`、`AiDefinition`、推荐/AI/多卡 UI 与“语境辅助”设置分区；目前是 M0 占位。
- 扩展目录尚不存在；M3 再按扩展桥接与 sense 轮询实现。
- 数据库尚不存在；M2 加 `saved_cards`，M4 加 `sense_cache`。

## 已知问题与阻塞

- 本机 Windows Defender 开启“受控文件夹访问”：从本会话沙箱启动的未签名调试程序无法写 `%APPDATA%` / `%LOCALAPPDATA%`。验证用 `LEXIGRAB_HOME` 指向工作区内目录；用户在自己终端运行时若仍失败，需要把 `lexigrab.exe` 加入白名单。
- 工作区沙箱下 cargo 走 Schannel 会报 `SEC_E_NO_CREDENTIALS`；需要在可联网的普通终端 `cargo fetch`/`build`。`git` 用 `-c http.sslBackend=openssl` 可克隆 GitHub。
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
