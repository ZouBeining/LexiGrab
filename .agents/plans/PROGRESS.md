# PROGRESS

最后更新：2026-10-28。新会话先读本文件，再读当前阶段计划。

## 已完成

- 仓库规划文档：[PRD](../../docs/PRD.md)、[APP_FLOW](../../docs/APP_FLOW.md)、[TECH_STACK](../../docs/TECH_STACK.md)、[FRONTEND_GUIDELINES](../../docs/FRONTEND_GUIDELINES.md)、[BACKEND_STRUCTURE](../../docs/BACKEND_STRUCTURE.md)。
- 六个阶段的计划：[M0](PLAN_M0.md)、[M1](PLAN_M1.md)、[M2](PLAN_M2.md)、[M3](PLAN_M3.md)、[M4](PLAN_M4.md)、[M5](PLAN_M5.md)。
- 参考材料目录与 [references/README.md](../../references/README.md)；ODH 已浅克隆到 `references/ODH`（commit `ce5e7da`，MIT），被 `.gitignore` 排除。
- 设计主参考已确认：`.agents/design/figma.md`；Apple/Cursor 仅作可替换配色主题。
- M0 工程骨架：Tauri 2 + React 19 + TypeScript + Tailwind 4，三个窗口入口（manager/popup/settings），Figma 三主题 token，ESLint 禁 `any` 与内联 style，CI 工作流。
- M0 划词链路：全局热键 + 250ms 去抖、Windows 剪贴板选区捕获（文本/锚点/前台进程，180ms 预算）、非激活置顶弹窗（NOACTIVATE + TOOLWINDOW，点击不抢焦点）、编辑模式激活、多屏 clamp、文件日志与诊断统计。
- M0 报告：[M0-selection-capture](M0-selection-capture.md)。

## 进行中

- M0 收尾：T0.7 手工应用矩阵（Notepad / Chrome / Firefox / VS Code 等）待用户在真实桌面补测，写入报告第 6 节。
- CI 文件已提交，GitHub Actions 尚未实跑。

## 接下来

1. 补 T0.7 手工矩阵与双屏/150% 实机检查，回填 M0 报告。
2. 进入 [PLAN_M1.md](PLAN_M1.md)：配置持久化、MDX/远程源、缓存与查询路由。
3. M1 开始前把 `docs/` 与 `src/lib` 的 IPC 类型对齐一次（M0 新增的 `get_last_capture` / `popup://capture-error` 是临时诊断入口）。

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
- ODH 只研究，不复制代码。
- 三个窗口启动时预创建（popup/settings 隐藏）：Tauri 在命令线程里构建 WebView 会阻塞事件循环，懒创建不可用。
- 快速模式用纯 Win32 `ShowWindow(SW_SHOWNOACTIVATE)` 显示，不走 tao `show()`；弹窗隐藏用 `SW_HIDE`，不然 tao 状态不一致导致空操作。
