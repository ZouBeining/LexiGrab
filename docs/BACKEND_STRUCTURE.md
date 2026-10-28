# BACKEND_STRUCTURE

本文件定义 LexiGrab 后端的数据结构、配置、存储、IPC 与本地 HTTP 契约。它是实现的唯一技术契约来源；各阶段的执行顺序见 [.agents/plans](../.agents/plans/)，产品范围见 [PRD.md](PRD.md)，交互路径见 [APP_FLOW.md](APP_FLOW.md)。

## 1. 运行时结构

```text
Tauri 主进程（Windows）
├─ AppState
│  ├─ Settings（RwLock）
│  ├─ SourceRegistry（本地/远程源）
│  ├─ QueryRouter
│  ├─ CacheManager（SQLite）
│  ├─ AnkiClient
│  ├─ BridgeHandle（127.0.0.1）
│  └─ QueryMeta 环形缓存
├─ 词典 worker 线程（每个 MDX 文件一个）
├─ Tokio 运行时（命令、桥接、远程 HTTP）
└─ WebView 窗口：manager / popup / settings
```

### 1.1 线程与阻塞边界

- libmdx 的 `MdxReader` 内含 `Rc`，不是 `Send`/`Sync`。每个 `.mdx` 文件由独立的标准线程持有，外部只保留命令通道。
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
- IPC JSON 字段使用 snake_case，与 PROMPT 的示例一致。

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
schema_version = 1

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
allow_duplicate = false
field_map = { word = "Word", definition = "Definition", sentence = "Sentence", source = "Source" }

[bridge]
enabled = true
port = 12787
token = "<32-byte hex>"

[cache]
ttl_hours = 24
negative_ttl_minutes = 5

[[sources]]
id = "local-00000001"
name = "Mini"
type = "local_file"
enabled = true
priority = 10
paths = ["tests/fixtures/mini.mdx"]
```

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
```

规则：

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
| `lookup` | `query`, `source_ids?`, `query_id?` | `Entry[]` | 同步结果；远程异步事件追加 |
| `list_sources` | 无 | `SourceConfig[]` | 仅配置 |
| `add_source` | `config` | `SourceConfig` | 保存并异步打开 |
| `update_source` | `id`, `config` | `SourceConfig` | 保存并重载 |
| `remove_source` | `id` | `void` | 关闭 worker 并删除配置 |
| `export_to_anki` | `payload` | `{ note_id: number }` | 使用全局 Anki 设置 |
| `get_anki_decks` | 无 | `string[]` | AnkiConnect |
| `get_anki_models` | 无 | `{ name, fields }[]` | AnkiConnect |
| `get_settings` | 无 | `Settings` | 含令牌，UI 遮罩 |
| `update_settings` | `settings` | `Settings` | 校验、应用热键、保存 |
| `get_runtime_status` | 无 | `RuntimeStatus` | 热键、版本、路径 |
| `get_source_statuses` | 无 | `SourceStatus[]` | 运行时状态 |
| `test_source_query` | `config`, `query` | `TestResult` | 草稿配置，不写盘 |
| `get_current_selection` | 无 | `SelectionContext \| null` | 弹窗挂载兜底 |
| `set_popup_interactive` | `interactive` | `void` | 切换编辑焦点 |
| `hide_popup` | 无 | `void` | 关闭弹窗 |
| `open_window` | `manager \| settings` | `void` | 白名单 |
| `probe_anki` | 无 | `AnkiStatus` | 只读探测 |
| `open_config_dir` | 无 | `void` | 打开配置目录 |
| `get_query_meta` | `query_id` | `QueryMeta \| null` | 缓存/远程标记 |
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
| `dict://index-progress` | `{ source_id, path, phase, entry_count?, message? }` |
| `popup://selection` | `SelectionContext` |
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
| POST | `/v1/selection` | 是 | 划词查询 |
| POST | `/v1/anki` | 是 | 制卡 |
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
  "elapsed_ms": 42
}
```

- `lookup_collect_all` 等待所有启用源，截止 3.5s，返回已收集结果与 `partial`。
- 结果按 `max_entries_per_source` 与 `max_entries_total` 截断。

**POST /v1/anki**

```json
{
  "word": "serendipity",
  "definition_html": "<div>...</div>",
  "sentence": "It was pure serendipity that we met.",
  "source": "CC-CEDICT"
}
```

返回 `{ "note_id": 12345 }`。定义 HTML 上限 512KB，服务端再次清洗并剥离本地媒体。

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
      "options": { "allowDuplicate": false }
    }
  }
}
```

- 连接超时 1.5s，总超时 3s，不自动重试。
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
| Anki 重复 | 提示，不自动改原句 |
| 字段缺失 | 导出前拦截，指向设置 |
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

## 12. 测试缝

- `DictionarySource` 用 fake source 测路由。
- HTTP 用 wiremock 测超时、状态码、重定向、响应体上限。
- 脚本用固定夹具测超时、OOM、错误结构。
- 缓存用可注入时钟测 TTL。
- 桥接用 `tower::ServiceExt::oneshot` 测鉴权、CORS、限流。
- MDX 用自建夹具测查询、资源、损坏文件。
- 命令层只做薄封装，逻辑放在 core，便于单元测试。
