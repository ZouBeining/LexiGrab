# BACKEND_STRUCTURE

本文件定义 LexiGrab 后端的数据结构、配置、存储、IPC 与本地 HTTP 契约。它是实现的唯一技术契约来源；系统分层见 [ARCHITECTURE.md](ARCHITECTURE.md)，组件位置见 [ROADMAP](../ROADMAP.md)，各阶段的执行顺序见 [.agents/plans](../.agents/plans/)，产品范围见 [PRD.md](PRD.md)，交互路径见 [APP_FLOW.md](APP_FLOW.md)。

## 1. 运行时结构

```text
Tauri 主进程（Windows）
├─ AppState
│  ├─ Settings（RwLock）
│  ├─ SourceRegistry（本地/远程源）
│  ├─ CardStore / CacheStore（SQLite）
│  ├─ QueryMeta 环形缓存与语境结果
│  └─ 捕获诊断、热键状态
├─ QueryRouter（按查询临时构建）
├─ BridgeService（127.0.0.1）
├─ 词典 worker 线程（每个 MDX 文件一个）
├─ Tokio 运行时（命令、桥接、远程 HTTP）
└─ WebView 窗口：manager / popup / settings
```

### 1.1 线程与阻塞边界

- `mdictlib` 的 `MdxFile`/`MddFile` 为文件后端解析器；每个 `.mdx` 文件仍由独立的标准线程持有，外部只保留命令通道，便于超时、回收与替换实现。
- 每个 worker 的查询超时 500ms，打开超时 15s。命令队列有界，满时返回 `DICT_BUSY`。
- 本地查询、AnkiConnect、远程 HTTP、SQLite 都在 `spawn_blocking` 中执行，不阻塞异步运行时。
- QuickJS 脚本在独立调用中创建 Runtime，超时 200ms（范围 50–1000ms），内存上限 16MB。
- worker 崩溃时返回可读错误；下一次查询最多自动重启一次。

## 2. 数据模型

### 2.1 Entry

```rust
pub struct Entry {
    pub headword: String,
    pub definition_html: String,
    pub phonetic: Option<String>,
    pub resources: Vec<ResourceRef>,
    pub source_name: String,
}

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResourceRef {
    Local { dict_id: String, path: String },
    Remote { url: String },
}
```

规则：

- 解析器返回 `source_name = ""`，由数据源层统一填写。
- `definition_html` 已经过 ammonia 清洗，资源路径已改写。
- 本地资源逻辑 URI：`mdx-res://{dict_id}/{percent-encoded path}`。
- IPC JSON 字段使用 snake_case，与 `src/lib/types.ts` 保持一致。

### 2.2 SourceConfig

```rust
pub struct SourceConfig {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub kind: SourceKind,
    pub enabled: bool,
    pub priority: i32,
}

#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceKind {
    LocalFile { paths: Vec<PathBuf> },
    RemoteHttp {
        url_template: String,
        response_parser: ResponseParserConfig,
        auth_header: Option<String>,
        auth_value: Option<String>,
        timeout_ms: u64,
    },
}

#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ResponseParserConfig {
    Jsonpath { expression: String },
    Script { path: PathBuf, timeout_ms: Option<u64> },
}
```

- `id` 由后端生成：`local-<uuid8>`、`remote-<uuid8>`。
- 本地路径相对于配置文件目录解析；绝对路径原样使用。
- 同一路径不得出现在多个源中。
- 远程模板必须恰好包含一个 `{word}`，scheme 只允许 `http/https`。

### 2.3 Settings

```toml
[meta]
schema_version = 2
device_id = "<uuid>"              # 首次启动生成，用于诊断与后续多端同步

[general]
hotkey = "CmdOrCtrl+Shift+D"
theme = "figma"                   # figma | apple | cursor
query_strategy = "local_first"    # local_first | first_hit | aggregate
max_entries_per_source = 3
max_entries_total = 20
start_minimized = false

[anki]
endpoint = "http://localhost:8765"
deck = "Default"
model = "Basic"
tags = ["global-dict"]
duplicate_policy = "warn"         # warn | allow
field_map = { word = "Word", definition = "Definition", sentence = "Sentence", source = "Source" }

[bridge]
enabled = true
port = 12787
token = "<32-byte hex>"

[cache]
ttl_hours = 24
negative_ttl_minutes = 5

[sense]
enabled = false
selector_endpoint = "http://127.0.0.1:8787/v1/select"
selector_timeout_ms = 800
selector_allow_remote = false
low_confidence = 0.55
high_confidence = 0.72
max_senses = 20

[llm]
enabled = false
endpoint = "http://127.0.0.1:11434/v1/chat/completions"
model = ""
api_key_env = "LEXIGRAB_LLM_API_KEY"
timeout_ms = 15000
max_tokens = 320
allow_remote = false
cache_ttl_hours = 168

[[sources]]
id = "local-00000001"
name = "Mini"
type = "local_file"
enabled = true
priority = 10
paths = ["tests/fixtures/mini.mdx"]
```

