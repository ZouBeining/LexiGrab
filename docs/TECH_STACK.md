# TECH_STACK

本文件是给人看的版本速查表。真正生效的是 `package.json`、`extension/package.json`、`src-tauri/Cargo.toml` 与两份锁文件；版本冲突时以锁文件为准，并回写本表。

版本锁定日期：2026-10-28。升级任何直接依赖都要单独 PR，并在 `PROGRESS.md` 记录影响。

## 1. 运行时与工具链

| 组件 | 锁定版本 | 用途 |
| --- | --- | --- |
| Windows | 10 22H2 / 11 | MVP 支持范围 |
| WebView2 | 系统最新稳定版 | Tauri WebView |
| Node.js | 24.22.1 | 前端、扩展、脚本 |
| pnpm | 11.28.5 | 唯一包管理器 |
| Rust | 1.99.1 | 后端与 Tauri |
| Rust edition | 2021（本项目），2024（部分依赖） | 由各 crate 决定 |
| Tauri CLI | 2.12.4 | 构建与开发 |

## 2. 桌面前端

| 包 | 锁定版本 | 许可证 | 用途 |
| --- | --- | --- | --- |
| `react` | 19.3.1 | MIT | UI |
| `react-dom` | 19.3.1 | MIT | UI |
| `typescript` | 5.9.3 | Apache-2.0 | 类型检查 |
| `vite` | 8.3.5 | MIT | 构建与开发服务器 |
| `@vitejs/plugin-react` | 6.1.2 | MIT | React 转换 |
| `tailwindcss` | 4.3.3 | MIT | 样式 |
| `@tailwindcss/vite` | 4.3.3 | MIT | Tailwind 集成 |
| `@tauri-apps/api` | 2.12.4 | Apache-2.0 OR MIT | IPC 与窗口 API |
| `@tauri-apps/cli` | 2.12.4 | Apache-2.0 OR MIT | `tauri` 命令 |
| `@tauri-apps/plugin-dialog` | 2.8.4 | MIT OR Apache-2.0 | 文件选择 |
| `@tauri-apps/plugin-opener` | 2.7.2 | MIT OR Apache-2.0 | 打开链接与目录 |
| `@types/react` | 19.3.0 | MIT | 类型 |
| `@types/react-dom` | 19.3.0 | MIT | 类型 |
| `@types/node` | 24.10.0 | MIT | Vite 配置类型 |

不使用路由库、状态管理库、CSS-in-JS、组件库或图标包。页面导航用查询参数，状态用 React 内置能力。

## 3. 前端检查与测试

| 包 | 锁定版本 | 许可证 | 用途 |
| --- | --- | --- | --- |
| `eslint` | 10.13.0 | MIT | 静态检查 |
| `typescript-eslint` | 8.72.1 | MIT | TS 规则 |
| `eslint-plugin-react-hooks` | 7.1.1 | MIT | Hooks 规则 |
| `vitest` | 5.0.5 | MIT | 单元测试 |
| `happy-dom` | 21.0.4 | MIT | DOM 测试环境 |
| `@testing-library/react` | 16.3.3 | MIT | 组件测试 |
| `@testing-library/dom` | 10.4.2 | MIT | Testing Library 核心 |

兼容性说明：

- TypeScript 锁在 5.9.3，而不是 7.x。`typescript-eslint@8.72.1` 的 peer 范围是 `>=4.8.4 <6.1.0`。
- 不安装 `eslint-plugin-react`。禁止内联 `style` 用 ESLint `no-restricted-syntax` 的 JSXAttribute 选择器实现，避免插件与 ESLint 10 的 peer 冲突。
- Vite 8 要求 Node `^20.19.0 || >=22.12.0`，当前 Node 24 满足。

## 4. Firefox 扩展

| 包 | 锁定版本 | 许可证 | 用途 |
| --- | --- | --- | --- |
| `wxt` | 0.21.5 | MIT | 扩展构建 |
| `@wxt-dev/module-react` | 1.2.2 | MIT | React 支持 |
| `web-ext` | 10.8.0 | MPL-2.0 | 打包与 lint，仅构建期 |
| `dompurify` | 3.4.17 | Apache-2.0 OR MPL-2.0 | 气泡内二次清洗 |
| `react` | 19.3.1 | MIT | 气泡与设置页 |
| `react-dom` | 19.3.1 | MIT | 气泡与设置页 |

Firefox 使用 WXT 默认的 MV2 构建。`browser_specific_settings.gecko.id` 固定为 `lexigrab@localhost`，`strict_min_version` 为 `109.0`。

`web-ext` 的 MPL-2.0 不影响分发的扩展产物，但升级时仍要记录。DOMPurify 优先按 Apache-2.0 选项使用。

## 5. Rust 后端

