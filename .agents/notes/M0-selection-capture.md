# M0 技术验证报告：划词链路

记录日期：2026-10-28。对应计划：[PLAN_M0](../plans/PLAN_M0.md)。本文件只记录 M0 的验证结果与决策，不复制 docs 中的契约。

## 1. 环境

| 项 | 值 |
| --- | --- |
| 系统 | Windows 11（本机） |
| Node / pnpm | 24.22.1 / 11.28.5 |
| Rust | 1.99.1（MSVC） |
| Tauri | 2.12.5（CLI 2.12.4） |
| WebView2 | 系统运行时，Chromium 内核 |
| 主题 | Figma（默认），另含 apple / cursor token |

自动化验证方式：`debug_trigger_capture` / `show_window` 通过 WebView2 远程调试（CDP）调用，配合 `GetForegroundWindow`、`GetWindowLongW(GWL_EXSTYLE)`、鼠标注入与 `%LOCALAPPDATA%\LexiGrab\logs` 记录交叉检查。Debug 构建默认加载 `devUrl`，验证时先启动 `pnpm dev`。

## 2. 选区捕获（T0.4）

实现：`src-tauri/src/platform/windows/capture.rs`，事件链 `capture_selection()`；纯函数 `normalize_captured_text()` 在 `platform/selection.rs`。

路径：

1. `GetForegroundWindow` → `GetWindowThreadProcessId` → `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` + `QueryFullProcessImageNameW` 取进程名（不读窗口标题）。
2. 锚点：`GetGUIThreadInfo(thread_id).rcCaret` 右下角 `ClientToScreen`；`hwndCaret` 为空退 `GetCursorPos`；都失败返回 `anchor = null`。锚点为物理屏幕像素，另带 `dpi_scale`。
3. 文本：记录 `CF_UNICODETEXT` 与剪贴板序列号 → 发送 Ctrl+C → 每 5ms 轮询序列号，预算 180ms。热键触发时 Shift 可能仍按下，先单独发一次 Shift keyup，再发 Ctrl+C。
4. 恢复：仅当剪贴板内容仍等于本次捕获文本且原本有文本时写回原文本；原本无文本时不清空，保留复制结果（报告已知副作用）。
5. 剪贴板占用：`OpenClipboard` 5 次 × 10ms 重试。
6. 清理：去首尾空白与引号、压缩空白、保留词内连字符/撇号，截断 200 字符。
7. 错误码：`CAPTURE_TIMEOUT`（可重试）、`CAPTURE_EMPTY`、`CAPTURE_CLIPBOARD_BUSY`、`TARGET_ELEVATED`、`CAPTURE_NO_FOREGROUND`。`access_denied` 时超时归因为 `TARGET_ELEVATED`。

实测（manager 页面选中 95 字符后触发，n=6）：

| 指标 | 值 |
| --- | --- |
| p50 | 33–38 ms |
| p95 | 42 ms |
| 采样上限 | 200 条（环形裁剪） |
| 剪贴板恢复 | 通过（哨兵文本验证） |
| 日志泄漏 | 无：日志只有 `len/method/app/elapsed_ms` |

注意：调试命令必须 `spawn_blocking` 执行。同步命令会占住主线程，自身窗口无法及时处理合成的 Ctrl+C，表现为“超时但剪贴板随后才变化”。

UIA 只做可行性判断，未实现：WebView2/Chromium 的 UIA 文本模式需要额外 COM 封装，M0 时间用于打通剪贴板基线；D0-2 维持剪贴板为基线。

## 3. 焦点实验（T0.5）

实现：`src-tauri/src/platform/windows/popup.rs`。

- 顶层窗口追加 `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`、去掉 `WS_EX_APPWINDOW`，`SetWindowPos(HWND_TOPMOST, SWP_NOACTIVATE|SWP_SHOWWINDOW)` + `ShowWindow(SW_SHOWNOACTIVATE)`。
- 关键结论：**样式必须在 Tauri `window.show()` 之前写入，且不要调用 `window.show()`**。tao 的 show 会按 builder 配置重写扩展样式，还会激活窗口；改回纯 Win32 `ShowWindow` 后，弹窗保持 `NOACTIVATE + TOOLWINDOW`，且 WebView2 仍能正常渲染与响应鼠标。
- `SetWindowSubclass` 安装 `WM_MOUSEACTIVATE → MA_NOACTIVATE`；必须切到主线程执行（`SetWindowSubclass` 要求创建窗口的线程）。`AppHandle::run_on_main_thread` 在同步命令里等待自身会死锁，因此记录了主线程 id，已在主线程时直接执行。
- WebView2 子窗口点击仍可能主动激活宿主：兜底在 `WM_ACTIVATE` 时延迟 350ms 归还前台；编辑模式用 `ALLOW_ACTIVATION` 放行，避免把编辑焦点也还回去。
- 弹窗关闭用 `ShowWindow(SW_HIDE)`，不用 `WebviewWindow::hide()`：窗口是 Win32 直接显示的，tao 内部可见状态不一致，`hide()` 会变空操作。
- 三个窗口在启动时由窗口工厂预创建（popup/settings 隐藏）。在命令线程里 `WebviewWindowBuilder::build()` 会阻塞事件循环，懒创建会卡住命令。
- 定位：锚点 → `MonitorFromPoint` 工作区 → `place_popup`（物理像素，优先锚点下方 18px，越界翻到上方，再按工作区 clamp；负坐标副屏不回归主屏）。单元测试覆盖 6 种情况。
- 透明 / 阴影：M0 保持不透明、开启系统阴影（`shadow: true`），未做透明实验；D0-3 维持默认不透明。

