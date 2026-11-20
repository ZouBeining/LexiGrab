# 安全审查

日期：2026-11-21。范围：`0.1.0` 桌面端、Firefox 扩展与本地桥接。结论按
[PLAN_M5](../.agents/plans/PLAN_M5.md) T5.8 的十项逐条给出；证据优先指向自动化测试与
可复现命令，不复制字段表。

结论：**高危 0 项，中危 2 项（均为已知且已文档化的设计选择），低危 3 项**。

## 1. CSP 只允许本地脚本

- `src-tauri/tauri.conf.json` 的生产 `csp` 为 `default-src 'self'; script-src 'self';
  object-src 'none'; base-uri 'self'; frame-ancestors 'none'`，只放行 `mdx-res:` 与
  `http://mdx-res.localhost` 资源，`connect-src` 仅 `ipc:`。生产 CSP 不含 `ws:`。
- `devCsp` 额外放行 Vite 开发服务器（`http://localhost:1420 ws://localhost:1420`）与
  内联/求值脚本，只在 `tauri dev` 使用，不进入安装包。
- 扩展内容脚本渲染在 Shadow DOM，释义经 DOMPurify 二次清洗，不执行词典脚本。

结论：符合。注意 `style-src` 含 `'unsafe-inline'`（Tailwind 运行时样式需要），不影响脚本执行。

## 2. 词典 HTML 清洗

- 所有词条 HTML 先经 `lol_html` 改写资源路径，再经 `ammonia` 白名单清洗：
  `src-tauri/src/core/html.rs::sanitize_html` 只允许 http/https/mdx-res/data（data 仅限媒体标签），
  移除 `script`、事件属性与 `entry:`/`javascript:` 链接，外链补 `rel="noopener noreferrer"`。
- 回归测试：`tests/html_tests.rs` 的 `removes_scripts_and_event_handlers`、
  `removes_javascript_links_and_keeps_safe_links`、`removes_entry_links`、
  `sanitize_strips_dangerous_markup_without_rewriting`、`sanitize_removes_unsupported_schemes`。
- 远程源响应同样经 `finalize_entries` 清洗并限制 512KB/20 条，脚本解析器不注入宿主能力。

结论：符合。

## 3. `mdx-res` 路径与 MIME

- 只接受 GET/HEAD；分段解码后拒绝 `..`、`.`、空段、反斜杠、非法百分号编码与超长路径，
  并跳过 Windows 的 `localhost` authority（Wry 行为，见 `core/resources.rs`）。
- MIME 采用白名单（图片/音频/字体），非白名单返回 403；响应带 `X-Content-Type-Options: nosniff`。
- 回归测试：`tests/resource_tests.rs` 的 `traversal_requests_are_bad_requests`、
  `non_whitelisted_mime_is_forbidden`、`mime_whitelist_rejects_scripts_and_markup`、
  `invalid_utf8_path_is_bad_request`、`safely_built_parsed_resource_rejects_traversal`。

结论：符合。

## 4. 桥接边界

- 只绑 `127.0.0.1`（`bridge/mod.rs` 监听与 `tests/bridge_tests.rs::real_port_serves_health_on_loopback`）；
  没有 `0.0.0.0`。
- 校验顺序固定：预检放行 → Origin（存在时必须以 `moz-extension://` 开头）→ 令牌桶
  （20 次/秒、突发 40）→ `X-Auth-Token` 常量时间比较。
- 请求体上限 1MB（`DefaultBodyLimit`），处理超时 10s；401/403/413/429 都补 CORS，扩展能读取错误码。
- 令牌轮换即时生效，旧令牌立即 401；令牌错误响应不泄露期望值。
- 回归测试：`missing_or_wrong_token_is_401_with_cors`、`foreign_origin_is_403_with_cors`、
  `oversized_body_is_413_with_cors`、`rate_limiter_rejects_burst_overflow_with_cors`、
  `unknown_route_is_404_with_cors`。

结论：符合。残余风险见第 10 节（未签名、明文令牌）。

## 5. 日志不记敏感内容

