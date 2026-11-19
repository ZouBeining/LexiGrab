# M3 Firefox 扩展与本地桥接验证

最后更新：2026-11-19。对应计划 [PLAN_M3](../plans/PLAN_M3.md)，契约见
[BACKEND_STRUCTURE](../../docs/BACKEND_STRUCTURE.md) §5 与 [APP_FLOW](../../docs/APP_FLOW.md) §8。

## 1. 交付内容

| 部分 | 位置 | 说明 |
| --- | --- | --- |
| 桥接服务 | `src-tauri/src/bridge/{mod,auth,dto,error,server}.rs` | axum 路由 + 守卫中间件；`AppBridge` 用 `AppState` 实现查询/制卡；`BridgeService` 管生命周期 |
| 桥接接线 | `src-tauri/src/{lib,commands}.rs`、`core/{state,registry}.rs` | 启动/重启/关闭、`get_bridge_status`、`regenerate_bridge_token`、`lookup_collect_all`、语境查询登记 |
| 设置页 | `src/windows/settings/ExtensionSection.tsx` | 启用、端口、状态、令牌显示/复制/轮换、配对三步 |
| 扩展 | `extension/` | WXT 0.21.5 + React + Tailwind 4；background/content/options；句子提取与气泡 |
| 真机验收自动化 | `.agents/automation/m3-firefox/` | 自带 Marionette 客户端，无 geckodriver/selenium；驱动 Dev Edition 安装 `.xpi`、配对、划词、制卡、重启、站点矩阵 |
| 本轮产品修复 | `extension/lib/bridge.ts`、`content/Bubble.tsx`、`content/style.css` | 透出桥接错误码、保存失败给建议动作、Shadow DOM 内补齐 Tailwind 变量默认值 |
| 产物 | `extension/dist/lexigrab-firefox-0.1.0.xpi` | 未签名，sha256 见第 4 节（不提交仓库） |

## 2. 自动化测试

Rust（`cd src-tauri && cargo test`）：228 项通过。

| 套件 | 数量 | 覆盖 |
| --- | --- | --- |
| lib（含 bridge 单元） | 109 | 令牌常量时间比较、Origin 前缀、令牌桶补充/上限、DTO 校验、错误映射、`lookup_collect_all`、语境查询环形缓存、健康检查与预检 |
| `tests/bridge_tests.rs` | 17 | 401/CORS、403 Origin、预检、1MB→413、突发溢出→429、selection 形状、partial、sense 200/202/404、anki 成功与 503、未知路由、真实回环端口 |
| 其他既有套件 | 102 | anki 25、card 12、cli 3、config 5、html 14、mdx 10、resource 14、router 7、sense 12 |

扩展（`pnpm -C extension test`）：42 项通过。

- `tests/sentence.test.ts`（14）：普通段落、嵌套 span/链接、中文标点、跨节点多词、无标点长文本、短块、隐藏元素、密码框、10 条固定样本（要求 ≥90% 含选词）、`Intl.Segmenter` 缺失时的标点回退。
- `tests/bridge.test.ts`（19）：未配对、URL 与鉴权头、401/403 映射、网络错误→`DESKTOP_OFFLINE`、超时→`TIMEOUT`、服务端错误正文、6 个透传错误码（`ANKI_UNREACHABLE`/`ANKI_FIELD_MISSING`/`ANKI_API_ERROR`/`RATE_LIMITED`/`PAYLOAD_TOO_LARGE`/`INVALID_REQUEST`）、未知码回落 `SERVER_ERROR`、非法 JSON、selection/anki/sense 形状。
- `tests/bubble.test.tsx`（9）：加载→结果、DOMPurify 二次清洗、空态、离线与令牌错误区分、保存→再存一张（同词不阻断）、同语境提示、`ANKI_UNREACHABLE` 可操作提示、语境轮询 pending→AI 卡、disabled 不干扰。

前端（根目录）：`pnpm typecheck`、`pnpm lint` 通过；前端测试 81 项通过（含
`ExtensionSection.test.tsx` 6 项：状态、遮罩/显示、复制、轮换二次确认、端口校验、保存）。

静态检查：`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check`、扩展 `tsc --noEmit`
通过；`web-ext lint` 0 error / 7 warning（见第 4 节）。

## 3. 真实桌面端 + HTTP 端到端

