---
phase: M0
title: 技术风险验证
depends_on: []
exit_gate: Windows 上热键、选区捕获、非激活弹窗三项同时成立
estimate: 5-8 人日
status: done
---

# M0 技术风险验证

M0 只证明 Windows 上的划词链路成立：按热键，拿到选中文本与屏幕坐标，在附近显示一个不抢焦点的窗口，点击窗口按钮时原应用仍是前台。MDX、Anki、远程源、扩展都不在本阶段。

## 全局依据

- 产品范围：[PRD](../../docs/PRD.md)
- 页面与焦点流程：[APP_FLOW](../../docs/APP_FLOW.md)
- 版本与工具链：[TECH_STACK](../../docs/TECH_STACK.md)
- 设计主参考：[figma](../design/figma.md)；实现规则：[FRONTEND_GUIDELINES](../../docs/FRONTEND_GUIDELINES.md)
- 接口与错误契约：[BACKEND_STRUCTURE](../../docs/BACKEND_STRUCTURE.md)
- 会话断点：[PROGRESS](PROGRESS.md)

本计划只保留阶段任务、风险与验收；全局契约以 docs 为准，不在此复制。

## 1. 范围

**做**

- Tauri 2 + React 19 + TypeScript + Tailwind 4 骨架。
- manager / popup / settings 三个窗口入口，默认只显示 manager。
- 全局快捷键注册、冲突报错、250ms 去抖。
- Windows 选区捕获：文本、锚点、前台窗口、捕获方式。
- 非激活弹窗：置顶、无边框、不进 Alt+Tab、不抢焦点。
- 编辑模式焦点切换实验。
- 多显示器、DPI、剪贴板恢复、隐私日志。
- 最小 CI。
- 技术验证报告 `.agents/notes/M0-selection-capture.md`。

**不做**

- MDX/Anki/远程源/桥接/扩展。
- 完整设置与源管理界面。
- macOS/Linux 实现。
- UIA 句子捕获正式实现，只做可行性测量。
- 自动更新、签名、开机自启。

## 2. 目录落位

```text
package.json / pnpm-lock.yaml / tsconfig.json / vite.config.ts / eslint.config.js
manager.html / popup.html / settings.html
src/lib/{ipc.ts,types.ts,theme.ts}
src/styles/tailwind.css
src/windows/{popup,manager,settings}/
src-tauri/{Cargo.toml,Cargo.lock,build.rs,tauri.conf.json,capabilities,icons,src}
src-tauri/src/{main.rs,lib.rs,commands.rs,core/{config.rs,window.rs},platform/{mod.rs,windows/*}}
.github/workflows/ci.yml
.agents/notes/M0-selection-capture.md
```

细节目录可按实现微调，但必须落在 [AGENTS.md](../../AGENTS.md) 的层级内。

## 3. 任务分解

### T0.1 工程骨架

1. pnpm 项目，脚本 `dev`、`build`、`typecheck`、`lint`、`tauri`、`test`。
2. Vite 多入口：`manager.html`、`popup.html`、`settings.html`；端口 1420，`strictPort`。
3. Tailwind 4 使用 `@tailwindcss/vite`；token 按 [FRONTEND_GUIDELINES](../../docs/FRONTEND_GUIDELINES.md) 接入，不在本计划写色值。
4. TypeScript strict + `noUncheckedIndexedAccess`；ESLint 禁用 `any` 与 JSX 内联 `style`。
5. Tauri 2.12.5 + `tauri-plugin-global-shortcut` 2.4.1；Rust 1.99.1。
6. capabilities 只给 `core:default` 与 `global-shortcut:default`。
7. 生成占位图标，提交 `src-tauri/icons/`。
8. 提交 `Cargo.lock` 与 `pnpm-lock.yaml`。

**验收**：`pnpm install`、`pnpm typecheck`、`pnpm lint`、`cargo check` 通过；`pnpm tauri dev` 打开 manager。

### T0.2 窗口与路由