| crate | 锁定版本 | 许可证 | 用途 |
| --- | --- | --- | --- |
| `tauri` | 2.12.5 | Apache-2.0 OR MIT | 桌面框架 |
| `tauri-build` | 2.7.4 | Apache-2.0 OR MIT | 构建脚本 |
| `tauri-plugin-global-shortcut` | 2.4.1 | Apache-2.0 OR MIT | 全局热键 |
| `tauri-plugin-dialog` | 2.8.4 | Apache-2.0 OR MIT | 文件选择 |
| `tauri-plugin-opener` | 2.7.2 | Apache-2.0 OR MIT | 打开链接与目录 |
| `tauri-plugin-single-instance` | 2.5.5 | Apache-2.0 OR MIT | 单实例 |
| `serde` | 1.0.215 | MIT OR Apache-2.0 | 序列化 |
| `serde_json` | 1.0.140 | MIT OR Apache-2.0 | JSON |
| `toml` | 0.8.19 | MIT OR Apache-2.0 | 配置 |
| `thiserror` | 2.0.11 | MIT OR Apache-2.0 | 错误类型 |
| `tokio` | 1.43.0 | MIT | 异步运行时 |
| `reqwest` | 0.13.0 | MIT OR Apache-2.0 | 远程词典与 AnkiConnect |
| `ammonia` | 4.0.0 | MIT OR Apache-2.0 | HTML 清洗 |
| `lol_html` | 2.6.0 | BSD-3-Clause | 资源路径改写 |
| `uuid` | 1.17.0 | MIT OR Apache-2.0 | ID 生成 |
| `mime_guess` | 2.0.4 | MIT | 资源 MIME |
| `url` | 2.5.2 | MIT OR Apache-2.0 | URL |
| `percent-encoding` | 2.3.1 | MIT OR Apache-2.0 | 资源路径编码 |
| `unicode-normalization` | 0.1.24 | MIT OR Apache-2.0 | 查询归一化 |
| `clap` | 4.5.23 | MIT OR Apache-2.0 | 开发 CLI |
| `tracing` | 0.1.41 | MIT | 日志 |
| `tracing-subscriber` | 0.3.19 | MIT | 日志输出 |
| `tracing-appender` | 0.2.3 | MIT | 文件日志滚动 |
| `axum` | 0.8.1 | MIT | 本地桥接 |
| `tower` | 0.5.2 | MIT | 中间件测试 |
| `tower-http` | 0.6.2 | MIT | 请求体限制、CORS |
| `subtle` | 2.6.1 | BSD-3-Clause | 令牌常量时间比较 |
| `jsonpath-rust` | 1.0.6 | MIT | JSONPath |
| `rquickjs` | 0.14.0 | MIT | 脚本沙箱 |
| `rusqlite` | 0.33.0（`bundled`） | MIT | 缓存 |
| `tokio-util` | 0.7.13 | MIT | 取消令牌 |
| `windows` | 0.62.0 | MIT OR Apache-2.0 | Win32 API |
| `raw-window-handle` | 0.6.2 | MIT OR Apache-2.0 | HWND 获取 |

测试专用：

| crate | 锁定版本 | 许可证 | 用途 |
| --- | --- | --- | --- |
| `wiremock` | 0.6.2 | MIT OR Apache-2.0 | HTTP mock |
| `http-body-util` | 0.1.2 | MIT | 响应体断言 |

## 6. 词典与脚本依赖

| 组件 | 版本 | 许可证 | 说明 |
| --- | --- | --- | --- |
| libmdx | git `111cab6cddb119ce35158ee1a8d641bab698c87e`（0.5.0） | AGPL-3.0 | 唯一 MDX/MDD 读取库 |
| QuickJS | 由 `rquickjs 0.14.0` 内置 | MIT | 脚本执行 |
| SQLite | 由 `rusqlite` bundled 提供 | 公有领域 | 缓存 |

libmdx 不在 crates.io，必须使用 git 依赖并固定 rev。其 `Cargo.toml` 的 license 字段不是合法 SPDX，需要在合规检查时按 PLAN_M5 处理，不得直接忽略。

## 7. 外部服务

| 服务 | 默认地址 | 说明 |
| --- | --- | --- |
| AnkiConnect | `http://127.0.0.1:8765` | 用户安装插件后提供 |
| 本地桥接 | `http://127.0.0.1:12787` | 桌面端内置，扩展访问 |
| 远程词典 | 用户配置 | 仅 http/https，超时默认 3000ms |

不接入任何第三方分析、崩溃上报、CDN、远程字体或云配置服务。

## 8. 明确不引入

| 类别 | 决定 |
| --- | --- |
| 路由 | 不使用 react-router 等；查询参数切换分区 |
| 状态管理 | 不使用 Redux/Zustand/Jotai；局部 state + 事件 |
| 组件库 | 不使用 shadcn/Radix/MUI；按设计文档自建 |
| 图标 | 不引入图标包；使用内联 SVG |
| CSS-in-JS | 不使用；只使用 Tailwind 工具类 |
| 字体包 | 不打包 Inter/Geist；使用系统字体栈 |
| HTTP 封装 | 前端不请求远程服务；一律经 Tauri Command 或桥接 |
| Anki 客户端 | 不引入第三方 Anki 库；直接实现 AnkiConnect 契约 |
| 测试浏览器 | 不做 Playwright E2E；手工矩阵 + 单元测试 |

## 9. 升级规则

1. 直接依赖升级单独提交，附变更原因与验证结果。
2. 前端与扩展升级后必须跑 `pnpm typecheck`、`pnpm lint`、`pnpm test`。
3. Rust 升级后必须跑 `cargo test`、`cargo clippy --all-targets -- -D warnings`、`cargo deny check`。
4. 涉及 libmdx、rquickjs、rusqlite 的升级要额外跑 MDX 夹具、脚本沙箱、缓存 TTL 测试。
5. 本表与锁文件不一致时，先修表，再合并代码。
