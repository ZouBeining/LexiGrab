# THIRD_PARTY_LICENSES

本文件列出 LexiGrab 直接依赖的第三方组件及其许可证，供分发审查使用。LexiGrab 自身以
[AGPL-3.0](LICENSE) 发布；`src-tauri/Cargo.toml` 的 `license` 字段与源文件 SPDX 头统一为
`AGPL-3.0-only`。

事实来源：`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`（经 `cargo metadata` 读取
registry 元数据）、`package.json`、`extension/package.json` 与 `pnpm-lock.yaml`。
下表版本与许可证均取自锁文件/registry 元数据或 `node_modules/*/package.json` 的
`license` 字段，未做人工推测。版本以锁文件为准；升级直接依赖时同步更新本文件与
[docs/TECH_STACK.md](docs/TECH_STACK.md)。

覆盖范围：三份清单（Cargo、桌面 npm、扩展 npm）中的**直接依赖**，共 59 个唯一包
（Rust 34 个；npm 25 个）。Rust 的 `tokio` 同时出现在 `[dependencies]` 与
`[dev-dependencies]`，只计一次。传递依赖由 `cargo deny` 与 `pnpm licenses list` 覆盖，
不在本表逐项列出；其中与合规相关的传递项在第 5 节单列。

## 1. 运行时依赖

随桌面端或扩展一起分发、在用户机器上执行的依赖。

### 1.1 Rust 后端（`src-tauri/Cargo.toml`）

| 名称 | 版本 | 许可证（registry 元数据） | 用途 |
| --- | --- | --- | --- |
| `tauri` | 2.12.5 | Apache-2.0 OR MIT | 桌面应用框架：窗口、托盘、IPC |
| `tauri-plugin-global-shortcut` | 2.4.1 | Apache-2.0 OR MIT | 全局热键注册 |
| `tauri-plugin-single-instance` | 2.5.5 | Apache-2.0 OR MIT | 单实例守护 |
| `tauri-plugin-dialog` | 2.8.4 | Apache-2.0 OR MIT | 文件选择对话框 |
| `tauri-plugin-opener` | 2.7.2 | Apache-2.0 OR MIT | 打开链接与目录 |
| `serde` | 1.0.229 | MIT OR Apache-2.0 | 序列化/反序列化 |
| `serde_json` | 1.0.154 | MIT OR Apache-2.0 | JSON 读写 |
| `thiserror` | 2.0.21 | MIT OR Apache-2.0 | 错误类型派生 |
| `tracing` | 0.1.44 | MIT | 结构化日志 |
| `tracing-subscriber` | 0.3.23 | MIT | 日志订阅与过滤 |
| `tracing-appender` | 0.2.5 | MIT | 日志文件滚动 |
| `toml` | 0.8.23 | MIT OR Apache-2.0 | 配置读写 |
| `uuid` | 1.29.0 | Apache-2.0 OR MIT | 唯一 ID |
| `unicode-normalization` | 0.1.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | 查询 NFKC 归一化 |
| `percent-encoding` | 2.3.2 | MIT OR Apache-2.0 | 资源路径编码 |
| `url` | 2.5.8 | MIT OR Apache-2.0 | URL 解析 |
| `mime_guess` | 2.0.5 | MIT | `mdx-res` 资源 MIME |
| `ammonia` | 4.2.4 | MIT OR Apache-2.0 | 释义 HTML 白名单清洗 |
| `lol_html` | 2.9.0 | BSD-3-Clause | 流式 HTML 改写（资源路径） |
| `scraper` | 0.24.0 | ISC | 从释义 HTML 提取义项候选 |
| `clap` | 4.6.7 | MIT OR Apache-2.0 | 开发 CLI 参数解析 |
| `reqwest` | 0.13.5 | MIT OR Apache-2.0 | 远程词典 / AnkiConnect / 选择器 / LLM 的 HTTP 客户端 |
| `mdictlib` | 0.2.8 | MIT | 唯一 MDX/MDD 读取库 |
| `rusqlite` | 0.32.1 | MIT | SQLite 缓存（`bundled`） |
| `jsonpath-rust` | 1.0.6 | MIT | 远程响应 JSONPath 解析 |
| `rquickjs` | 0.14.0 | MIT | QuickJS 脚本沙箱 |
| `axum` | 0.8.9 | MIT | 本地桥接 HTTP 路由 |
| `tokio` | 1.54.1 | MIT | 异步运行时（同时用于测试） |
| `windows` | 0.62.2 | MIT OR Apache-2.0 | Win32 API（`cfg(windows)`） |
| `raw-window-handle` | 0.6.2 | MIT OR Apache-2.0 OR Zlib | 获取 HWND（`cfg(windows)`） |

### 1.2 桌面前端（根 `package.json` 的 `dependencies`）

