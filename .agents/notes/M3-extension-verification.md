# M3 Firefox 扩展与本地桥接验证

最后更新：2026-11-15。对应计划 [PLAN_M3](../plans/PLAN_M3.md)，契约见
[BACKEND_STRUCTURE](../../docs/BACKEND_STRUCTURE.md) §5 与 [APP_FLOW](../../docs/APP_FLOW.md) §8。

## 1. 交付内容

| 部分 | 位置 | 说明 |
| --- | --- | --- |
| 桥接服务 | `src-tauri/src/bridge/{mod,auth,dto,error,server}.rs` | axum 路由 + 守卫中间件；`AppBridge` 用 `AppState` 实现查询/制卡；`BridgeService` 管生命周期 |
| 桥接接线 | `src-tauri/src/{lib,commands}.rs`、`core/{state,registry}.rs` | 启动/重启/关闭、`get_bridge_status`、`regenerate_bridge_token`、`lookup_collect_all`、语境查询登记 |
| 设置页 | `src/windows/settings/ExtensionSection.tsx` | 启用、端口、状态、令牌显示/复制/轮换、配对三步 |
| 扩展 | `extension/` | WXT 0.21.5 + React + Tailwind 4；background/content/options；句子提取与气泡 |
| 产物 | `extension/dist/lexigrab-firefox-0.1.0.xpi` | 未签名，sha256 见第 4 节（不提交仓库） |

## 2. 自动化测试

Rust（`cd src-tauri && cargo test`）：228 项通过。

| 套件 | 数量 | 覆盖 |
| --- | --- | --- |
| lib（含 bridge 单元） | 109 | 令牌常量时间比较、Origin 前缀、令牌桶补充/上限、DTO 校验、错误映射、`lookup_collect_all`、语境查询环形缓存、健康检查与预检 |
| `tests/bridge_tests.rs` | 17 | 401/CORS、403 Origin、预检、1MB→413、突发溢出→429、selection 形状、partial、sense 200/202/404、anki 成功与 503、未知路由、真实回环端口 |
| 其他既有套件 | 102 | anki 25、card 12、cli 3、config 5、html 14、mdx 10、resource 14、router 7、sense 12 |

扩展（`pnpm -C extension test`）：34 项通过。

- `tests/sentence.test.ts`（14）：普通段落、嵌套 span/链接、中文标点、跨节点多词、无标点长文本、短块、隐藏元素、密码框、10 条固定样本（要求 ≥90% 含选词）、`Intl.Segmenter` 缺失时的标点回退。
- `tests/bridge.test.ts`（12）：未配对、URL 与鉴权头、401/403 映射、网络错误→`DESKTOP_OFFLINE`、超时→`TIMEOUT`、服务端错误正文、非法 JSON、selection/anki/sense 形状。
- `tests/bubble.test.tsx`（8）：加载→结果、DOMPurify 二次清洗、空态、离线与令牌错误区分、保存→再存一张（同词不阻断）、同语境提示、语境轮询 pending→AI 卡、disabled 不干扰。

前端（根目录）：`pnpm typecheck`、`pnpm lint` 通过；前端测试 81 项通过（含
`ExtensionSection.test.tsx` 6 项：状态、遮罩/显示、复制、轮换二次确认、端口校验、保存）。

Rust 静态检查：`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check` 通过。

## 3. 真实桌面端 + HTTP 端到端

用 `cargo build` 的 debug 产物在 `%TEMP%\lexigrab-m3` 运行（仓库根目录带 Low 完整性标签，
按 M2 结论把 exe 复制到 `%TEMP%`），`LEXIGRAB_CONFIG_DIR` 指向临时配置，词典源为
`tests/fixtures/mini.mdx`，桥接端口 12787。

| 请求 | 结果 |
| --- | --- |
| `GET /v1/health`（无令牌） | 401 `AUTH_FAILED`，带 `Access-Control-Allow-Origin` |
| `GET /v1/health`（错误令牌） | 401 |
| `GET /v1/health`（正确令牌） | 200 `{"ok":true,"app":"LexiGrab","version":"0.1.0","api_version":1}` |
| `GET /v1/health` + `Origin: https://evil.example` | 403 `ORIGIN_REJECTED` |
| `OPTIONS /v1/selection` | 204，`Allow-Headers: content-type, x-auth-token` |
| `POST /v1/selection`（`hello`） | 200，1 条 Mini 词条，`[图片] [音频]` 占位，`sense_available=false`，2.2ms |
| `GET /v1/sense/whatever` | 200 `{"status":"disabled"}`（语境辅助默认关闭） |
| `POST /v1/anki`（Anki 未运行） | 503 `ANKI_UNREACHABLE` |
| 端口 12787 被占用时启动 | 应用不崩溃，日志 `WARN 桥接服务启动失败 code=BRIDGE_PORT_IN_USE`，管理窗口仍可用 |

## 4. `.xpi` 构建与安装

```powershell
pnpm install
pnpm -C extension build:firefox     # .output/firefox-mv2
pnpm -C extension lint:firefox      # web-ext lint：0 error / 7 warning
pnpm -C extension package:firefox   # dist/lexigrab-firefox-0.1.0.xpi
```

- 产物：`extension/dist/lexigrab-firefox-0.1.0.xpi`（167 KB），
  sha256 `622D129743FD12974EE75E7EC0504C61456314E8400FDC3E94AEA98D4AFBF7F5`。
- manifest 实测：`manifest_version=2`、`permissions=["storage","http://127.0.0.1/*"]`、
  `gecko.id=lexigrab@localhost`、`strict_min_version=109.0`、content script `<all_urls>`、
  `options_ui` 指向 `options.html`；无 `tabs`/`history`/`bookmarks`。
- `web-ext lint` 的 7 条 warning：1 条 `MISSING_DATA_COLLECTION_PERMISSIONS`（AMO 2025-11
  新政策，自托管未签名分发不适用；补该键会与 `strict_min_version=109.0` 冲突）、
  6 条 `UNSAFE_VAR_ASSIGNMENT`（React 与 DOMPurify 打包产物里的 `innerHTML`，释义已两次清洗）。

安装方式：

| 浏览器 | 步骤 | 重启后 |
| --- | --- | --- |
| Firefox Developer Edition / Nightly | `about:config` 允许未签名扩展（`xpinstall.signatures.required=false`），`about:addons` → 从文件安装 `.xpi` | 保留，`storage.local` 中的端口与令牌保留 |
| Firefox 正式版 | `about:debugging` → 此 Firefox → 临时载入附加组件 → 选择 `manifest.json` | 失效，需重新载入 |

## 5. 未完成与限制

- **本机没有安装 Firefox**（`Program Files`、注册表与 `%LOCALAPPDATA%` 均无 Firefox/Dev
  Edition/Nightly），因此“在 Firefox 中划词→查询→制卡、`.xpi` 安装、重启保留”无法在本机
  自动验证。已交付 `.xpi`、sha256、安装步骤与 lint 结果，留作真实桌面手工验收。
- 手工站点矩阵（Wikipedia / 新闻 / 中文页面各 3 次、令牌轮换、端口冲突、Anki 关闭、
  同词多句保存）同样待真实 Firefox 环境补测。
- iframe 内选区不支持，`content_scripts` 只跑顶层文档；这是 M3 范围外。
- 语境辅助在 M3 只会返回 `disabled`/`pending`；`selected`/`low_confidence`+AI 由 M4 写入，
  扩展已按 `SenseResolution` 展示推荐义项与 AI 卡（单元测试覆盖）。