- 令牌只在桥接请求头与配置中使用，`tracing` 调用没有令牌字段；`bridge/error.rs` 也不返回期望值。
- 选中文本与原句从不写入日志；查询相关日志只有 `query_id`、`source_id`、耗时与错误码。
- 远程请求失败原先可能把 `reqwest::Error` 的完整 URL 带进错误文案，现已统一
  `reqwest::Error::without_url()`（`sources/remote_http.rs`、`core/sense/{selector,llm}.rs`），
  日志与界面只保留失败原因。
- 桥接内部错误（`BridgeError::internal`）会记录 `detail`；detail 来自后端错误链，已按上述规则去掉 URL，
  但仍可能包含本地路径。桌面端日志保存在本机 `%LOCALAPPDATA%/LexiGrab/logs`。

结论：符合；残余风险：本地路径会进入日志（低危，仅本机可读）。

## 6. 令牌存储与配对

- 令牌为 32 字节随机 hex，保存在用户配置 `config.toml` 的 `[bridge] token`，明文存储；
  文档与设置页明确说明“明文存本机用户配置，不声称加密”，扩展只在 background 内存中按需读取。
- 令牌不写入日志、不进入 Anki 字段、不出现在扩展页面上下文。

结论：符合既定设计（明文存储是已知限制，写入 README 与用户指南）。

## 7. 脚本沙箱

- `parsers/script.rs` 每次查询新建 QuickJS `Runtime`/`Context`，限制 16MB 内存、256KB 栈、
  50–1000ms 中断器；不注入 `fetch`/`require`/定时器/文件系统，脚本必须定义 `parse()` 返回条目数组。
- 脚本源码上限 64KB，初始化时校验文件存在与大小。
- 管理器脚本编辑器显示“只运行可信脚本”警告；文档说明沙箱不是强隔离，只运行可信脚本。

结论：符合；残余风险：QuickJS 沙箱不是进程级隔离（已文档化）。

## 8. 依赖与许可证审计

- `deny.toml` 限制许可证白名单、未知 registry/git 源与 wildcard 依赖；CI 运行
  `cargo deny check licenses bans sources`。
- 许可证清单见 [THIRD_PARTY_LICENSES.md](../THIRD_PARTY_LICENSES.md)：直接依赖均为
  MIT/Apache-2.0/ISC/BSD/MPL-2.0/Zlib/Unicode-3.0 等 AGPL-3.0 兼容许可，无未知 copyleft。
- `pnpm licenses list` 人工核对前端依赖；字体不打包。

结论：见清单与 CI 结果。已知偏差：计划中的 libmdx 不可达，实际使用 `mdictlib 0.2.8`（MIT）。

## 9. 无遥测 / 无 CDN / 无远程字体

- 前端与扩展不加载分析脚本、不引用 CDN、不下载字体；样式由本地 Tailwind 构建产物提供。
- 网络请求只有三类：用户配置的远程词典、用户显式启用的语境选择器/LLM、回环桥接。
- 词典 HTML 里的外部 `https` 图片属于词典内容，按原样加载（生产 CSP 的 `img-src` 允许 `http(s)`
  词典内容）；这一行为在用户指南与已知限制中说明，不接入遥测。

结论：符合。

## 10. 发布与运行边界

- 安装包未签名（NSIS），发布说明与 README 明确 SmartScreen 可能拦截；不伪造签名。
- 扩展 `.xpi` 未签名，需临时加载或自行签名；扩展需要 `moz-extension://` Origin 与配对令牌。
- 应用只绑定回环桥接，不监听公网；退出时注销热键、关闭桥接。

结论：符合，已在发布说明中披露。

## 复现命令

```powershell
cd src-tauri
cargo test --test bridge_tests --test resource_tests --test html_tests
cargo deny check licenses bans sources   # 需要安装 cargo-deny
pnpm licenses list                       # 根目录
pnpm -C extension test
```

## 残余风险汇总

| 等级 | 项 | 处置 |
| --- | --- | --- |
| 中 | 令牌明文存于本机配置 | 文档披露；不声称加密；可随时轮换 |
| 中 | QuickJS 沙箱非进程隔离 | UI 警告 + 文档只运行可信脚本 |
| 低 | 内部错误日志可能含本地路径 | 仅本机日志；已去除 URL |
| 低 | 安装包/扩展未签名 | 发布说明披露 |
| 低 | 外部词典图片可加载 https 资源 | 属于词典内容；CSP 已限制脚本 |