迁移规则：

- `schema_version = 1` 的 `anki.allow_duplicate = false` 迁移为 `duplicate_policy = "warn"`，`true` 迁移为 `"allow"`。
- 新增 `[sense]` / `[llm]` 缺省为关闭；`api_key` 不写入配置，只读环境变量。
- 配置保存前校验阈值 `0 < low < high <= 1`、端点 scheme、超时范围。

保存规则：

1. 校验失败不写盘。
2. 写 `config.toml.tmp`，`sync_all` 后替换。
3. Windows 覆盖前删除旧文件；失败保留 tmp 并返回错误。
4. 加载失败时把原文件重命名为 `config.toml.bak-<timestamp>`，用默认配置启动，并提示用户。

### 2.4 AnkiPayload

```rust
pub struct AnkiPayload {
    pub word: String,
    pub definition_html: String,
    pub sentence: Option<String>,
    pub source: String,
    /// 追加标签；AI 释义卡带 `ai-generated`，缺省为空。
    #[serde(default)]
    pub tags: Vec<String>,
}
```

牌组、模板、字段映射只来自 `Settings.anki`，payload 不携带。

### 2.5 QueryMeta

```ts
interface QueryMeta {
  query_id: string;
  cached_sources: { source_id: string; source_name: string }[];
  pending_remote: number;
}
```

只保留最近 8 个 query_id。`lookup` 返回 `Entry[]`，元数据由 `get_query_meta` 单独读取。

`get_cache_stats` 返回缓存统计：

```ts
interface CacheStats {
  entries: number;           // lookup_cache 行数
  positive_entries: number;  // 非空结果
  empty_entries: number;     // 负缓存
  sense_entries: number;     // sense_cache 行数
  bytes: number;             // entries_json 总字节
}
```

### 2.6 语境辅助模型

```rust
#[derive(Debug, Clone, Serialize)]
pub struct SenseCandidate {
    pub id: String,                 // e{entry_index}-s{sense_index}
    pub entry_index: usize,
    pub source_name: String,
    pub headword: String,
    pub part_of_speech: Option<String>,
    pub definition_text: String,
    pub definition_html: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SenseStatus {
    Disabled,
    NoContext,
    Selected,
    Ambiguous,
    LowConfidence,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct SenseResolution {
    pub query_id: String,
    pub headword: String,
    pub status: SenseStatus,
    pub selected_sense_id: Option<String>,
    pub confidence: Option<f32>,    // 0..1，选择器自评
    pub selector_model: Option<String>,
    pub ai: Option<AiDefinition>,
    pub error: Option<String>,
    /// 本次候选义项快照；UI 据此高亮与切换，事件与 `get_sense_resolution` 都带。
    #[serde(default)]
    pub candidates: Vec<SenseCandidate>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiDefinition {
    pub definition_text: String,
    pub part_of_speech: Option<String>,
    pub model: String,
    pub confidence: Option<f32>,
    pub based_on_sense_id: Option<String>,
    pub generated_at: i64,
}
```

规则：

- `SenseCandidate` 从 `Entry.definition_html` 提取；无法拆分的条目整体作为一个候选。
- `selected_sense_id` 必须属于本次候选集合。
- `SenseResolution` 不改变 `Entry[]`；UI 同时保留全部义项。
- AI 定义是纯文本，不包含 HTML；写入 Anki 时转义并包一层 `<div>`。
- `confidence` 是模型自评，不是校准后的真实概率；阈值可配置。

### 2.7 选区上下文

M0 的 `SelectionContext` 只有文本/锚点/应用信息。语境辅助需要原句，M2 扩展为：

```rust
pub struct SelectionContext {
    pub text: String,
    pub sentence: Option<String>,   // 新增；桌面 UIA 或扩展桥接提供
    pub url: Option<String>,        // 扩展提供；桌面为空
    pub title: Option<String>,
    pub domain: Option<String>,
    pub anchor: Option<ScreenAnchor>,
    pub foreground_hwnd: i64,
    pub app_name: String,
    pub method: CaptureMethod,
    pub captured_at_ms: i64,
    pub elapsed_ms: f64,
}
```