实测：

| 检查 | 结果 |
| --- | --- |
| 弹窗显示前后 `GetForegroundWindow` | 不变（manager 保持前台） |
| 弹窗样式 | `NOACTIVATE=true`、`TOOLWINDOW=true`、`APPWINDOW=false` |
| 点击弹窗按钮后前台 | 不变；真实鼠标点击“关闭”可隐藏弹窗 |
| 编辑模式 | 560×640、`document.hasFocus()=true`，关闭后前台恢复触发前窗口 |
| 热键二次按下 | 隐藏；再次按下重新捕获 |
| 弹窗位置 | 锚点附近且不越界（manager 内选区 1112,4） |

## 4. 热键（T0.3）

`CmdOrCtrl+Shift+D`，只处理 `Pressed`，250ms 去抖（`AppState::allow_trigger`，有单测）。注册失败写 `HotkeyStatus` 并发 `runtime://hotkey-error`，manager 运行状态页显示“未注册 + 原因”。`register_hotkey(app, accelerator)` 已抽出，M2 改热键可直接复用。

## 5. 日志与诊断（T0.6）

- `tracing` + `tracing-appender` 按天写入 `%LOCALAPPDATA%\LexiGrab\logs\lexigrab.log.YYYY-MM-DD`，debug 构建同时输出控制台。
- 打点：启动、热键注册、捕获成功/失败（长度、方法、应用名、耗时、错误码）、弹窗显示/隐藏、前端命令。无选中文本、剪贴板内容、窗口标题。
- manager 运行状态页含“捕获测试”面板：最近一次文本、方法、应用、耗时、锚点、样本数、p50/p95、成功率，以及“重新捕获”。
- 测试钩子：`LEXIGRAB_HOME` 覆盖配置/日志/WebView2 数据目录（便携与自动化验证用），`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` 用于 CDP 验证。

## 6. 应用矩阵（T0.7）

已自动验证：LexiGrab 自身 manager（WebView2/Chromium），脚本选中 95 字符。

**待手工**（本次未在 Notepad / Chrome / Firefox / VS Code / Word / Windows Terminal / Excel 上逐个按键验证）：在同一台 Windows 上按计划逐应用跑 3 次，记录文本、锚点、焦点与失败码。预期 Chromium/Electron 系与 WebView2 表现一致。已知测试环境限制：本机开启 Windows Defender 受控文件夹访问，沙箱内启动的未签名调试程序无法写 `%APPDATA%`，所以自动化验证统一用 `LEXIGRAB_HOME` 指向工作区内目录；正常用户终端运行不受该沙箱限制，但若本机确实开了受控文件夹访问，需要把 `lexigrab.exe`/`tauri dev` 加入白名单。

## 7. 决策（D0-1 ~ D0-4）

| 编号 | 结论 |
| --- | --- |
| D0-1 | 快速模式永远非激活；编辑模式才激活。编辑模式激活 100% 成功（CDP 与焦点检查均通过），不需要回退为可聚焦弹窗。 |
| D0-2 | 文本主路径为剪贴板；UIA 未实测，M2 暂不改 UIA 优先。 |
| D0-3 | 默认不透明 + 系统阴影；透明未实验，M2 也不需要。 |
| D0-4 | 点击外部关闭 M0 不做，M2 评估低级鼠标钩子。 |

## 8. 交接 M1 / M2

- 复用：`SelectionContext` DTO（`src/lib/types.ts` ↔ `platform/selection.rs`）、`core::config::Paths`、`core::logging`、`core::window` 窗口工厂、`core::shortcut::register_hotkey`、`AppState` 采样统计。
- 事件：`popup://selection`、`popup://capture-error`（M0 新增，M2 可并入正式契约）、`runtime://hotkey-error`。
- M0 专用命令：`debug_trigger_capture`、`get_last_capture`、`get_capture_stats`；M1 起保留为诊断入口，不进入正式 IPC 契约。
- 注意：`get_last_capture` / `capture-error` 事件是 M0 为无热键环境补的，M2 实现完整弹窗状态机时可删。
- 剪贴板副作用：原本无文本时不清空剪贴板，保留复制结果；M2 若要彻底无副作用，需要“记录并恢复非文本剪贴板”或 UIA。

## 9. 出口检查对照

- [x] manager 可启动，三个入口存在（settings 也随启动预创建）。
- [x] 默认热键可触发（自动验证：manager）。
- [x] 捕获返回文本、锚点、前台窗口，p95 ≤ 180ms（实测 ~42ms）。
- [x] 弹窗显示后前台窗口不变，点击按钮焦点不变。
- [x] 编辑模式可输入并恢复焦点（`document.hasFocus()` + 关闭后前台恢复）。
- [x] 150% / 双屏定位：仅有 clamp 单测，未在实机双屏跑过，M1 开始时补一次。
- [x] 日志不含选中文本。
- [x] CI 文件就位；本地 `cargo fmt/clippy/test` 与 `pnpm typecheck/lint/test` 全绿。
- [ ] GitHub Actions 尚未实际跑过。
- [x] 报告完成，D0 决策已定。

## 10. 后续建议

1. 用真实应用补 T0.7 手工矩阵，把结果补进本文第 6 节。
2. M1 起把 `LEXIGRAB_HOME` 保留为测试钩子，正式路径仍走 `%APPDATA%` / `%LOCALAPPDATA%`。
3. 若目标机开启受控文件夹访问，在 README/关于页补充“把 lexigrab.exe 加入白名单”的说明。
