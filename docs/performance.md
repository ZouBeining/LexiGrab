# 性能与可靠性

日期：2026-11-21。目标来自 [PLAN_M5](../.agents/plans/PLAN_M5.md) T5.7；历史基线见
[M1-core-baseline](../.agents/notes/M1-core-baseline.md)、[M2-acceptance](../.agents/notes/M2-acceptance.md)、
[M4-remote-baseline](../.agents/notes/M4-remote-baseline.md)。

## 1. 环境

| 项 | 值 |
| --- | --- |
| CPU | Intel Core Ultra 5 225H（14 核 14 线程） |
| 内存 | 31.4 GB |
| GPU | Intel Arc 130T（16 GB） |
| 系统 | Windows 11 家庭版 10.0.26300（x64） |
| WebView2 | 157.0.4341.60 |
| Rust | 1.100.0（release 构建） |
| Node / pnpm | 26.13.1 / 11.28.5 |

说明：本机 `%TEMP%` 指向另一个用户目录且不可写，长跑与调试产物改用
`%LOCALAPPDATA%\Temp\lexigrab-soak`；仓库带 Low 完整性标签时，运行副本必须放在普通目录，
否则无法写 `%APPDATA%`（见 [M2-acceptance](../.agents/notes/M2-acceptance.md) §3.3）。

## 2. 目标与结论

| 指标 | 目标 | 实测 | 结论 |
| --- | --- | --- | --- |
| 热键到弹窗可见 | p95 ≤200ms | p95 84.3ms（M2，12 次） | 通过（沿用 M2 证据） |
| 本地查询（热，10 万条级） | p95 <100ms | p95 **0.07ms**（M5，100k 合成词典，500 次） | 通过 |
| 首结果 | p95 ≤350ms | p95 296.4ms（M2） | 通过（沿用 M2 证据） |
| 启动到托盘 | ≤1.5s | 1.34s（M2 重启） | 通过（沿用 M2 证据） |
| 空闲内存 | ≤150MB | 46.4MB（20 分钟长跑首个采样） | 通过（短跑） |
| 500MB 词典后内存 | ≤500MB | 未覆盖 500MB 样本；100k 条 13.5MB 峰值 | 未验证 |
| 远程超时 | 配置 +200ms 内 | 300ms 配置实测 783.5ms | 偏差，见 §5 |
| 语境选择器 | p95 ≤800ms，不阻塞首屏 | 选择器额外 4.1ms（mock）；`aggregate` 首屏 0.0ms、pending=1 | 通过 |
| LLM 兜底 | 低置信度才调用，超时 15s 内静默 | 契约 15s 超时；失败不降级、不影响词典 | 通过（契约与测试） |
| 8 小时内存增长 | <10% | +0.79%（20 分钟短跑）；完整 8 小时待发布前执行 | 未完成 |

## 3. 测量方法与原始数据

### 3.1 本地查询与内存（M5）

生成 100,000 条合成 MDX（zlib 压缩 1.23MB），用 release 版 `lexigrab-dev bench`
查询 `word00042` 500 次，重复 3 轮，同时每 5ms 采样进程 RSS：

```powershell
cd src-tauri
cargo build --release --bin lexigrab-dev --example gen_large
target/release/examples/gen_large.exe 100000 target/perf/large.mdx
cd ..
.agents/automation/m5-soak/.venv/Scripts/python.exe .agents/automation/m5-soak/bench_rss.py `
  --exe src-tauri/target/release/lexigrab-dev.exe `
  --config src-tauri/target/perf/config.toml `
  --word word00042 --iterations 500 --runs 3
