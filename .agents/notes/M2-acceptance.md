# M2 桌面端 MVP 验收记录

日期：2026-10-29。范围与任务见 [PLAN_M2](../plans/PLAN_M2.md)，接口契约见 [BACKEND_STRUCTURE](../../docs/BACKEND_STRUCTURE.md)。

## 1. 交付摘要

| 任务 | 交付 |
| --- | --- |
| T2.1 AppState / Command | `core/state.rs` 统一持有配置、注册表、`CardStore`、QueryMeta 环形缓存、捕获诊断；`commands.rs` 24 个命令，查询/Anki/建库走 `spawn_blocking`，错误统一 `IpcError` |
| T2.2 mdx-res / CSP | `core/resources.rs` 解析 `mdx-res://` 与 `http://mdx-res.localhost/`，拒绝穿越/反斜杠/空段/超长/非法 UTF-8；MIME 白名单；`lib.rs` 注册协议并对 GET/HEAD 返回 nosniff + 缓存头；`tauri.conf.json` 配置 `csp` / `devCsp` |
| T2.3 弹窗快速模式 | `src/windows/popup/` 状态机 + 420×560 快速模式；启动预创建隐藏窗口；`popup://selection` 事件 + `get_current_selection` 拉取兜底；选择/编辑/关闭/跳转管理器与设置 |
| T2.4 编辑模式与制卡 | 560×640 编辑态（`set_popup_interactive` 触达）；预览/源码 tab、原句可编辑、`Ctrl+Enter` / `Esc`；导出前剥离本地媒体；四字段映射；同语境提示可覆盖 |
| T2.5 管理器 | 源列表/添加（plugin-dialog 多选 `.mdx`）/编辑/删除/启停/排序/状态；同名 `.mdd` 提示与重复路径拦截；`dict://index-progress` + 轮询；查询测试使用草稿配置 |
| T2.6 设置骨架 | 六分区；热键录制与应用（冲突回滚不写盘）；主题三选一即时预览；Anki 探测/牌组/模板/字段映射校验；语境辅助、扩展、缓存为 M3/M4 说明块 |
| T2.7 托盘 / 单实例 / 生命周期 | `single-instance` 首个注册；托盘菜单（管理器/设置/退出）；manager/settings 关闭隐藏；退出注销热键；启动顺序：配置 → 日志 → 单实例 → 托盘 → 管理器 → 预创建 popup → 热键 → 后台词典/Anki |
| T2.8 桌面句子捕获 | `platform/windows/uia.rs` 独立线程 UIA TextPattern，120ms 预算；失败/超时留空；成功后补发 `popup://selection`，前端静默更新不重置状态 |
| T2.9 同词多卡 | `db/saved_cards`（SQLite，WAL，损坏隔离重建）；NFKC + 小写 + 压缩空白 + uuid v5 指纹；`duplicate_policy = warn|allow`；始终 `allowDuplicate: true`；不写指纹/URL 进 Anki 字段 |
| T2.10 测试 | Rust 189 项、前端 10 文件 75 项；`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check`、`pnpm typecheck`、`pnpm lint`、`pnpm build` 全绿 |

## 2. 自动化验证

```text
cd src-tauri && cargo test
lib 89 / cli 3 / anki 25 / card 11 / config 5 / html 14 / mdx 10 / resource 13 / router 7 / sense 12 = 189 passed, 0 failed

cargo clippy --all-targets -- -D warnings   # clean
cargo fmt --check                            # clean

pnpm typecheck                               # clean
pnpm lint                                    # 0 error
pnpm test                                    # 10 files, 75 tests
pnpm build                                   # manager / popup / settings 三入口构建成功
```

后端冒烟（`LEXIGRAB_HOME=src-tauri/target/m2-smoke`，`start_minimized=true`）：

- `lexigrab.exe` 启动成功，日志写入 `logs/`；`cache.db` 与 WebView2 数据目录创建成功；
- 全局热键注册成功；托盘创建无警告；管理器以隐藏方式启动（`start_minimized`）；
- 修正夹具路径后无 `DICT_OPEN_FAILED`，注册表后台加载正常；
- 进程运行 6s 无崩溃后由测试脚本终止。

CLI 回归（夹具 `tests/fixtures/mini-config.toml`）：

- `lookup --word hello`：1 条 Entry，2 项资源，退出码 0；
- `sources`：`local-mini ready entries=3`；
- `bench --word hello`：100 次，open 0.6ms，p50 0.28ms，p95 0.43ms。

## 3. 手工矩阵状态

本机为无人值守会话，无法可靠驱动真实应用（记事本/浏览器划词、Anki 客户端、文件选择器）。以下项目**未执行**，需在交互桌面补测：

| 项目 | 状态 | 说明 |
| --- | --- | --- |
| 记事本 / Chrome / Firefox 划词 → 弹窗 | 待手工 | 依赖真实前台窗口与模拟按键 |
| 快速模式不抢焦点、编辑模式恢复焦点 | 待手工 | 需目视与 Alt+Tab 验证 |
| MDX 图片/音频显示与 Windows URI | 部分自动 | 协议解析/解析器测试通过，窗口内渲染未目视 |
| 本地源增删改、启停、排序、重启保持 | 待手工 | 配置持久化有单元/集成测试 |
| 大词典不冻结 UI | 待手工 | 后台加载已实现；100k 词典为 M1 基线 |
| 制卡成功 / Anki 未运行 / 字段缺失 / 同词多句 / 同语境提示 | 部分自动 | wiremock 覆盖请求形状与错误码；真实 Anki 未跑 |
| 热键 → 可见 p95 ≤200ms | 待手工 | 需真实输入设备；设计上已预创建弹窗并后置句子捕获 |
| 热键 → 首结果 p95 ≤350ms、本地查询 p95 <100ms | 部分自动 | 夹具查询 p95 0.43ms（100 次） |
| 托盘、单实例、关闭到托盘 | 待手工 | 单实例与托盘代码已接入 |

## 4. 已知限制与后续

- M2 不实现远程源、缓存（`lookup_cache` / `sense_cache`）、本地桥接、扩展、语境推荐与 AI 释义；相关命令与事件保留给 M4。
- `saved_cards` 只用于同语境提示，不参与 Anki 唯一性；清缓存不删历史（M4 提供清理入口）。
- 设置页“重新打开首次引导”通过一次性 localStorage 标记交给管理器，不改变窗口 URL 查询参数。
- 设置中切换主题不实时推送到已打开的管理器窗口，重开/重载后应用。
- 词典链接用系统浏览器打开（`opener:allow-open-url`）；不做 WebView 内导航。
- 进程退出时未显式等待 worker 线程 join，依赖 Drop 回收（M1 设计）。
- 本机 MSVC 14.51 在编译 bundled SQLite 的 C 代码时会报 `D8050`；`[profile.dev] debug = 0` 后消失。`cargo test` 仍保留完整回溯所需的 Rust 符号。
- 计划中的 `rusqlite 0.33` 实际锁定 `0.32.1`（bundled），行为与契约一致，已在 TECH_STACK 记录。