用 `cargo build` 的 debug 产物在 `%TEMP%\lexigrab-m3` 运行（仓库根目录带 Low 完整性标签，
按 M2 结论把 exe 复制到 `%TEMP%`），`LEXIGRAB_CONFIG_DIR` 指向临时配置，词典源为
`tests/fixtures/mini.mdx`，桥接端口 12787。2026-11-19 的真机验收中，以下请求由
`run.py` 的桥接场景再次复核。

| 请求 | 结果 |
| --- | --- |
| `GET /v1/health`（无令牌） | 401 `AUTH_FAILED`，带 `Access-Control-Allow-Origin` 并回显 `moz-extension://` Origin |
| `GET /v1/health`（错误令牌） | 401 |
| `GET /v1/health`（正确令牌） | 200 `{"ok":true,"app":"LexiGrab","version":"0.1.0","api_version":1}` |
| `GET /v1/health` + `Origin: https://evil.example` | 403 `ORIGIN_REJECTED` |
| `OPTIONS /v1/selection` | 204，`Allow-Headers: content-type, x-auth-token` |
| `POST /v1/selection`（`hello`） | 200，1 条 Mini 词条，`[图片] [音频]` 占位，`sense_available=false`，约 2ms |
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

- 产物：`extension/dist/lexigrab-firefox-0.1.0.xpi`（167,848 字节），
  sha256 `CBCD124504590C9D1A6124D262F51B5E21941FEAEEE7D9306ED2560F4F23B362`。
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

## 5. Firefox Developer Edition 真机验收（2026-11-19）

### 5.1 环境与方法

- 浏览器：Firefox Developer Edition 161.0（buildID `20261118090429`），安装于
  `D:\Firefox Developer Edition`；验收用独立 profile，`xpinstall.signatures.required=false`。
- 桌面端：本轮 `cargo build` 的 debug `lexigrab.exe` 复制到 `%TEMP%\lexigrab-m3\e2e`，
  配置 `LEXIGRAB_CONFIG_DIR` + `mini.mdx`，桥接 `127.0.0.1:12787`。
- Anki：26.9.4 + AnkiConnect（FooSoft 官方源码安装到 `addons21/2055492159`），
  牌组 `LexiGrab E2E`、笔记类型 `LexiGrab Basic`（`Word/Definition/Sentence/Source`）。
- 驱动：Firefox 自带 Marionette（TCP 2828），不依赖 geckodriver/selenium；
  代码在 `.agents/automation/m3-firefox/`（`marionette.py`、`ffx.py`、`desktop.py`、
  `testpage.py`、`anki_api.py`、`run.py`）。`.xpi` 用 `Addon:Install`（非临时）安装，
  相当于 `about:addons` 的“从文件安装”；配对走选项页真实表单与“测试连接”。
- 外网：本机 Firefox 访问 Wikipedia/BBC 需走系统代理（`network.proxy.type=5`）。

运行：

```powershell
cd .agents/automation/m3-firefox
python -u run.py          # 结果：artifacts/results.json 与截图
```

### 5.2 结果

**29/29 场景通过**（`artifacts/results.json`），全程无脚本异常。关键结果：

| 场景 | 结果 | 证据 |
| --- | --- | --- |
| 未签名 `.xpi` 安装并启用 | 通过 | `Addon:Install` 返回 `lexigrab@localhost`，`isActive=true` |
| 选项页配对 | 通过 | “测试连接”显示 `连接成功：LexiGrab 0.1.0（API v1）`；保存后 `storage.local` 端口/64 位令牌正确 |
| 本地英文页划词→查询 | 通过 | 选中 `Hello`，气泡显示 Mini 释义、来源与 `[图片] [音频]` 占位 |
| 保存到真实 Anki | 通过 | `note_id` 落库，四字段分别为 `Hello` / Mini 释义 / `Hello world` / `Mini` |
| 同词同句同释义再存 | 通过 | 第二次仍写入 Anki，气泡显示“此前已保存过” |
| 同词不同句 | 通过 | `Hello, can you hear me?` 与 `She said hello to the world.` 各一张卡，无同语境提示 |
| 中文页划词 | 通过 | 选区句子为整句中文，`。` 切句正确 |
| 新闻页 `world` | 通过 | 返回 Earth 释义 |
| Anki 关闭→保存 | 通过 | 气泡显示“Anki 未连接 / 启动 Anki 并确认 AnkiConnect 已安装…”与 `ANKI_UNREACHABLE` |
| 重启 Anki→重试保存 | 通过 | 同一气泡内重试成功 |
| 令牌轮换后旧令牌 | 通过 | 气泡显示“令牌错误 / AUTH_FAILED”，提示重新配对 |
| 选项页换新令牌 | 通过 | 重新配对后划词立即恢复 |
| 重启 Firefox 后扩展 | 通过 | `isActive=true`，扩展仍在 |
| 重启 Firefox 后配对信息 | 通过 | `storage.local` 端口与令牌保留 |
| 重启后再次划词 | 通过 | 查询与制卡链路正常 |
| 端口占用 | 通过 | 应用不崩溃、不换端口，日志 `BRIDGE_PORT_IN_USE`，health 不可达 |
| 端口释放后重启 | 通过 | 同端口桥接恢复 |
| 桥接鉴权/CORS | 通过 | 无令牌 401（回显 Origin）、旧令牌 401、当前令牌 200、非法 Origin 403、预检 204 |

