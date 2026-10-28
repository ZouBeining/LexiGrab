# PROGRESS

最后更新：2026-10-28。新会话先读本文件，再读当前阶段计划。

## 已完成

- 仓库规划文档：[PRD](../../docs/PRD.md)、[APP_FLOW](../../docs/APP_FLOW.md)、[TECH_STACK](../../docs/TECH_STACK.md)、[FRONTEND_GUIDELINES](../../docs/FRONTEND_GUIDELINES.md)、[BACKEND_STRUCTURE](../../docs/BACKEND_STRUCTURE.md)。
- 六个阶段的计划：[M0](PLAN_M0.md)、[M1](PLAN_M1.md)、[M2](PLAN_M2.md)、[M3](PLAN_M3.md)、[M4](PLAN_M4.md)、[M5](PLAN_M5.md)。
- 参考材料目录与 [references/README.md](../../references/README.md)；ODH 已浅克隆到 `references/ODH`（commit `ce5e7da`，MIT），被 `.gitignore` 排除。
- AGENTS.md 精简到 60 行以内，加入 docs、PROGRESS、figma 主参考与 references 规则。
- 设计主参考已确认：`.agents/design/figma.md`；Apple/Cursor 仅作可替换配色主题。

## 进行中

- 规划阶段收尾：把 PLAN_M0–M5 对齐到 docs，去掉重复契约，补 ODH 研究任务。
- 尚未开始任何产品代码；仓库仍没有 `package.json`、`src-tauri/`、`extension/`。

## 接下来

1. 完成计划对齐与提交。
2. 按 [PLAN_M0.md](PLAN_M0.md) 搭 Tauri 2 + React + Tailwind 骨架。
3. M0 技术验证：热键、选区捕获、非激活弹窗。
4. 在 M0 报告里记录 Windows 焦点、剪贴板、DPI 结果。

## 已知问题与阻塞

- 工作区 Windows 文件权限曾阻止 `git`/`pwsh`；已通过权限修复脚本补齐当前用户完全控制并验证。恢复命令与报告在仓库外的 `..\LexiGrab-acl-recovery\`。
- Git 直接使用 Schannel 克隆 GitHub 会报 `SEC_E_NO_CREDENTIALS`；使用 `git -c http.sslBackend=openssl` 可克隆。
- 没有产品代码，因此 `cargo test` 与 `pnpm typecheck` 暂无可执行对象；文档提交不以此为前提。

## 关键决策

- UI 主参考 figma；0.1.0 只做三套浅色主题（figma/apple/cursor），不做深色。
- 主题只换 CSS 变量，布局与组件结构固定按 figma。
- 浏览器扩展只做 Firefox，`.xpi` 未签名分发。
- 远程解析同时支持 JSONPath 与脚本；脚本 200ms/16MB 沙箱。
- ODH 只研究，不复制代码。