新增字段必须向后兼容：Rust serde 用 `#[serde(default)]`，TS 老代码可忽略。

## 3. SQLite 缓存

位置：`%LOCALAPPDATA%\LexiGrab\cache.db`。

```sql
CREATE TABLE IF NOT EXISTS lookup_cache (
  source_id    TEXT NOT NULL,
  query_key    TEXT NOT NULL,
  entries_json TEXT NOT NULL,
  is_empty     INTEGER NOT NULL DEFAULT 0,
  created_at   INTEGER NOT NULL,
  expires_at   INTEGER NOT NULL,
  PRIMARY KEY (source_id, query_key)
);
CREATE INDEX IF NOT EXISTS idx_lookup_cache_expires
  ON lookup_cache(expires_at);

CREATE TABLE IF NOT EXISTS saved_cards (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  word            TEXT NOT NULL,
  sentence_key    TEXT NOT NULL,     -- uuid v5(normalized sentence)
  definition_key  TEXT NOT NULL,     -- uuid v5(normalized definition text)
  source          TEXT NOT NULL,
  note_id         INTEGER,
  created_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_saved_cards_context
  ON saved_cards(word, sentence_key, definition_key);

CREATE TABLE IF NOT EXISTS sense_cache (
  context_key     TEXT PRIMARY KEY,   -- uuid v5(word|sentence|senses|model)
  resolution_json TEXT NOT NULL,
  created_at      INTEGER NOT NULL,
  expires_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sense_cache_expires
  ON sense_cache(expires_at);
```

规则：

- `saved_cards` 只用于“同词同句同释义”提示与本地历史，不参与 Anki 唯一性。
- 指纹：NFKC + 小写 + 压缩空白后，用 uuid v5 生成；释义取纯文本。
- `sense_cache` 缓存选择器/LLM 结果，TTL 7 天；key 含 model 与提示词版本；不缓存失败。
- `lookup_cache` 只缓存 HTTP 远程源；本地 MDX 查询不经过缓存（延迟已满足首屏预算）。
- 清除“词典缓存”不默认清除 `saved_cards`（历史）与 `sense_cache`；设置页分别提供清理。
- `PRAGMA journal_mode = WAL`、`busy_timeout = 2000`、`user_version` 记录 schema 版本。
- 查询键：trim + NFKC + 小写。
- 正结果 TTL 24h；空结果 TTL 5 分钟；错误不缓存。
- 过期即 miss，后台清理。
- 源修改/删除时失效该 source_id。
- 数据库损坏时重命名为 `cache.db.corrupt-<timestamp>` 并重建。
- 所有 SQL 在 `spawn_blocking` 中执行，连接由 `Mutex<Connection>` 保护。

## 4. IPC 命令