```

结果（三轮中位数）：

```text
open_ms=0.4  p95_ms=0.07  peak_rss_mb=13.5
run1 wall=1078ms  run2 wall=62ms  run3 wall=63ms
```

`open_ms` 是首次打开数据源的时间，100k 条词条采用块索引惰性加载；`entries_total=500`
表示 500 次查询每次命中 1 条。首轮 wall 时间包含冷启动文件缓存，稳态查询 p95 0.07ms。
与 [M1 基线](../.agents/notes/M1-core-baseline.md) 的 100k 条、20MB 内峰值一致。

### 3.2 缓存与解析（M5 复跑）

```powershell
cd src-tauri
cargo test --test perf_tests -- --ignored --nocapture
cargo test --test perf_tests -- --nocapture
```

```text
cache.get p95 = 0.021 ms                 # 1000 次，200 条预热
jsonpath 100 entries p95 = 1.587 ms      # 50 次解析
local_first first screen = 0.0 ms, pending remote = 1
aggregate 不等待远程源                    # 0.0ms 返回同步结果
selector overhead = 4.1 ms               # 选择器 mock 调用（含阈值状态机）
remote timeout elapsed = 783.5 ms (config 300ms, retry once)
```

### 3.3 长跑

脚本与说明：[.agents/automation/m5-soak](../.agents/automation/m5-soak/)。它用调试版桌面端
+ 回环桥接，每分钟发一次 `POST /v1/selection`，采样 RSS、句柄、线程与延迟。

```powershell
pnpm tauri build --debug --no-bundle
cd .agents/automation/m5-soak
uv sync
.venv/Scripts/python.exe -u soak.py --minutes 20 --interval-min 1
```

M5 短跑结果（20 分钟、1 分钟间隔）：

<!-- SOAK_RESULT -->
2026-11-21 的 20 分钟冒烟（21 次采样，每 60 秒一次）：

| 项 | 结果 |
| --- | --- |
| 采样 / 失败 | 21 / 0 |
| 桥接选择 p50 / p95 | 0.0ms / 31.0ms（含首次连接与冷启动抖动） |
| 空闲 RSS 首 / 末 | 46.4MB / 46.8MB |
| RSS 增长（跳过前 3 个预热采样） | **+0.79%** |
| 句柄首 / 末（增量） | 401 / 400（−1） |
| 线程 | 25–29，无单调增长 |

20 分钟内进程未崩溃，句柄与线程稳定；空闲内存 46.4MB，低于 150MB 目标。
这只能说明短跑无泄漏迹象，不能代替 8 小时结论。


完整 8 小时命令为 `soak.py --minutes 480`（默认 10 分钟间隔，480 次采样），
产物 `artifacts/soak.jsonl` 与 `artifacts/summary.json`。发布前必须在目标机器完成后，
把首末 RSS、增长百分比与句柄增量补到上一段；本次会话只跑了 20 分钟冒烟。

## 4. 可靠性与故障恢复

| 场景 | 行为 | 证据 |
| --- | --- | --- |
| 配置损坏 | 备份为 `.bak`，以默认值启动并记录警告 | `config_tests`、`core/config.rs` |
| 缓存库损坏 | 卡片库/查询缓存分别回退内存库并记录日志 | `state.rs`、`cache_tests` |
| 词典 worker 崩溃 | 状态转为可读错误，单源隔离，不阻断其他源 | `mdx_tests`、`router_tests` |
| 远程源失败 | 只影响该源；`local_first` 不清空本地结果 | `remote_tests`、`perf_tests` |
| 端口冲突 | 桥接状态置 `failed`，不自动换端口，换端口后重启 | `bridge_tests` |
| 进程退出 | 注销热键、停桥接、释放 SQLite | `lib.rs`、`p4` 生命周期脚本 |

## 5. 偏差与未覆盖项

- **远程超时**：计划写“配置+200ms”，但契约要求超时最多重试一次、间隔 150ms；300ms 配置实测
  783.5ms（≈2×300+150+网络）。实现以 `BACKEND_STRUCTURE` 为准，`first_hit` 同步路径会等待远程，
  `local_first`/`aggregate` 不拖首屏。
- **M5 未重跑桌面端 E2E**：本机 `references/dict` 无可用的真实词典样本，热键→弹窗、首结果、
  启动时间沿用 2026-11-07 的 M2 自动化数据；发布前应在有真实 MDX/MDD 的干净虚拟机复测。
- **500MB 词典内存**：未生成 500MB 级样本；100k 条实测 13.5MB，不能外推为 500MB 结论。
- **8 小时长跑**：脚本已就绪并通过 20 分钟冒烟，完整 8 小时未在本次会话执行。
- **LLM 兜底时延**：本次只验证契约（15s 超时、失败静默、不写缓存），未接真实模型测端到端时延。
