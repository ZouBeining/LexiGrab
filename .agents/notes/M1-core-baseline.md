# M1 核心链路基线报告

日期：2026-10-29。对应 [PLAN_M1](../plans/PLAN_M1.md) T1.1 / T1.10。

## 1. 环境

| 项 | 值 |
| --- | --- |
| OS | Windows 10/11（开发机，Defender 受控文件夹访问开启） |
| Rust | 1.99.1（stable-x86_64-pc-windows-msvc） |
| 构建 | MSVC 14.51 / VS 2026 Community |
| 项目 | LexiGrab 0.1.0，release profile（codegen-units=1, lto, strip） |
| 时钟 | 2026-10-29，单机单次测量，非统计基准 |

## 2. 依赖选型偏差：libmdx → mdictlib

计划要求 libmdx git `111cab6cddb119ce35158ee1a8d641bab698c87e`。核实结果：

- GitHub 上不存在该 commit；`https://github.com/QuickDict/libmdx` 是 C 实现（mdx.c/mdd 解析），不是 Rust crate。
- crates.io 无 `libmdx` 包。

改用 `mdictlib 0.2.8`（MIT，AGPL-3.0 兼容）：

- 读取 MDX/MDD v1/v2，zlib/LZO，UTF-8/UTF-16LE/GBK/GB18030/Big5；
- 文件后端、惰性块解码，接口 `MdxFile::open/lookup/len`、`MddFile::open/lookup`；
- 自带 v2.0 写入器，用于生成测试夹具（替代 Python mdict-utils）。

适配层仍按计划保留“每文件一个 worker 线程 + 有界队列 + 超时”的结构，替换实现时只改 `parsers/mdx.rs` 内部。`TECH_STACK.md`、`BACKEND_STRUCTURE.md`、`PROMPT.md` 已同步。

另：reqwest 默认 rustls/aws-lc-sys 在本机 MSVC 编译失败（`cl D8050`），改为 `default-features = false, features = ["blocking","json","native-tls"]`，Windows 走 Schannel。

## 3. 构建时间与体积

| 指标 | 值 |
| --- | --- |
| 首次依赖拉取 + `cargo check --all-targets`（含 27 个新 crate） | ~1 分 20 秒（其中编译 ~28 秒，其余为下载/索引） |
| 增量 `cargo check --all-targets` | ~5–10 秒 |
| release 全量构建（含 Tauri + 全部 M1 依赖） | 5 分 20 秒 |
| release 增量（改 CLI 后重链） | 3 分 16 秒 |
| `cargo test` 全量（125 个测试，已构建） | 5.7 秒 |
| `target/debug` 体积 | 11.94 GB（含历史 M0 构建与测试产物） |
| `target/release` 体积 | 1.82 GB |
| `lexigrab-dev.exe`（release） | 4.0 MB |
| `lexigrab-dev.exe`（debug） | 14.5 MB |

## 4. 查询性能（release）

夹具：`tests/fixtures/mini.mdx`（3 条）；大词典：`examples/gen_large.rs` 生成的 100,000 条合成 MDX（1.23 MB，zlib），不提交。

| 场景 | 打开耗时 | p50 | p95 |
| --- | --- | --- | --- |
| mini，500 次 | 0.4 ms | 0.05 ms | 0.11 ms |
| 100k，500 次 | 0.4 ms | 0.05 ms | 0.11 ms |

- 100k 词典冷启动进程总耗时（release，含进程启动、配置加载、打开、查询、JSON 输出）：32–39 ms（首次冷缓存 371 ms，后续稳定 30–40 ms 量级）。
- 峰值工作集：19.5 MB（100k 词典，500k 次查询进程）。
- 末位词 `word99999` 可正常命中，p95 0.11 ms，远低于 100 ms 的关注阈值；M2 不需要额外索引加载策略。
- bench 输出格式：`iterations=... entries_total=... open_ms=... p50_ms=... p95_ms=... mean_ms=...`。

## 5. 交付测试

`cargo test`：125 个测试全部通过。

| 测试文件 | 数量 | 覆盖 |
| --- | --- | --- |
| lib 单元测试 | 49 | 配置/错误/模型/worker 归一化/路由/HTML/义项/Anki 映射 |
| `tests/config_tests.rs` | 5 | 夹具配置、保存重载、非法配置不写盘、首次生成、损坏备份 |
| `tests/mdx_tests.rs` | 10 | 打开、大小写/标点归一、资源/MIME、路径穿越、截断文件、worker 关闭 |
| `tests/router_tests.rs` | 7 | 三策略、优先级、禁用源、坏文件隔离、全失败 |
| `tests/html_tests.rs` | 14 | 脚本/事件/javascript:、相对/绝对/data 资源、entry 链接、超长 |
| `tests/sense_tests.rs` | 12 | 列表/段落/整条退化、空/超长、去重、词性 |
| `tests/anki_tests.rs` | 25 | wiremock 请求体、allowDuplicate、错误分类、字段映射 |
| CLI 单元测试 | 3 | HTML 输出、文本提取、百分位 |

CLI 出口验证（release）：

- `lookup --word hello --json` / `--html-out`：输出 1 条 Entry，`mdx-res://local-mini-f1/img/hello.png` 已改写，resources 2 项。
- `sources`：`local-mini  Mini  enabled=true  priority=10  ready entries=3`。
- `bench`：p50/p95 见上。
- `anki-decks` / `anki-models` / `add-note`：用本地 AnkiConnect 假服务（wiremock 之外的一次性 Python mock）验证，请求体含 `allowDuplicate: true`，字段 `Word/Definition/Sentence/Source` 正确，返回 note id；Anki 未运行时退出码 3 并打印安装指引。

## 6. 已知限制与 M2 交接

- mdictlib 的具体大小写/前后缀行为：`lookup` 先精确匹配，再按文件头 `KeyCaseSensitive` 归一化；适配层额外尝试原文/小写/大写/首字母大写四种变体，最多 3 条。M2 若要做前缀联想，可直接用 `MdxFile::locate/prefix_keys`，无需改 worker 协议。
- 大词典打开量级：元数据 + 块索引惰性加载，100k 条约 0.3–0.4 ms；内存主要随查询与条目 HTML 增长，500k 次查询峰值 20 MB 内。
- MSVC 无法编译 aws-lc-sys（cl D8050），远程 HTTPS 用 Schannel；换回 rustls 需单独验证。
- 无真实受版权词典的矩阵数据；M2 起用用户本地词典补小规模矩阵。