| 命令 | 参数 | 返回 | 说明 |
| --- | --- | --- | --- |
| `lookup` | `query`, `source_ids?`, `query_id?`, `sentence?` | `Entry[]` | 同步本地与缓存结果；远程异步事件追加，有原句时后台触发语境辅助 |
| `list_sources` | 无 | `SourceConfig[]` | 仅配置 |
| `add_source` | `config` | `SourceConfig` | 保存并异步打开 |
| `update_source` | `id`, `config` | `SourceConfig` | 保存并重载 |
| `remove_source` | `id` | `void` | 关闭 worker 并删除配置 |
| `export_to_anki` | `payload` | `{ note_id, exact_context_seen }` | 始终允许重复；提示只用于 UI |
| `check_card_context` | `payload` | `{ seen, note_id?, created_at? }` | 保存前同语境检查 |
| `get_anki_decks` | 无 | `string[]` | AnkiConnect |
| `get_anki_models` | 无 | `{ name, fields }[]` | AnkiConnect |
| `get_settings` | 无 | `Settings` | 含令牌，UI 遮罩 |
| `update_settings` | `settings` | `Settings` | 校验、应用热键、保存 |
| `get_runtime_status` | 无 | `RuntimeStatus` | 热键、版本、路径 |
| `get_source_statuses` | 无 | `SourceStatus[]` | 运行时状态 |
| `test_source_query` | `config`, `query` | `TestResult` | 草稿配置，不写盘；远程失败放在 `error`，附 `raw_preview`（前 4KB） |
| `get_current_selection` | 无 | `SelectionContext \| null` | 弹窗挂载兜底 |
| `set_popup_interactive` | `interactive` | `void` | 切换编辑焦点 |
| `hide_popup` | 无 | `void` | 关闭弹窗 |
| `open_window` | `manager \| settings` | `void` | 白名单 |
| `probe_anki` | 无 | `AnkiStatus` | 只读探测 |
| `open_config_dir` | 无 | `void` | 打开配置目录 |
| `get_query_meta` | `query_id` | `QueryMeta \| null` | 缓存/远程标记 |
| `show_window` | `label` | `void` | 白名单显示窗口（M0 遗留，供诊断页使用） |
| `debug_trigger_capture` | 无 | `CaptureResult` | 绕过热键触发一次捕获（开发诊断） |
| `get_capture_stats` | 无 | `CaptureStats` | 捕获耗时与成功率样本 |
| `get_last_capture` | 无 | `CaptureResult \| null` | 弹窗恢复捕获失败态 |
| `get_sense_resolution` | `query_id` | `SenseResolution \| null` | 语境推荐结果 |
| `test_sense_selector` | `word`, `sentence` | `SenseResolution` | 设置页测试，不写缓存 |
| `test_llm` | `word`, `sentence`, `senses` | `AiDefinition` | 设置页测试，不写缓存 |
| `clear_sense_cache` | 无 | `{ removed }` | 清理语境缓存 |
| `get_cache_stats` | 无 | `CacheStats` | 词典/语境缓存统计 |
| `clear_lookup_cache` | 无 | `{ removed }` | 清理词典缓存，保留 `saved_cards` 与 `sense_cache` |
| `get_bridge_status` | 无 | `BridgeStatus` | 桥接状态 |
| `regenerate_bridge_token` | 无 | `string` | 轮换令牌 |

### 4.1 错误边界

```rust
pub struct IpcError {
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
}
```

命令返回 `Result<T, IpcError>`。固定错误码：

```text
CONFIG_INVALID
SOURCE_NOT_FOUND
SOURCE_DUPLICATE
DICT_OPEN_FAILED
DICT_NOT_READY
DICT_BUSY
QUERY_EMPTY
ANKI_UNREACHABLE
ANKI_API_ERROR
ANKI_DUPLICATE
ANKI_FIELD_MISSING
SENSE_DISABLED
SENSE_NO_CONTEXT
SENSE_SELECTOR_FAILED
SENSE_LOW_CONFIDENCE
LLM_DISABLED
LLM_REMOTE_BLOCKED
LLM_TIMEOUT
LLM_BAD_RESPONSE
CARD_CONTEXT_SEEN
HOTKEY_CONFLICT
WINDOW_NOT_FOUND
RESOURCE_NOT_FOUND
BRIDGE_PORT_IN_USE
AUTH_FAILED
INTERNAL
```

`message` 是中文短句，`detail` 只放可公开诊断信息。日志保存完整错误链，前端不显示堆栈。

### 4.2 事件

| 事件 | 载荷 |
| --- | --- |
| `lookup://remote-result` | `{ query_id, source_id, source_name, entries, cached, elapsed_ms }` |
| `lookup://remote-error` | `{ query_id, source_id, code, message }` |
| `lookup://sense-result` | `SenseResolution` |
| `dict://index-progress` | `{ source_id, path, phase, entry_count?, message? }` |
| `popup://selection` | `SelectionContext` |
| `popup://capture-error` | `CaptureError`（M0 起；捕获失败时弹窗进入错误态） |
| `runtime://hotkey-error` | `{ message }` |
| `runtime://anki-status` | `AnkiStatus` |
| `bridge://status` | `BridgeStatus` |
| `popup://edit-ready` | 无 |

事件名与载荷类型集中在 `src/lib/events.ts`。

## 5. 本地 HTTP 桥接

### 5.1 端点

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| GET | `/v1/health` | 是 | `{ ok, app, version, api_version }` |
| POST | `/v1/selection` | 是 | 划词查询；返回 `sense_available` |
| GET | `/v1/sense/{query_id}` | 是 | 语境推荐；完成前 202 |
| POST | `/v1/anki` | 是 | 制卡；返回 `exact_context_seen` |
| OPTIONS | 以上 | 否 | CORS 预检 |

### 5.2 请求/响应

**POST /v1/selection**

```json
{
  "word": "serendipity",
  "sentence": "It was pure serendipity that we met.",
  "url": "https://example.com/article",
  "title": "An article",
  "domain": "example.com"
}
```

