---
phase: M1
title: 核心链路无 UI
depends_on: [M0]
exit_gate: 命令行完成 MDX 查词并写入 Anki 四字段卡片
estimate: 8-12 人日
status: done
---

# M1 核心链路无 UI

M1 把业务核心做成不依赖 Tauri 的 Rust 库，用开发 CLI 驱动。出口是命令行能查 `.mdx` 并输出 HTML，以及把词、释义、原句、来源写入 Anki。

## 全局依据

- 数据模型、配置、AnkiConnect、解析器契约：[BACKEND_STRUCTURE](../../docs/BACKEND_STRUCTURE.md)
- 精确依赖版本：[TECH_STACK](../../docs/TECH_STACK.md)
- 产品范围：[PRD](../../docs/PRD.md)
- 会话断点：[PROGRESS](PROGRESS.md)

类型、命令、配置字段不在本计划重复，直接按 BACKEND_STRUCTURE 实现。

## 1. 范围

**做**

- 配置加载、校验、原子保存。
- `Entry`、`ResourceRef`、`SourceConfig`、`Settings`、`AnkiPayload` 的 Rust 实现。
- libmdx 适配层、独立 worker、多文件源、查询归一化。
- MDX HTML 清洗、资源路径改写、资源收集。
- 义项提取基础：从 Entry HTML 切出 `SenseCandidate`，为 M4 的语境选择准备数据。
- `LocalFileSource`、`SourceRegistry`、本地 `QueryRouter`。
- AnkiConnect 客户端与字段映射。
- 开发 CLI：查词、列牌组/模板、制卡、压测。
- MDX 测试夹具、集成测试、性能基线。

**不做**

- Tauri Command、事件、窗口、托盘。
- 远程 HTTP、JSONPath、脚本、缓存。
- `mdx-res` 协议本身；只产出 URI。
- 桥接与扩展。
- Anki 媒体上传。
- 全部 MDX 版本的兼容承诺。

## 2. 目录

```text
src-tauri/src/
├─ lib.rs / main.rs / commands.rs（空壳）
├─ core/{mod,config,entry,error,html,query_router,registry,runtime}.rs
├─ sources/{mod,local_file,remote_http（占位）}.rs
├─ parsers/{mod,mdx}.rs
├─ anki/{mod,client,mapping}.rs
└─ bin/lexigrab-dev.rs
src-tauri/tests/{support,fixtures,config_tests,mdx_tests,router_tests,html_tests,anki_tests}
```

## 3. 任务分解

### T1.1 依赖与 libmdx 适配准备

1. 按 TECH_STACK 加入依赖；libmdx 用 git 依赖固定 `111cab6c`，写明不能用 crates.io 的 `mdx`。
2. 记录首次/增量编译时间、`target/debug` 体积。
3. 验证 libmdx `default-features = false` 是否可用；不可用保留 `icu`，不 fork。
4. 确认源码事实：`MdxReader::from_url`、`find_index`、`get_html`、`get_data`；README 的 `lookup()` 不存在。
5. 确认 `MdxReader` 非 `Send`；worker 方案在 T1.4 落地。
6. `main.rs` 仍可启动 Tauri；CLI 只链接库。

**验收**：`cargo check` 通过；CLI `--help` 可用；耗时记录。

### T1.2 测试夹具

1. `tests/fixtures/gen_fixtures.py` 用 `mdict-utils` 生成 v2.0 `mini.mdx` + `mini.mdd`，内容自拟：`hello`、`world`、`lexigrab`，含粗体、列表、一张图、一个音频占位。
2. 提交二进制夹具、脚本、版本与 sha256；CI 不跑 Python。
3. `mini-config.toml` 指向夹具。
4. 生成截断的 `broken.mdx`。
5. `fixtures/README.md` 写明内容许可、生成命令、字段。

**验收**：libmdx 能打开夹具并查到 `hello`；MDD 图片可读出，MIME 为 `image/png`。

### T1.3 配置与运行时

- 按 BACKEND_STRUCTURE §2.3 实现配置、路径解析、原子保存、损坏备份。
- 首次启动生成 `device_id`、bridge token。
- 实现 `QueryId`、`EventSink` trait，M1 用记录型 sink 测试。
- `LEXIGRAB_CONFIG_DIR` 覆盖配置目录，测试用临时目录。