- manager 980×700，最小 900×600；settings 900×700；popup 420×560，无边框、置顶、跳过任务栏、初始隐藏、不可缩放。
- Rust 提供 `show_window(label)`；已存在则显示，不存在则创建。
- 前端每个入口单独挂载，不引入路由库；分区参数按 [APP_FLOW](../../docs/APP_FLOW.md)。
- 关闭 manager/settings 为隐藏；退出走托盘或开发命令。
- 样式按 [FRONTEND_GUIDELINES](../../docs/FRONTEND_GUIDELINES.md)，禁止内联 style。

**验收**：三个窗口可开关复用；manager 关闭后进程与热键仍在；窗口尺寸与设计一致。

### T0.3 全局快捷键

- 默认 `CmdOrCtrl+Shift+D`，只处理 Pressed。
- 250ms 内重复触发忽略。
- 注册失败写进运行状态，manager 显示“快捷键被占用”。
- 抽出 `register_hotkey(app, accelerator)`，供 M2 修改热键复用。
- 提供 `debug_trigger_capture` 命令，便于无热键环境测试。

**验收**：记事本、Chrome、VS Code 各触发一次；占用冲突不崩溃；连按 5 次最多触发 1 次。

### T0.4 Windows 选区捕获

实现 `SelectionHook`，返回文本、锚点、前台 HWND、应用名、捕获方式。

1. 捕获开始时记录前台窗口、线程 id、进程可执行文件名；不记录窗口标题。
2. 锚点：`GetGUIThreadInfo` 的 `rcCaret` 右下角转屏幕坐标；失败退 `GetCursorPos`；都失败返回 `NoAnchor`。
3. 文本：保存 `CF_UNICODETEXT` 与剪贴板序列号；`SendInput` 发 Ctrl+C；5ms 轮询序列号，最多 250ms。
4. 仅在剪贴板内容仍等于本次捕获文本时恢复原文本；原本没有文本时不清空，避免破坏图片/文件列表。
5. 剪贴板占用时 `OpenClipboard` 重试 5 次，每次 10ms。
6. 清理首尾空白与引号，压缩空白，保留词内连字符/撇号；超过 200 字符截断，只记录长度。
7. 总预算 180ms；超时返回可重试错误。
8. UIA 只做可行性测量：TextPattern 取选区、段落扩展取句、耗时、Chrome/Word/VS Code 命中率。

**验收**：必测应用返回正确文本与锚点；剪贴板文本恢复；管理员应用返回可读错误；纯函数测试覆盖清理、clamp、超时。

### T0.5 非激活弹窗

1. 通过 `HasWindowHandle` / `raw-window-handle` 取 HWND；若是 WebView2 子窗口，用 `GetAncestor(GA_ROOT)`。
2. 追加 `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`，去掉 `WS_EX_APPWINDOW`；`SetWindowPos` 带 `SWP_NOACTIVATE`。
3. `SetWindowSubclass` 处理 `WM_MOUSEACTIVATE → MA_NOACTIVATE`，转发其余消息，销毁时移除。
4. 显示流程：隐藏时定位/设尺寸 → `ShowWindow(SW_SHOWNOACTIVATE)` → 置顶且不激活。不调用 `set_focus()`。
5. 编辑模式用 `set_interactive(true)` 清除 NOACTIVATE，`SetForegroundWindow`；被前台锁阻止时用 `AttachThreadInput` 重试。
6. 关闭：热键切换、关闭按钮；点击外部关闭留到 M2。
7. 定位按锚点、工作区、DPI 换算；副屏负坐标不得被 clamp 到主屏。
8. 透明/阴影各做一次实验，记录 Win10/11 表现后定默认。

**验收**：弹窗显示前后 `GetForegroundWindow()` 不变；点击按钮后直接在记事本输入，字符进入记事本；不出现在 Alt+Tab；编辑模式可输入，Esc 后焦点回原窗口；150% 与双屏不越界。

### T0.6 诊断与日志