```json
{
  "query_id": "uuid",
  "entries": [],
  "source_names": ["CC-CEDICT"],
  "partial": false,
  "sense_available": true,
  "elapsed_ms": 42
}
```

- `lookup_collect_all` 等待所有启用源，截止 3.5s，返回已收集结果与 `partial`。
- 结果按 `max_entries_per_source` 与 `max_entries_total` 截断。
- 返回值里的 `definition_html` 已剥离本地媒体（`[图片]`/`[音频]`/`[视频]`）并清空 `resources`：浏览器无法读取 `mdx-res`，扩展只展示文本与远程资源。
- `sense_available` 为真时，扩展可轮询 `/v1/sense/{query_id}`；不阻塞首屏。为真时桌面端登记该 `query_id` 供后续查询。

**GET /v1/sense/{query_id}**

- 已完成：200 + `SenseResolution`。
- 进行中：202 + `{ "status": "pending" }`。
- 未知/过期：404。
- 未启用语境辅助：200 + `{ "status": "disabled" }`。

**POST /v1/anki**

```json
{
  "word": "serendipity",
  "definition_html": "<div>...</div>",
  "sentence": "It was pure serendipity that we met.",
  "source": "CC-CEDICT"
}
```

返回 `{ "note_id": 12345, "exact_context_seen": false }`。定义 HTML 上限 512KB，服务端再次清洗并剥离本地媒体。`exact_context_seen` 只提示同词同句同释义，不影响写入。

**错误响应**

所有非 2xx 响应（含 401/403/413/429）都带 CORS 头，正文为：

```json
{ "code": "AUTH_FAILED", "message": "令牌缺失或错误，请在扩展设置中重新配对" }
```

| 状态码 | `code` | 触发条件 |
| --- | --- | --- |
| 400 | `INVALID_REQUEST` | `word` 为空或过长、`definition_html` 为空 |
| 401 | `AUTH_FAILED` | 缺失或错误的 `X-Auth-Token` |
| 403 | `ORIGIN_REJECTED` | Origin 存在但不是 `moz-extension://` |
| 404 | `NOT_FOUND` | 未知端点，或 `/v1/sense` 的 query_id 未知/过期 |
| 413 | `PAYLOAD_TOO_LARGE` | 请求体超 1MB，或定义 HTML 超 512KB |
| 422 | `ANKI_FIELD_MISSING` | 模板缺少字段 |
| 429 | `RATE_LIMITED` | 超过 20 次/秒（突发 40） |
| 502 | `ANKI_API_ERROR` | AnkiConnect 返回错误 |
| 503 | `ANKI_UNREACHABLE` | AnkiConnect 不可达 |
| 504 | `TIMEOUT` | 单次处理超过 10s |
| 500 | `INTERNAL` | 未归类错误，细节只进日志 |

`message` 是中文短句，不含堆栈、令牌或完整选中文本。

### 5.3 鉴权与安全

- 只绑定 `127.0.0.1`，禁止 `0.0.0.0`、`[::]` 或局域网地址。
- `X-Auth-Token` 与配置中的 32 字节 hex 令牌做常量时间比较。
- 缺失/错误返回 401；不返回期望值。
- Origin 存在时必须以 `moz-extension://` 开头，否则 403。
- 缺失 Origin 的请求仍需令牌，供调试工具使用。
- CORS 头必须加到所有响应，包括 401/403/413/429；预检不参与限流。
- 请求体上限 1MB，限流 20 次/秒、突发 40。
- 不信任 `X-Forwarded-For` 等代理头。
- 日志不记录令牌、完整 URL、完整选中文本。

### 5.4 运行时状态

`BridgeStatus` 同时用于 `get_bridge_status` 与 `bridge://status`：

```ts
interface BridgeStatus {
  enabled: boolean;            // 配置是否启用
  running: boolean;            // 监听是否真的起来了
  port: number;                // 运行中为实际端口，失败时为期望端口
  url: string;                 // http://127.0.0.1:<port>
  api_version: number;         // 固定 1
  last_success_ms: number | null;
  error: string | null;        // 端口占用等启动失败原因
}
```

- 令牌轮换通过 `regenerate_bridge_token` 写配置，下一次请求即用新令牌，无需重启监听。
- 端口或启用状态变化由 `update_settings` 触发重启；绑定失败只更新 `error`，不改写已保存的端口。
- 服务只随应用运行，退出时关闭监听。

## 6. AnkiConnect