| 名称 | 版本 | 许可证（`package.json`） | 用途 |
| --- | --- | --- | --- |
| `@tauri-apps/api` | 2.12.4 | Apache-2.0 OR MIT | IPC 与窗口 API |
| `@tauri-apps/plugin-dialog` | 2.8.4 | MIT OR Apache-2.0 | 对话框前端封装 |
| `@tauri-apps/plugin-opener` | 2.7.2 | MIT OR Apache-2.0 | 打开链接前端封装 |
| `react` | 19.3.1 | MIT | UI |
| `react-dom` | 19.3.1 | MIT | UI 渲染 |

### 1.3 Firefox 扩展（`extension/package.json` 的 `dependencies`）

| 名称 | 版本 | 许可证（`package.json`） | 用途 |
| --- | --- | --- | --- |
| `react` | 19.3.1 | MIT | 气泡与设置页 UI（与桌面前端同版本） |
| `react-dom` | 19.3.1 | MIT | 气泡与设置页渲染（与桌面前端同版本） |
| `dompurify` | 3.4.17 | (MPL-2.0 OR Apache-2.0) | 气泡内释义二次清洗 |

## 2. 构建期依赖

只在构建、打包、类型检查时执行，不进入最终用户运行时（`web-ext` 除外说明见第 5 节）。

| 名称 | 版本 | 许可证 | 用途 | 清单 |
| --- | --- | --- | --- | --- |
| `tauri-build` | 2.7.4 | Apache-2.0 OR MIT | Tauri 构建脚本 | Cargo `[build-dependencies]` |
| `@tauri-apps/cli` | 2.12.4 | Apache-2.0 OR MIT | `tauri` 命令（构建/开发） | 根 `devDependencies` |
| `vite` | 8.3.5 | MIT | 前端构建与开发服务器 | 根 `devDependencies` |
| `@vitejs/plugin-react` | 6.1.2 | MIT | React 转换 | 根 `devDependencies` |
| `@tailwindcss/vite` | 4.3.3 | MIT | Tailwind 的 Vite 集成 | 根与扩展 `devDependencies` |
| `tailwindcss` | 4.3.3 | MIT | 工具类 CSS 生成 | 根与扩展 `devDependencies` |
| `typescript` | 5.9.3 | Apache-2.0 | 类型检查与 TS 编译 | 根与扩展 `devDependencies` |
| `wxt` | 0.21.5 | MIT | Firefox 扩展构建框架 | 扩展 `devDependencies` |
| `@wxt-dev/module-react` | 1.2.2 | MIT | WXT 的 React 支持 | 扩展 `devDependencies` |
| `web-ext` | 10.8.0 | MPL-2.0 | 扩展 lint / `.xpi` 打包，仅构建期 | 扩展 `devDependencies` |

## 3. 开发期与测试依赖

只在本地开发和 CI 中运行，不随产品分发。

| 名称 | 版本 | 许可证 | 用途 | 清单 |
| --- | --- | --- | --- | --- |
| `wiremock` | 0.6.5 | MIT/Apache-2.0（元数据旧式写法，等价 MIT OR Apache-2.0） | 远程源 HTTP mock | Cargo `[dev-dependencies]` |
| `tower` | 0.5.3 | MIT | 桥接路由 `oneshot` 测试 | Cargo `[dev-dependencies]` |
| `http-body-util` | 0.1.5 | MIT | 响应体断言 | Cargo `[dev-dependencies]` |
| `eslint` | 10.13.0 | MIT | 静态检查 | 根 `devDependencies` |
| `typescript-eslint` | 8.72.1 | MIT | TypeScript ESLint 规则 | 根 `devDependencies` |
| `eslint-plugin-react-hooks` | 7.1.1 | MIT | React Hooks 规则 | 根 `devDependencies` |
| `vitest` | 5.0.5 | MIT | 单元测试 | 根与扩展 `devDependencies` |
| `happy-dom` | 21.0.4 | MIT | DOM 测试环境 | 根与扩展 `devDependencies` |
| `@testing-library/dom` | 10.4.2 | MIT | Testing Library 核心 | 根与扩展 `devDependencies` |
| `@testing-library/react` | 16.3.3 | MIT | React 组件测试 | 根与扩展 `devDependencies` |
| `@types/node` | 24.10.0 | MIT | Node 类型 | 根 `devDependencies` |
| `@types/react` | 19.3.0 | MIT | React 类型 | 根与扩展 `devDependencies` |
| `@types/react-dom` | 19.3.0 | MIT | React DOM 类型 | 根与扩展 `devDependencies` |

## 4. 运行时传递依赖中的许可证要点

`cargo deny` 会检查全部传递依赖。当前锁文件中出现、且值得记录的非 MIT/Apache 许可：