- `tracing` + 文件日志到 `%LOCALAPPDATA%\LexiGrab\logs\`。
- 打点：热键、捕获完成、窗口显示、前端首帧；开发构建显示耗时浮层。
- 日志不写选中文本、剪贴板内容、窗口标题；只写长度、方法、应用名、耗时、错误码。
- manager 放“捕获测试”面板，显示最近一次结果与重新捕获。

**验收**：20 次捕获有 p50/p95；日志搜不到测试词；失败有可读提示。

### T0.7 手工矩阵

必测：记事本、Chrome、Firefox、VS Code。选测：Word、Windows Terminal、资源管理器重命名框、Excel 单元格。每个 3 次，记录文本、锚点、焦点、失败码。矩阵与结论写入报告。

### T0.8 CI

- 前端：Node 24、pnpm 11，`pnpm install --frozen-lockfile`、typecheck、lint。
- Rust：windows-latest，fmt、clippy `-D warnings`、test；启用缓存。
- M0 不构建安装包。

**验收**：故意提交未格式化文件与 `any` 均失败；正常提交全绿。

### T0.9 报告

`.agents/notes/M0-selection-capture.md` 包含环境、捕获路径对比、焦点实验、透明/阴影结论、延迟、应用矩阵、D0-1 至 D0-4 决策、给 M1/M2 的交接。

## 4. 阶段决策

| 编号 | 问题 | 默认 | 改变条件 |
| --- | --- | --- | --- |
| D0-1 | 快速模式永远非激活？ | 是；编辑模式才激活 | 激活成功率 <90% 时改为可聚焦弹窗，仍用 popup label |
| D0-2 | 文本主路径 | 剪贴板为基线，UIA 测量 | UIA 在 3/4 必测应用稳定且 p95 <80ms，M2 可改 UIA 优先 |
| D0-3 | 透明窗口 | 先实验，默认不透明 | 黑边/拖影/点击异常时保持不透明 |
| D0-4 | 点击外部关闭 | M0 不做 | 低级鼠标钩子开销 <1ms 时 M2 增加 |

## 5. 风险

| 风险 | 对策 |
| --- | --- |
| NOACTIVATE 与 WebView2 冲突 | 第一天验证；失败回退普通置顶窗口，关闭时恢复焦点 |
| 剪贴板副作用 | 只恢复文本；原剪贴板无文本时不写回；报告声明 |
| 管理员应用拦截 SendInput | 返回 `TargetElevated`，提示同权限运行 |
| Ctrl+C 无响应 | 180ms 预算、可重试；UIA 作为后续优化 |
| 透明窗口黑边 | 默认不透明 |
| 多屏负坐标 | 工作区 clamp 单元测试 |
| WebView2 首启慢 | 启动时预创建隐藏 popup（M2） |

## 6. 测试命令

```powershell
pnpm install
pnpm typecheck
pnpm lint
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cd ..
pnpm tauri dev
```

## 7. 提交拆分

```text
chore(repo): scaffold tauri react tailwind project
feat(platform): add windows selection capture
feat(platform): add non activating popup window
feat(core): register global shortcut and window factory
feat(popup): add capture diagnostics view
feat(manager): add verification panel
ci: add frontend and rust checks
docs(notes): record m0 selection capture findings
```

## 8. 出口检查清单

- [ ] manager 可启动，三个入口存在。
- [ ] 默认热键在 4 个必测应用触发。
- [ ] 捕获返回文本、锚点、前台窗口，p95 ≤180ms。
- [ ] 弹窗显示后前台窗口不变，点击按钮焦点不变。
- [ ] 编辑模式可输入并恢复焦点，或已按 D0-1 记录替代方案。
- [ ] 双屏与 150% 缩放下定位正确。
- [ ] 日志不含选中文本。
- [ ] CI、`cargo test`、typecheck、lint 全绿。
- [ ] 报告完成，D0 决策已定。

## 9. 交接给 M1

M1 复用：SelectionHook DTO、配置路径与原子写入、窗口工厂、日志/打点、CI。M0 报告必须写清剪贴板恢复行为、NOACTIVATE 样式值、编辑模式激活成功率、透明窗口结论。