固定 POST `{endpoint}`，`version: 6`。

```json
{
  "action": "addNote",
  "version": 6,
  "params": {
    "note": {
      "deckName": "<settings.anki.deck>",
      "modelName": "<settings.anki.model>",
      "fields": {
        "Word": "...",
        "Definition": "...",
        "Sentence": "...",
        "Source": "..."
      },
      "tags": ["global-dict"],
      "options": { "allowDuplicate": true }
    }
  }
}
```

- 连接超时 1.5s，总超时 3s，不自动重试。
- **始终发送 `allowDuplicate: true`**；同一单词在不同语境/义项下必须可建多张卡。
- 应用层用 `saved_cards` 检查“同词同句同释义”指纹；命中时按 `duplicate_policy` 提示或静默保存，绝不阻断不同语境。
- AI 释义生成的卡片追加标签 `ai-generated`，`Source` 写 `AI · <model>`；词典义项仍写来源名。
- 连接失败 → `ANKI_UNREACHABLE`，文案“Anki 未运行或 AnkiConnect 未安装”。
- HTTP 200 + `error` 非空：包含 `duplicate` → `ANKI_DUPLICATE`，否则 `ANKI_API_ERROR`。
- 导出前用 `modelFieldNames` 校验四个字段存在。
- `word`、`sentence`、`source` HTML 转义；换行转 `<br>`。
- 空 source 写“未知来源”；空 sentence 写空字符串。
- 本地图片/音频在导出时移除，插入 `[图片]`/`[音频]`。

## 7. 解析器契约

### 7.1 FormatParser

```rust
pub trait FormatParser: Send + Sync {
    fn can_parse(&self, path: &Path) -> bool;
    fn open(&mut self, path: &Path) -> Result<(), ParserError>;
    fn lookup(&self, query: &str) -> Result<Vec<Entry>, ParserError>;
}
```

- MDX 查询按原文、小写、大写、首字母大写依次尝试，最多 3 条。
- 资源路径拒绝 `..`，允许正/反斜杠回退。
- HTML 经 lol_html 改写后交 ammonia 清洗。

### 7.2 ResponseParser

```rust
pub trait ResponseParser: Send + Sync {
    fn parse(&self, ctx: ParseContext<'_>) -> Result<Vec<Entry>, ParserError>;
}
```

- JSONPath 表达式返回对象数组；缺失 `headword` 用查询词，缺失 `definition_html` 跳过。
- 资源 URL 只接受 http/https，相对 URL 丢弃。
- 解析错误只让该源失败，不影响其他源。

### 7.3 脚本 API

```js
function parse(ctx) {
  const data = JSON.parse(ctx.responseBody);
  return data.items.map((item) => ({
    headword: item.word,
    definition_html: item.html,
    phonetic: item.phonetic || null,
    resources: [],
  }));
}
```

- 输入：`{ responseBody: string, query: string }`。
- 输出：Entry 数组，`source_name` 忽略。
- 无 `fetch`、`XMLHttpRequest`、`require`、`process`、`setTimeout`。
- 超时 200ms，内存 16MB，栈 256KB。
- 编译/运行/超时/内存/结构错误分别映射，不中断查询。
- 脚本是本地代码，UI 必须提示只运行可信脚本。

### 7.4 义项提取

`SenseExtractor` 在 `Entry` 上工作，不新增字典格式依赖：

1. 用 `scraper`（ISC）解析 `definition_html`。
2. 优先取顶层 `ol > li` / `ul > li`；没有列表时取顶层 `p` 分组；仍无法拆分则整条 Entry 作为一个候选。
3. 每个候选生成 `e{entry_index}-s{n}`，保留该段 HTML 与去标签文本；空文本或长度 <2 的候选丢弃。
4. 最多 20 个候选；同一 Entry 内去重。
5. 词性从 `<span class="pos">`、`<i>` 前缀或括号缩写中尽力提取，取不到为 `null`。
6. 抽取失败不报错，退化为整条 Entry 单候选。
7. 该方法对结构化词典有效，对整段 HTML 词典只能整条推荐；这是已知限制，不假装能精确切分所有 MDX。

### 7.5 JEV-like 选择器契约

配置：`sense.selector_endpoint`，默认 `http://127.0.0.1:8787/v1/select`。

MVP 只支持 HTTP 适配，不直接启动任意本地进程。JEV 类 CLI 需要用户自行包装成本地 HTTP 服务，避免命令注入与权限扩大。

请求：