- `cssparser`（0.35–0.38）、`cssparser-macros`、`selectors`、`dtoa-short`、`option-ext`：
  **MPL-2.0**，经 `scraper`/`html5ever` 引入；MPL-2.0 与 AGPL-3.0 兼容，且按文件级
  copyleft 使用，未修改其源码。
- `brotli`：BSD-3-Clause AND MIT；`brotli-decompressor`：BSD-3-Clause/MIT。
- `matchit`：MIT AND BSD-3-Clause；`dpi`：Apache-2.0 AND MIT。
- `zlib-rs`、`foldhash`：Zlib；`ego-tree`、`libloading`：ISC。
- `ar_archive_writer`、`target-lexicon`：Apache-2.0 WITH LLVM-exception。
- `unicode-ident`、`icu_*`/`zerovec` 等：Unicode-3.0（经 `unicode-normalization` 等引入）。

## 5. 需要单列说明的组件

**libmdx 计划偏差。** 计划 T5.5 假设 MDX 使用 `libmdx`（git `111cab6c`，AGPL-3.0），
并为它保留 `[sources] allow-git`。实际实现使用 crates.io 的 **`mdictlib 0.2.8`（MIT）**：
计划中的 libmdx 提交在 GitHub 上不存在（2026-10-29 核实，见
[docs/TECH_STACK.md](docs/TECH_STACK.md) 第 6 节）。因此当前没有任何 git 源依赖，
`deny.toml` 的 `allow-git` 为空；只有将来确实改回 libmdx 时，才允许写死
`https://github.com/ZouBeining/*` 下的具体 fork rev，不得放宽 `unknown-git`。

**DOMPurify 双许可。** `dompurify 3.4.17` 的 `package.json` 写的是
`(MPL-2.0 OR Apache-2.0)`。本项目按 **Apache-2.0** 选项使用，MPL-2.0 与 Apache-2.0
均与 AGPL-3.0 兼容；若未来分发方式变化，可在两者中择一遵守。

**WXT / React 为 MIT。** `wxt 0.21.5`、`@wxt-dev/module-react 1.2.2`、`react` /
`react-dom 19.3.1` 均为 MIT；React 不附带额外专利限制以外的条款。

**web-ext 为 MPL-2.0，仅构建期。** `web-ext 10.8.0` 只用于 `build:firefox` 后的
lint 与 `.xpi` 打包，不进入扩展产物，也不随桌面端分发。MPL-2.0 与 AGPL-3.0 兼容。

**scraper 为 ISC。** `scraper 0.24.0` 用于义项提取，ISC 与 AGPL-3.0 兼容。

**rusqlite + bundled SQLite。** `rusqlite 0.32.1` 为 MIT；`bundled` 特性会编译并静态
链接 vendored SQLite（经传递依赖 `libsqlite3-sys 0.30.1`，MIT）。SQLite 本身为
**public domain**，可自由分发；分发时无需附带 SQLite 源码。

**rquickjs / QuickJS。** `rquickjs 0.14.0`（及 `rquickjs-sys 0.14.0`）为 MIT，内置的
QuickJS 引擎同为 MIT；脚本沙箱随二进制分发。

**jsonpath-rust。** `jsonpath-rust 1.0.6` 为 MIT，用于远程响应解析。

**mdictlib。** `mdictlib 0.2.8` 为 MIT，是唯一 MDX/MDD 读取库，替代不可达的 libmdx。

**字体不打包。** 项目不引入 Inter/Geist 等字体包，使用系统字体栈（见
[docs/TECH_STACK.md](docs/TECH_STACK.md) 第 8 节）；因此没有字体许可证与再分发义务。

## 6. 待决策

- 暂无许可证标识冲突：`src-tauri/Cargo.toml`、`LICENSE` 与源文件 SPDX 头统一为
  `AGPL-3.0-only`（2026-11-21 由 `AGPL-3.0-or-later` 对齐）。
- 前端 `src/**` 的 SPDX 头由负责人统一处理，不在本次范围。

## 7. 如何重新生成 / 核对

```bash
# Rust：读取锁文件里每个包的 license 元数据（不联网也可，用 --offline）
cd src-tauri
cargo metadata --format-version 1 > /tmp/lexigrab-metadata.json
cargo deny check licenses bans sources

# npm：列出全部依赖的实际许可证（根与扩展都要看）
cd ..
pnpm licenses list
pnpm -C extension licenses list
```

核对要点：`cargo metadata` 的 `packages[].license` 与 `Cargo.lock` 版本一致；
`pnpm licenses list` 输出与 `node_modules/<pkg>/package.json` 的 `license` 一致；
本文件表格与 `docs/TECH_STACK.md` 无冲突。发现新增直接依赖时，先确认许可证在
`deny.toml` 的 allow 列表内，再更新本文件。