真实站点矩阵（每站 3 次可见选区；隐藏/无正文的同词节点自动跳过）：

| 站点 | 选区 | 有释义 | 说明 |
| --- | --- | --- | --- |
| 英文 Wikipedia `Hello` | 3/3 | 3/3 | 标题、消歧义行、正文各一次，句子提取均为可见原文 |
| 中文 Wikipedia `Hello` | 3/3 | 3/3 | 中文条目与中文标点，选区与气泡正常 |
| BBC News World | 3/3 | 2/3 | 两次 `world` 命中释义；第三次用 `news` 验证空态（Mini 词典无该词），气泡正常提示“未找到释义” |

截图（`artifacts/`）：`10-local-en-results.png`、`11-local-en-saved.png`、
`12-local-same-context.png`、`13-local-different-sentence.png`、`14-local-zh-results.png`、
`15-local-news-results.png`、`20-anki-closed-error.png`、`21-anki-retry-saved.png`、
`30-token-rotated-error.png`、`40-restart-query.png`、`50-*.png`（三个真实站点）、
`60-style-probe.png`（气泡计算样式复验）。

### 5.3 本轮发现并修复的问题

1. **扩展丢失桥接错误码**（`extension/lib/bridge.ts`、`content/Bubble.tsx`）。
   非 2xx 响应一律映射成 `SERVER_ERROR`，Anki 未运行时用户只看到泛化文案，
   `ANKI_UNREACHABLE` 等可操作错误被吞掉，与 APP_FLOW §8.2“按错误码给出动作”不符。
   现按 BACKEND_STRUCTURE §5.2 透出 `ANKI_UNREACHABLE`、`ANKI_API_ERROR`、
   `ANKI_FIELD_MISSING`、`ANKI_DUPLICATE`、`RATE_LIMITED`、`PAYLOAD_TOO_LARGE`、
   `INVALID_REQUEST`，保存失败区同时显示标题、建议动作、服务端文案与错误码。
   新增 bridge 7 项、bubble 1 项测试（34 → 42）。
2. **气泡在 Firefox Shadow DOM 里没有边框和阴影**（`entrypoints/content/style.css`）。
   Tailwind 4 把 `--tw-border-style`、`--tw-inset-shadow` 等默认值放在 `@property`
   与 `@supports` 保护块中；Firefox 在 Shadow Root 内不应用这些注册，导致
   `border`、`shadow-2xl` 生成的 `var()` 无效，气泡与白色页面混在一起。
   现在在 `.lg-interactive` 作用域为每层元素直接声明这些变量；实测计算样式
   `border: 1px solid rgb(230,230,230)`、`box-shadow: … rgba(0,0,0,0.25) 0 25px 50px -12px`，
   见 `60-style-probe.png`。

### 5.4 仍有约束

- iframe 内选区不支持：content script 只跑顶层文档，属 M3 范围外。
- 本机 Firefox 访问境外站点依赖系统代理；这是网络环境约束，不是扩展行为。
- 验收用 Marionette 安装 `.xpi`，未逐击 `about:addons` 的图形安装向导；两者都走
  Firefox 的 AddonManager，安装产物与重启保留一致。
- 语境辅助在 M3 只返回 `disabled`/`pending`；`selected`/`low_confidence`+AI 由 M4 写入，
  扩展已按 `SenseResolution` 展示推荐义项与 AI 卡（单元测试覆盖）。
