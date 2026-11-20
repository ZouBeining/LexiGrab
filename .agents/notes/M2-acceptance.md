# M2 桌面端 MVP 验收记录

日期：2026-10-29；手工矩阵自动化补测：2026-11-07；缺陷修复复验：2026-11-15。范围与任务见 [PLAN_M2](../plans/PLAN_M2.md)，接口契约见 [BACKEND_STRUCTURE](../../docs/BACKEND_STRUCTURE.md)。

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
| T2.10 测试 | Rust 193 项（含缺陷修复回归）、前端 10 文件 75 项；`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check`、`pnpm typecheck`、`pnpm lint`、`pnpm build` 全绿 |

## 2. 自动化验证

```text
cd src-tauri && cargo test
lib 92 / cli 3 / anki 25 / card 12 / config 5 / html 14 / mdx 10 / resource 14 / router 7 / sense 12 = 193 passed, 0 failed

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

手工矩阵（真实窗口）已在 2026-11-07 用 pywinauto 自动化执行，见第 3 节。

## 3. 手工矩阵（pywinauto 自动化执行）

2026-11-07 在交互桌面用 uv 管理的 Python 环境 + pywinauto 驱动真实应用跑完整矩阵，脚本与产物在 [.agents/automation/m2-manual](../automation/m2-manual/)：

- `p1_manager.py` 源增删改/启停/排序/持久化/大词典；`p2_popup.py` 划词、焦点、快速/编辑态、时延；`p3_anki.py` 真实 Anki 制卡与错误态；`p4_lifecycle.py` 关闭到托盘/单实例/托盘菜单；`p5_media.py` 图片与音频渲染。
- 运行方式见 §3.3；JSON 结果与截图在 `artifacts/`（`p1-manager.json` … `p5-media.json`、`20-…`/`30-…`/`50-…png`）。

| 项目 | 结果 | 证据与说明 |
| --- | --- | --- |
| 记事本 / Chrome / Firefox 划词 → 弹窗 | 通过（部分浏览器未装） | Win11 记事本与 Edge 均捕获成功（Edge 弹窗 65.5ms）；Chrome、Firefox 未安装，以 Chromium 内核 Edge 代表；句子捕获取到整句 |
| 快速模式不抢焦点、编辑模式恢复焦点 | 通过 | 快速模式窗口带 `WS_EX_NOACTIVATE`/`TOPMOST`/`TOOLWINDOW`，前台始终是 Notepad；点「编辑」后前台切到弹窗、尺寸 420×560 → 560×640；关闭后 139.7ms 恢复 Notepad 前台；再次按热键可隐藏弹窗 |
| MDX 图片/音频显示与 Windows URI | 已修复（2026-11-15 复验通过） | 原失败：Oxford `apple` 的 `fruit.jpg` 与 mini `hello` 的 1×1 PNG 均为破图；audio 控件渲染但夹具 mp3 是占位字节，显示「无法播放媒体」。根因是 Wry 在 Windows 把 `http://mdx-res.localhost/{id}/...` 还原成 `mdx-res://localhost/{id}/...` 后交给处理器，`localhost` 被误读为 dict_id；协议层现跳过该 authority，复验 `apple` 正常显示 Fruit 插图。截图：`artifacts/50-popup-apple.png`、`50-popup-hello.png` |
| 本地源增删改、启停、排序、重启保持 | 通过 | 选择器添加、改名、重复路径内联拦截、上移（优先级重写 10/20）、停用/启用、删除均写盘；重启后 `mini` + Oxford 保持顺序与启用状态，启动 1.34s |
| 大词典不冻结 UI | 通过 | 41MB Oxford（59 976 条）后台加载，加载期间切到运行状态 137ms；就绪后可正常查询 `apple`（查询测试页） |
| 制卡成功 / Anki 未运行 / 字段缺失 / 同词多句 / 同语境提示 | 已修复（2026-11-15 回归测试覆盖） | 真实 Anki 26.9.3 + AnkiConnect：牌组 `LexiGrab E2E`、笔记类型 `LexiGrab Basic` 字段 `Word/Definition/Sentence/Source` 落库正确，标签 `global-dict`，本地媒体导出为 `[图片] [音频]` 占位；Anki 关闭时报 `ANKI_UNREACHABLE` 并提示「启动 Anki…」，重启后「重试保存」成功；字段缺失报 `Anki 模板缺少字段: Nope` 并显示「打开 Anki 设置」；同词不同句可连续生成两张卡。原缺陷：释义含本地图片/音频时，第二次保存不弹「此语境已存过」。原因是前端预检用原始 `definition_html` 计算指纹，导出端用剥离媒体后的 HTML；现统一走 `context_definition_text`（先剥离媒体再转纯文本），`card_tests` 覆盖预检与落库指纹一致 |
| 热键 → 可见 p95 ≤200ms | 通过 | 12 次：p50 82.3ms，p95 84.3ms（外部 `EnumWindows` 1ms 轮询，含合成按键注入） |
| 热键 → 首结果 p95 ≤350ms、本地查询 p95 <100ms | 通过 | 首结果 p50 276ms、p95 296.4ms（含 UIA 轮询开销）；本地查询沿用 M1 `bench` p95 0.43ms 与查询测试页 |
| 托盘、单实例、关闭到托盘 | 通过（a11y 已修复） | 关闭管理器只隐藏、进程存活；第二实例退出码 0 并唤起已有管理器；托盘菜单「打开管理器 / 设置 / 退出」均实测可用，托盘退出后进程结束（退出路径会注销全局热键，见 `lib.rs`）。原备注：托盘图标 UIA 名称为空（无 tooltip）；已加 `.tooltip("LexiGrab")`，2026-11-15 复验 UIA Name 为 `LexiGrab` |