**验收**：PROMPT 的 TOML 示例可加载；非法路径/重复 id/缺 `{word}` 报错；保存后重载一致；无半截文件。

### T1.4 MDX worker 与 FormatParser

1. `MdxParser::open` 创建线程，线程内 `MdxReader::from_url`，打开超时 15s。
2. 命令：`Lookup`、`Resource`、`Shutdown`，每个带一次性回复通道；有界队列，满返回 `DICT_BUSY`。
3. `lookup` 发命令后 `recv_timeout(500ms)`；超时返回 `Timeout`，下一次仍可用；连续 3 次标记 unhealthy。
4. 归一化 trim/NFKC/去首尾标点；尝试原文、小写、大写、首字母大写，按 entry_no 去重，最多 3 条。
5. 资源命令同时尝试 `/` 与 `\`，拒绝 `..`；返回字节与 MIME。
6. `Drop` 发 `Shutdown`，等待最多 1s，超时 detach。

**验收**：夹具返回 1–3 条 Entry；资源正常；截断文件返回 `Open`；worker 关闭后返回 `WorkerDead` 而非 panic。

### T1.5 HTML 清洗与资源改写

- 用 lol_html 改写 `img[src]`、`audio[src]`、`source[src]`、`video[poster]`。
- 相对路径按 `/` 分段百分号编码，写 `mdx-res://{dict_id}/{path}`，收集 `ResourceRef::Local`。
- http/https/data 保留；http/https 收集为 Remote。
- `entry://` 链接移除 href；`data:` 链接移除 href，图片/音频 data 保留。
- ammonia 白名单清洗；禁止 script/iframe/object/embed/表单/事件属性；链接加 rel。
- 单条 HTML >512KB 返回 `InvalidData`，由上层跳过。
- 黄金用例覆盖脚本、onclick、javascript:、相对/绝对/data 图片、entry 链接、超长。

**验收**：清洗后无 `<script`/`onerror`/`javascript:`；相对图片全部改写；resources 无重复。

### T1.6 LocalFileSource 与 SourceRegistry

- 每个文件一个 parser + resource provider；`dict_id -> provider`。
- 查询所有文件，单文件错误只记日志；全部失败才返回错误。
- Entry 的 `source_name = source.name`，解析器留空。
- Registry 按 enabled、priority、id 稳定排序；reload 复用未变化 worker。
- 状态：Loading / Ready{entry_count} / Failed{message}。

**验收**：多源顺序正确；禁用源不参与；坏文件不影响同源其他文件；reload 回收旧 worker。

### T1.7 QueryRouter 本地路径

- 保持 `DictionarySource` 同步签名；调用放 `spawn_blocking` + `JoinSet`。
- 实现 first_hit、aggregate、local_first 的本地语义；`EventSink` 预留远程。
- 单源失败只记录 `SourceFailure`；有成功结果就不整体失败。
- 同一 source 内按 `(source_id, headword)` 去重，应用条数上限。

**验收**：三种策略测试通过；失败隔离；顺序稳定。

### T1.8 AnkiConnect 客户端

- 按 BACKEND_STRUCTURE §6 实现 `version`、`deck_names`、`model_names`、`model_field_names`、`add_note`。
- 阻塞客户端，1.5s 连接超时、3s 总超时，不重试。
- 连接失败、API 错误、重复、字段缺失分类。
- 导出前校验四个字段存在；HTML 转义与换行处理。
- 空 source 写“未知来源”；空 sentence 写空字符串。
- **始终 `allowDuplicate: true`**；同词多卡从 M1 起就是默认行为，应用层指纹提示留到 M2。

**验收**：wiremock 断言请求体；连接拒绝/duplicate/字段缺失映射正确。

### T1.9 开发 CLI

`lexigrab-dev` 支持：

```text
lookup      --config <path> --word <word> [--source <id>] [--json] [--html-out <file>]
sources     --config <path>
anki-decks  --config <path>
anki-models --config <path>
add-note    --config <path> --word <word> --definition-file <file> [--sentence <text>] [--source <name>]
bench       --config <path> --word <word> [--iterations 100]
```

