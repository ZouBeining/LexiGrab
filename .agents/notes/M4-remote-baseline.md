# M4 远程源、缓存与路由基线

最后更新：2026-11-20。对应 [PLAN_M4](../plans/PLAN_M4.md) T4.1–T4.6、T4.10。
契约来源：[BACKEND_STRUCTURE](../../docs/BACKEND_STRUCTURE.md) §2.2、§3、§4.2、§7.2–§7.3。

## 1. 实现落位

| 能力 | 代码 | 说明 |
| --- | --- | --- |
| 响应解析契约 | `src-tauri/src/parsers/response.rs` | `ParseContext`、`ResponseParser`、`RawEntry`、`finalize_entries`；统一清洗、512KB/20 条/绝对 http(s) 资源限制 |
| JSONPath | `src-tauri/src/parsers/jsonpath.rs` | `jsonpath-rust 1.0.6`；`$` 根数组、`$.entries[*]`、嵌套路径、数组展平 |
| 脚本沙箱 | `src-tauri/src/parsers/script.rs` | `rquickjs 0.14`；源码 ≤64KB + mtime 缓存；每次新建 Runtime；16MB / 256KB 栈 / 50–1000ms 中断器；不注入 fetch/require/setTimeout/process |
| 远程源 | `src-tauri/src/sources/remote_http.rs` | URL 模板/鉴权/超时/重试/重定向/2MB 上限；`test_query` 返回前 4KB 预览；`last_error` 驱动就绪/上次失败 |
| 源工厂 | `src-tauri/src/sources/mod.rs` | `build_source` + `DictionarySource::is_remote` |
| 缓存 | `src-tauri/src/db/cache.rs` | 同 `cache.db` 的 `lookup_cache`/`sense_cache`；WAL、busy_timeout 2s、`user_version=2`；可注入 `Clock` |
| 路由 | `src-tauri/src/core/query_router.rs` | `local_first`/`first_hit`/`aggregate`；远程任务计划、负缓存跳过、正命中停止、失败隔离 |
| 事件 | `src-tauri/src/core/events.rs` | `lookup://remote-result` / `lookup://remote-error` |
| 命令与状态 | `commands.rs`、`state.rs` | `lookup` 返回同步结果并后台跑远程任务；QueryMeta 记录 `cached_sources`/`pending_remote`；更新/删除源失效缓存 |
| UI | `src/windows/manager/*`、`settings/CacheSection.tsx`、`popup/RemoteStatus.tsx` | 远程源编辑与测试面板、缓存统计与清理、远程进度与缓存徽标 |

## 2. 关键行为

- 查询键 `trim + NFKC + 小写`；正缓存 24h（可配 1–720h），负缓存 5min（1–60min），错误不缓存。
- `first_hit`：远程源未命中缓存时同步查询（保持“首个命中即停止”）；负缓存跳过继续下一个。
- `local_first` / `aggregate`：本地与缓存命中同步返回，未命中远程源进入 `RemoteJob`，由命令层后台线程执行；远程失败只发 `lookup://remote-error`，不改主结果、不弹模态框。
- 桥接 `/v1/selection` 的 `lookup_collect_all` 现在包含远程源，沿用 3.5s 截止与 `partial`；每源先查缓存、结束后写缓存。
- 远程源状态：初始化或最近一次查询失败显示“上次失败”，下一次成功恢复“就绪”；无索引概念。
- 更新/删除数据源时按 `source_id` 失效 `lookup_cache`；`clear_lookup_cache` 不删除 `saved_cards` 与 `sense_cache`。
- 相对资源 URL 丢弃；HTML 仍走统一 ammonia 白名单；重定向跨主机停止、同主机最多 2 次。

## 3. 自动化验证

| 套件 | 数量 | 覆盖 |
| --- | --- | --- |
| `tests/parsers_tests.rs` | 10 | JSONPath 标准/非标准、缺字段、坏 URL、空数组、语法错、超限；脚本正常/编译/运行/死循环/OOM/结构/fetch/Promise |
| `tests/remote_tests.rs` | 11 | wiremock：成功、401/500 不重试、超时重试一次、跨主机重定向、2MB 超限、鉴权头、查询词编码、两种解析端到端、状态恢复、4KB 预览 |
| `tests/cache_tests.rs` | 11 | TTL/负缓存/失效/并发/损坏重建/统计与清理/sense TTL |
| `router_tests.rs` + lib 测试 | 7 + 142 | 三种策略、缓存命中/负缓存、`first_hit` 远程同步、失败隔离、条数上限 |
| 全仓 `cargo test` | 328 通过 / 3 忽略 | 忽略项为性能探针 |

Mock 端到端：`.agents/automation/m4-mock/run.py`（Python 标准库 mock + `lexigrab-dev`）8/8 检查通过：

- 本地 + 远程 JSONPath + 远程脚本三源同时返回（3 条），`source_name` 正确；
- `<script>` 被清洗、相对资源被丢弃、绝对 https 资源保留；
- HTTP 500 源被隔离，整体退出码 0；
- 空结果退出码 4，不报错。

## 4. 性能记录（2026-11-20，dev profile，本机）

来源：`cargo test --test perf_tests -- --ignored --nocapture` 与默认用例。

| 指标 | 实测 | 计划目标 |
| --- | --- | --- |
| 缓存 `get` p95（200 条、1000 次） | 0.016 ms | <5 ms |
| JSONPath 100 条响应解析 p95（每查询取 20 条） | 1.865 ms | <10 ms |
| 选择器 HTTP 额外耗时（本地 mock，含客户端构建） | 2.3 ms | ≤800 ms |
| `local_first` 首屏等待（远程 3s 延迟源） | 0.0 ms，pending=1 | 不阻塞首屏 |
| 远程超时（config 300ms，含 1 次重试 + 150ms 间隔） | 762.5 ms | 见下 |

**偏差说明**：计划 T4.10 写“远程超时 ≤配置+200ms”，但 BACKEND_STRUCTURE T4.1 要求“超时最多重试 1 次、间隔 150ms”。两者同时成立时最坏耗时约为 `2 × timeout + 150ms`（300ms 配置实测 762.5ms）。实现优先遵守契约的重试语义；`local_first`/`aggregate` 下远程在后台执行，不拖慢首屏，只有 `first_hit` 会同步等待（受每源超时与优先级约束）。后续如需严格封顶，应在 M5 增加“整次查询总截止”并把该决定写回契约。

## 5. 安全与限制

- `rquickjs` 沙箱不是强安全边界：只运行可信脚本，UI 已显示警告；无网络/文件/定时器宿主能力。
- 内存上限依赖关闭 `rust-alloc`（Cargo.toml 已设 `default-features = false, features = ["std"]`），OOM 用例验证不崩溃。
- 重定向与鉴权头：跨主机停止跟随，不存在把鉴权头转发到第三方的路径。
- 日志不记录令牌、鉴权头与完整 URL；远程错误只保留可读摘要与错误码。
- 缓存与诊断留在本机；`/v1/selection` 仍剥离本地媒体。