```json
{
  "word": "bank",
  "sentence": "He sat on the bank of the river.",
  "locale": "en",
  "senses": [
    { "id": "e0-s1", "part_of_speech": "noun", "definition": "financial institution" },
    { "id": "e0-s2", "part_of_speech": "noun", "definition": "sloping land beside water" }
  ]
}
```

响应：

```json
{
  "selected_id": "e0-s2",
  "confidence": 0.81,
  "distribution": { "e0-s1": 0.19, "e0-s2": 0.81 },
  "model": "jev-like-2026-10",
  "reason": "river context"
}
```

规则：

- `selected_id` 必须属于请求候选；否则按低置信度处理。
- `confidence` 必须在 0–1；缺失按 0。
- 只接受 HTTP 200 + JSON；超时 800ms；不重试。
- 选择器失败 → `Unavailable`，不自动调用 LLM；用户可手动点“用 AI 解释”。
- 默认只允许回环地址；`selector_allow_remote = true` 才允许远程，并在 UI 二次确认。
- 置信度是模型自评；JEV-like 模型可能过度自信，阈值可调，界面不把它显示成客观概率。

### 7.6 LLM 兜底契约

触发条件：

- `sense.enabled = true` 且 `llm.enabled = true`；
- 有原句；
- 选择器状态为 `LowConfidence` 或没有命中候选；
- 用户未关闭“AI 兜底”。

请求使用 OpenAI-compatible `POST {llm.endpoint}`：

```json
{
  "model": "<llm.model>",
  "temperature": 0,
  "max_tokens": 320,
  "messages": [
    { "role": "system", "content": "<lexicographer prompt>" },
    { "role": "user", "content": "{word, sentence, senses, locale}" }
  ]
}
```

要求模型只返回 JSON：

```json
{
  "definition": "the land alongside a river",
  "part_of_speech": "noun",
  "confidence": 0.7,
  "explanation": "the sentence mentions the river"
}
```

规则：

- 解析 `choices[0].message.content` 的 JSON；坏 JSON → `LLM_BAD_RESPONSE`，不生成半截文本。
- `definition` 是纯文本；禁止 HTML；长度 ≤600 字符。
- 不把 LLM 输出写回 `Entry`，只作为 `AiDefinition` 候选。
- 同一查询最多调用 1 次；超时 15s；不流式；不重试。
- 结果写入 `sense_cache`，TTL 7 天；key 含 model 与 prompt version。
- 远程端点（非 loopback）必须 `allow_remote = true`；否则 `LLM_REMOTE_BLOCKED`，不发请求。
- API key 只从 `api_key_env` 指定的环境变量读取；不写入配置、不写日志。
- 用户点击“使用此释义”后，卡片 `Source` 写 `AI · <model>`，并加 `ai-generated` 标签。

### 7.7 状态与阈值

| 条件 | 状态 | UI |
| --- | --- | --- |
| `sense.enabled=false` | `disabled` | 不显示推荐，只显示全部义项 |
| 没有原句 | `no_context` | 不调用选择器，提示“缺少原句” |
| 选择器不可达/超时 | `unavailable` | 辅助区淡提示，可手动用 AI |
| `confidence >= high_confidence` | `selected` | 高亮推荐义项，保存默认使用 |
| `low < confidence < high` | `ambiguous` | 高亮 + “建议确认”，不自动改选择 |
| `confidence <= low` 或无匹配 | `low_confidence` | 显示“可能不在词典义项中”，LLM 兜底 |
| LLM 成功 | `low_confidence` + `ai` | 显示 AI 卡，默认不选中 |
| LLM 失败/未启用 | `low_confidence` | 保留全部义项，提示手动判断 |

- 用户手动切换义项后，以用户选择为准；选择器结果不覆盖。
- 用户选择 AI 释义后，保存使用 AI 文本；仍可编辑。
- 阈值默认 `low=0.55`、`high=0.72`，只允许 `0 < low < high <= 1`。

## 8. 存储与路径