退出码：0 成功；2 配置/源错误；3 Anki 错误；4 查询无结果。错误一行到 stderr，不打印堆栈。

**验收**：夹具查 `hello`；Anki 运行时制卡成功；Anki 关闭时输出安装指引；bench 输出 p50/p95。

### T1.10 性能与报告

- 用 mini 夹具测冷开/热查；用一份真实大词典（不提交）测 10 万级条目。
- 记录打开耗时、查询 p95、内存、二进制体积、测试耗时。
- 写 `.agents/notes/M1-core-baseline.md`。
- 若 10 万级 `find_index` 超 100ms，定位索引加载还是单次查找，给 M2 加载策略。

### T1.11 义项提取基础

- 引入 `scraper` 0.24.0（ISC），解析清洗后的 `definition_html`。
- 优先取顶层 `ol/ul > li`；没有列表时取顶层 `p` 分组；仍无法拆分则整条 Entry 一个候选。
- 生成 `SenseCandidate { id, entry_index, source_name, headword, part_of_speech, definition_text, definition_html }`。
- 候选最多 20 个；空文本/过短丢弃；词性尽力提取，失败为 `null`。
- 只做提取，不调用选择器/LLM；这些在 M4。
- 测试：列表型、段落型、无结构整段、空标签、超长候选。

**验收**：夹具的列表释义能拆出多个候选；无法拆分时退化为单候选且不报错。

## 4. 阶段决策

| 决策 | 结论 |
| --- | --- |
| libmdx 获取 | git 固定 rev，不用 crates.io 同名包 |
| 并发模型 | 每文件独立线程 + 有界命令队列，不用 `Mutex<MdxReader>` |
| 查询变体 | 原文、小写、大写、首字母大写，最多 3 条 |
| HTML 安全 | 先改写资源，再 ammonia 清洗；脚本与事件属性一律移除 |
| 媒体导出 | M1 不处理，M2 剥离 |
| 义项切分 | 列表/段落启发式；无法切分时整条 Entry 一个候选，不假装精确 |

## 5. 风险

| 风险 | 对策 |
| --- | --- |
| libmdx 新、文档与代码不一致 | 以固定 rev 源码为准，适配层隔离 |
| 依赖重、编译慢 | 记录基线，CI 缓存；评估关闭 `icu` |
| 部分词典不兼容 | 真实词典矩阵记录，不承诺全格式 |
| 夹具生成失败 | 先用真实词典验证链路；不提交受版权保护的词典 |
| CSS/路径千奇百怪 | 白名单清洗；资源失败静默；M2 CSP 兜底 |
| AnkiConnect 版本差异 | 只用 v6 核心 action，错误原样进日志 |

## 6. 测试命令

```powershell
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run --bin lexigrab-dev -- lookup --config tests/fixtures/mini-config.toml --word hello --json
```

## 7. 提交拆分

```text
feat(core): add settings model and atomic config store
feat(parsers): add mdx worker adapter
feat(core): sanitize and rewrite mdx html
feat(sources): add local file source and registry
feat(core): add query router with three strategies
feat(anki): add ankiconnect client and field mapping
feat(cli): add lexigrab-dev commands
test: add mini mdx fixtures and integration tests
test: cover sense extraction fallbacks
docs(notes): record m1 core baseline
```

## 8. 出口检查清单

- [x] 夹具可查 `hello`，资源可读，URI 已改写。
- [x] `SenseCandidate` 可从结构化释义切出，无法切分时单候选。
- [x] `source_name` 由 source 层填写。
- [x] 三种策略有测试，失败源不阻断。
- [x] Anki 四字段可写入，模板校验生效。
- [x] 未运行/重复/字段缺失有可读错误。
- [x] CLI 可查词、列牌组/模板、制卡、压测。
- [x] `cargo test`、clippy、fmt 全绿。
- [x] 无 `unwrap()` / `expect()` 进入非测试代码。
- [x] 性能基线报告完成。

## 9. 交接给 M2

M2 复用 Entry/Config/Registry/QueryRouter/EventSink/worker；M2 只加 `spawn_blocking`、Tauri 事件与 mdx-res 协议。M1 报告需说明 libmdx 的大小写/前后缀行为与大词典打开量级。

