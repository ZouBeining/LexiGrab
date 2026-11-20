# M5 长跑脚本

用真实桌面端 + 本地桥接跑 8 小时稳定性测试，每 10 分钟发一次
`POST /v1/selection` 并采样内存、句柄与线程数。目标见
[PLAN_M5](../../plans/PLAN_M5.md) T5.7：8 小时内存增长 <10%，无崩溃。

## 前置

```powershell
# 前端资源要内嵌进 exe，才会走生产 CSP 与自定义协议
pnpm tauri build --debug --no-bundle

cd .agents/automation/m5-soak
uv sync
```

应用会复制到 `%LOCALAPPDATA%\Temp\lexigrab-soak\`（`%TEMP%` 不可写时自动回退）运行，
避免本机仓库低完整性标签导致无法写 `%APPDATA%`（见
[M2-acceptance](../../notes/M2-acceptance.md) §3.3）。临时 `LEXIGRAB_HOME` 是
`home/`，配置写在 `home/config/config.toml`。

## 运行

```powershell
# 完整长跑（8 小时，每 10 分钟一次）
.venv/Scripts/python.exe -u soak.py --minutes 480

# 冒烟：20 分钟、每 1 分钟一次，验证脚本与首末采样
.venv/Scripts/python.exe -u soak.py --minutes 20 --interval-min 1
```

脚本用 `src-tauri/tests/fixtures/mini.mdx` 生成临时 `LEXIGRAB_HOME`，
开启回环桥接（默认端口 12789），每次请求轮换一个命中词与一个未命中词。

## 100k 词典 bench 与 RSS

`bench_rss.py` 用 release 版 `lexigrab-dev` 查询 100,000 条合成词典，重复多轮并每 5ms
采样 RSS，输出中位数（`open_ms`、`p95_ms`、`peak_rss_mb`）。生成与运行：

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

## 产物

- `artifacts/soak.jsonl`：每次采样的时间、词、HTTP 状态、延迟、RSS、
  private bytes、句柄、线程与累计失败数。
- `artifacts/summary.json`：运行时长、采样数、失败数、p50/p95 延迟、
  首末 RSS 与增长百分比、句柄首末值与增量（跳过前 3 个预热采样）。

`artifacts/` 与 `home/` 已在 `.gitignore` 中排除；报告结论写入
[docs/performance.md](../../../docs/performance.md)。

## 判定

- 退出码 0：所有采样 HTTP 200；退出码 2：桥接未就绪；3：进程退出或有失败采样。
- RSS 增长按预热后的首末采样计算；句柄增长应接近 0。
- 若应用在长跑中退出，`soak.jsonl` 最后一行前即为崩溃点。