### 3.1 关键测量

- 弹窗可见（12 次）：p50 82.3ms / p95 84.3ms，最小 77.5ms、最大 91.2ms。
- 首结果（同一 12 次，UIA 轮询）：p50 276ms / p95 296.4ms。
- 管理器导航切换：正常态 121ms，41MB 词典后台加载中 137ms。
- 重启：启动到管理器可见约 1.34s；源状态恢复后台加载完成 < 1s（本机）。
- 真实 Anki：7 张测试卡写入 `LexiGrab E2E`（含同词多句、同语境覆盖、离线重试），字段与标签逐条核对通过。

### 3.2 复现命令

```powershell
# 前端资源需内嵌到 exe，才会走生产 CSP/自定义协议
pnpm tauri build --debug --no-bundle

cd .agents/automation/m2-manual
uv sync
# 用 uv 安装的基座 CPython（原因见 3.3），run.py 负责把 venv site-packages 加回 sys.path
$py = (Get-Content .venv\pyvenv.cfg | Select-String '^home').ToString().Split('=')[1].Trim() + '\python.exe'
& $py run.py p1_manager.py
& $py run.py p2_popup.py
& $py run.py p3_anki.py
& $py run.py p4_lifecycle.py
& $py run.py p5_media.py
```

测试 HOME 为 `.agents/automation/m2-manual/home`，词典夹具为 `fixtures/` + `references/dict/`。副作用提示：`p3` 会在真实 Anki 的 Andy 配置内新建并保留 `LexiGrab E2E` 牌组与 7 张测试卡（可用 AnkiConnect `deleteDecks(cardsToo=true)` 清理）；`p2`/`p5` 会新开一个空白记事本窗口用于划词，不碰已有文档；`p4` 最后从托盘退出应用。运行前若 Anki 未启动，`p3` 会自动拉起 `D:\Anki\Anki.exe`。

### 3.3 环境问题与绕行（本机）

- **仓库根目录带 Low 完整性标签**（`icacls` 可见 `Mandatory Label\Low Mandatory Level`）。在其中构建/启动的 `lexigrab.exe` 进程继承 Low，无法向 Medium 窗口 `SendInput`，表现为划词一直 `CAPTURE_TIMEOUT`；把 exe 复制到 `%TEMP%`（Medium）启动后捕获正常。此前 M2 阶段把 `%APPDATA%` 写入失败归因于 Defender 受控文件夹访问，实际更可能是同一低完整性标签所致。
- **uv venv 的 `python.exe` 无法注入输入**：本机该启动器调用 `SetCursorPos`/`SendInput` 返回错误 1008；因此 `run.py` 用 uv 安装的基座 CPython + venv `site-packages` 执行脚本（依赖仍由 uv 管理），并显式写入 `mdx-res` 所需的 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--force-renderer-accessibility`。
- **`cargo build` 不等于可自动化的构建**：不带 `TAURI_ENV_*` 的 `cargo build` 会内嵌 devUrl（`localhost:1420`），窗口显示连接失败；自动化统一用 `pnpm tauri build --debug --no-bundle` 的内嵌资源产物。
- **Win11 记事本 UIA 只暴露标题栏**，正文在独立子窗口；但选区/句子捕获走剪贴板与 UIA TextPattern，实测可用。Edge（Chromium）UIA 正常。

## 4. 已知限制与后续

- M2 不实现远程源、缓存（`lookup_cache` / `sense_cache`）、本地桥接、扩展、语境推荐与 AI 释义；相关命令与事件保留给 M4。
- `saved_cards` 只用于同语境提示，不参与 Anki 唯一性；清缓存不删历史（M4 提供清理入口）。
- 设置页“重新打开首次引导”通过一次性 localStorage 标记交给管理器，不改变窗口 URL 查询参数。
- 设置中切换主题不实时推送到已打开的管理器窗口，重开/重载后应用。
- 词典链接用系统浏览器打开（`opener:allow-open-url`）；不做 WebView 内导航。
- 进程退出时未显式等待 worker 线程 join，依赖 Drop 回收（M1 设计）。
- 本机 MSVC 14.51 在编译 bundled SQLite 的 C 代码时会报 `D8050`；`[profile.dev] debug = 0` 后消失。`cargo test` 仍保留完整回溯所需的 Rust 符号。
- 计划中的 `rusqlite 0.33` 实际锁定 `0.32.1`（bundled），行为与契约一致，已在 TECH_STACK 记录。
- **已修复（2026-11-15）**：
  - `mdx-res` 图片在弹窗内破图：Wry 在 Windows 把显示用 URI `http://mdx-res.localhost/{id}/...` 还原为 `mdx-res://localhost/{id}/...` 再交给协议处理器，`localhost` 被当成 dict_id 导致 404。`core/resources.rs` 现在跳过该 authority，并新增 `mdx-res://localhost/...` 解析与端到端解析测试；`p5` 复跑 `apple` 正常显示 Oxford Fruit 插图（`artifacts/50-popup-apple.png`）。
  - 同语境提示的指纹不一致：前端预检用原始 `definition_html`，导出端用剥离本地媒体后的 HTML。新增 `anki::export::context_definition_text`，预检与导出共用同一路径（先剥离媒体再转纯文本），`card_tests` 覆盖预检命中与导出落库指纹一致。
  - 托盘图标可访问名称为空：`TrayIconBuilder` 增加 `.tooltip("LexiGrab")`，复验托盘溢出区 UIA Name 为 `LexiGrab`。
  - 仓库根目录低完整性标签仍存在；自动化继续从 `%TEMP%` 副本启动，不属代码缺陷。