| 内容 | 路径 |
| --- | --- |
| 配置 | `%APPDATA%\LexiGrab\config.toml` |
| 缓存 | `%LOCALAPPDATA%\LexiGrab\cache.db` |
| 日志 | `%LOCALAPPDATA%\LexiGrab\logs\` |
| 脚本 | 配置目录下的 `parsers/`，或用户选择的绝对路径 |
| 资源 | 不落盘，按需从 MDD 读取 |

- 配置、缓存、日志目录可通过“关于”页打开。
- 不提交 `.env`、令牌、词典文件。
- 令牌明文存储于用户配置，依赖 Windows 用户目录权限；文档明确说明。

## 9. 默认值与限制

| 项 | 默认 | 范围/限制 |
| --- | --- | --- |
| 热键 | `CmdOrCtrl+Shift+D` | 必须含修饰键 |
| 查询策略 | `local_first` | 三选一 |
| 每源条数 | 3 | 1–10 |
| 总条数 | 20 | 5–50 |
| 远程超时 | 3000ms | 200–15000ms |
| 脚本超时 | 200ms | 50–1000ms |
| 脚本内存 | 16MB | 固定 |
| 远程响应体 | 2MB | 超出失败 |
| 单条 HTML | 512KB | 超出跳过 |
| 桥接端口 | 12787 | 1024–65535 |
| 桥接请求体 | 1MB | 超出 413 |
| 缓存 TTL | 24h | 1–720h |
| 负缓存 | 5min | 1–60min |
| 词典打开超时 | 15s | 固定 |
| 查询超时 | 500ms | 固定 |
| 候选义项 | 20 | 每个 Entry/查询 |
| 选择器超时 | 800ms | 200–3000ms |
| 低置信度阈值 | 0.55 | 0–1，且低于 high |
| 高置信度阈值 | 0.72 | 0–1 |
| LLM 超时 | 15000ms | 1000–60000ms |
| LLM 输出 | 320 tokens | ≤600 字符定义 |
| 语境缓存 TTL | 7 天 | 1–720h |

## 10. 边缘情况

| 情况 | 行为 |
| --- | --- |
| 词典损坏或截断 | 该文件打开失败，同源其他文件继续；状态显示错误 |
| worker 无响应 | 查询超时；连续 3 次标记 unhealthy |
| 资源路径穿越 | 400/404，不读取文件 |
| 远程 401/403 | 该源失败，事件记录，不弹窗 |
| 远程重定向跨主机 | 停止跟随，按错误处理 |
| 脚本死循环 | 400ms 内终止，按超时处理 |
| 缓存损坏 | 重建，不阻断查询 |
| 桥接端口占用 | 状态 failed，设置页提示改端口 |
| 令牌轮换 | 旧令牌立即 401，扩展需重新配对 |
| Anki 重复 | 不同语境/义项允许；仅同词同句同释义提示 |
| 字段缺失 | 导出前拦截，指向设置 |
| 选择器返回未知 id | 按低置信度处理，不选中 |
| 选择器不可达 | `unavailable`，不自动调 LLM |
| LLM 坏 JSON | 丢弃，保留词典义项 |
| LLM 远程未允许 | `LLM_REMOTE_BLOCKED`，不发请求 |
| 同词不同句 | 正常建卡，不提示重复 |
| 同词同句不同义项 | 正常建卡，指纹不同 |
| 同词同句同释义 | 提示一次，用户可仍然保存 |
| 配置损坏 | 备份并用默认配置启动 |
| 单实例重复启动 | 显示已有管理器，退出新进程 |

## 11. 安全模型

- 攻击面：本地桥接、远程词典响应、MDX HTML、用户脚本、AnkiConnect 响应。
- 桥接：回环 + 令牌 + Origin + 限流 + 请求体限制。
- HTML：白名单清洗，移除脚本、事件属性、`javascript:`、`data:` 链接；资源协议只返回白名单 MIME。
- 脚本：无宿主能力、时间/内存限制；不作为强安全边界。
- 网络：远程只允许 http/https；鉴权头不随跨主机重定向发送；不记录敏感值。
- 依赖：AGPL-3.0 兼容；`cargo deny` 与前端许可证检查。
- 隐私：无遥测；缓存与日志留在本机；日志不记录选中文本。
- 语境辅助：选择器默认回环；LLM 默认关闭且只允许回环，远程需显式允许；API key 只读环境变量；短语/原句不写日志。
- AI 输出：纯文本、标注模型、用户确认后才写卡；不得冒充词典原文。

## 12. 测试缝

- `DictionarySource` 用 fake source 测路由。
- HTTP 用 wiremock 测超时、状态码、重定向、响应体上限。
- 脚本用固定夹具测超时、OOM、错误结构。
- 缓存用可注入时钟测 TTL。
- 桥接用 `tower::ServiceExt::oneshot` 测鉴权、CORS、限流。
- MDX 用自建夹具测查询、资源、损坏文件。
- 命令层只做薄封装，逻辑放在 core，便于单元测试。
