# PROMPT.md

## 总体规划和开发环境

你是一名资深全栈桌面应用工程师。

你的任务是开发一个 Windows 优先的跨平台桌面工具，核心用途是：用户在任意应用或浏览器中选中一个生词后，快速查询其释义，并能连同原句、来源一起、一键保存到 Anki 复习。

工具需支持本地 MDX/MDD 词典的直接读取，也支持远程 HTTP 词典服务（含脚本 hook），两者可并存、可切换，且未来能无代码改动地扩展更多词典格式与数据源。

项目以 AGPL 协议开源。

## 设计目标

### 功能性需求（MVP 范围）

1. 全局划词查询：在任意桌面应用中选中文本后，通过全局快捷键（默认单次按下 Ctrl+Shift+D（Windows，macOS 为 Cmd+Shift+D、后续提供），可配置）触发查询，在光标附近弹出轻量窗口显示释义。

1. 本地词典直接读取：无需启动任何后台服务，工具内直接解析 .mdx/.mdd 文件，支持多词典文件管理（一个源可挂载多个 .mdx/.mdd 文件）、多词典同时查询、MDD 中的图片/音频资源展示。

1. 远程词典 HTTP 查询：可配置一个或多个远程词典服务端点（URL 模板 + 响应解析规则 + 可选认证头），查询结果与本地结果统一呈现。

1. 义项选择（JEV-like 语境辅助）：本地 MDX 只能列出全部义项，工具需结合原句判断当前语境下最可能的义项。接入 JEV 类本地义项选择服务：输入词、原句与候选义项，返回所选义项与 0–1 置信度（模型自评）。高置信度时高亮推荐义项；置信度偏低时提示“该单词此处含义可能不在词典义项中”，并交由 LLM 生成语境释义。选择与 AI 结果只作建议，必须由用户确认后才写入卡片，不得静默替换词典原文。

1. 一键制卡到 Anki：通过 AnkiConnect 将"词条 + 释义 + 例句"写入指定牌组；支持配置笔记类型与字段映射；提交前允许用户编辑例句和释义。默认字段映射为：词（Word）、释义（Definition）、原句（Sentence）、来源（Source）。source 默认取命中 Entry.source_name ；aggregate 多源命中时，取全部命中源名称并以逗号分隔。使用 AI 生成释义时 source 记为 `AI · <模型名>`，并打 `ai-generated` 标签。

1. 同词多卡：同一单词在不同语境、不同义项下必须允许建立多张卡片。导出请求始终 `allowDuplicate: true`；应用层不以“首字段相同”判定重复，只对“同词 + 规范原句 + 规范释义”完全相同的语境做一次可覆盖的提示。

1. 句子捕获（分层策略）：早期版本接受“自动捕获 + 手动补充”的降级方案，不追求跨应用的高可靠捕获。桌面端尽力而为（Windows 优先），失败时弹窗保留“原句输入框”不阻塞制卡。浏览器场景要求完备支持：通过配套浏览器扩展，利用 content script 精确获取选中词所在的 DOM 上下文句子，实现高可靠捕获。

1. 词典源管理界面：增删改查词典源（本地文件/远程 HTTP）、设置优先级与启用状态。查询策略（local_first / first_hit / aggregate）为全局选项，在设置界面中选择。

1. 设置界面：AnkiConnect 地址、牌组、笔记模板、字段映射、全局快捷键、查询策略（local_first / first_hit / aggregate）、界面主题、语境辅助（选择器、阈值、LLM 与隐私开关）等。

1. 浏览器扩展（里程碑 M3）：开发配套浏览器扩展，在网页中划词后调用桌面工具的本地 API 完成查词与制卡，并自动捕获所在句子。桌面工具需为此暴露一个仅监听 127.0.0.1 的本地 HTTP 端点。

### 非功能性需求

1. 现代简洁的 UI：划词弹窗极简（释义预览 + 制卡按钮），管理/设置窗口功能完整。

1. 模块化与可扩展：格式解析与数据源获取为两个正交维度，新增格式或数据源不应触碰查询调度与 UI 层。

1. 性能：本地 MDX 查询响应目标 < 100ms（冷启动首次索引除外）；划词弹窗出现延迟目标 < 200ms。

1. 健壮性：远程源超时/失败时静默降级，不阻塞本地结果展示。义项选择器或 LLM 超时/失败同样只影响辅助区，词典结果照常显示。

1. 隐私：语境选择器与 LLM 默认只允许回环地址，LLM 默认关闭；只有启用语境辅助且存在原句时才把词与原句发送给对应服务；API key 只从环境变量读取，不写入配置与日志。

1. 开源友好：代码结构清晰，便于后续开源。

## 技术栈

层 | 选型 | 备注
-- | -- | --
桌面框架 | Tauri 2 | 系统 WebView，体积小；已有多个划词工具先例
后端语言 | Rust | 与 Tauri 天然集成，性能好
MDX 解析 | raymanzhang/mdx (LibMdx) | 功能完整；许可证 AGPL-3.0
前端 | React + TypeScript + Tailwind CSS | 现代简洁风格
Anki 集成 | AnkiConnect（Anki 插件，代码 2055492159） | 通过 `http://localhost:8765` 的 JSON-RPC 调用
全局划词 | Tauri 全局快捷键 + 平台相关选区捕获 | Windows 优先（UI Automation / 模拟复制）；X11、KDE、macOS 作为后续阶段，其他桌面环境不支持。
配置存储 | Tauri Store 插件或本地 JSON/TOML 文件 | ——
HTTP 客户端 | reqwest（Rust） | 支持超时、重试、自定义 header
脚本引擎 | rquickjs（QuickJS 的 Rust 绑定） | 用于执行远程响应解析脚本；轻量、可设执行超时、易沙箱化
浏览器扩展 | WXT（基于 Vite） | 原生支持多浏览器构建；wxt -b firefox 即可生成 Firefox 目标产物，并可自动打开 Firefox 加载开发版扩展
Firefox manifest 版本 | MV2 或 MV3（由 WXT 自动选择） | WXT 默认对 Firefox 使用 MV2，对其他浏览器使用 MV3。MV2 在 Firefox 中对 .xpi 签名和 ID 的要求相对宽松，建议 MVP 阶段跟随 WXT 的默认策略，避免过早陷入 MV3 的强制 ID 和签名细节
.xpi 构建 | WXT 构建 → web-ext build 打包 | web-ext build 是 Mozilla 官方推荐的打包工具，会自动排除 .git 等无关文件，输出 .zip，重命名为 .xpi 即可
义项提取 | scraper（Rust HTML 解析） | 从已清洗的义项 HTML 中切出候选义项列表，供语境选择使用
语境义项选择 | JEV 类本地服务（HTTP 适配） | 输入词 + 原句 + 候选义项，输出义项与置信度；只作建议，不覆盖词典；MVP 只接 HTTP，不直接启动第三方 CLI
语境释义兜底 | OpenAI-compatible LLM 接口 | 低置信度或词典外含义时调用；默认关闭、默认回环；AI 结果标注模型并需用户确认

## 架构与模块划分

```text
┌──────────────────────────────────────────────┐
│  UI 层（Tauri 前端）                          │
│  划词弹窗 / 词典管理 / 设置 / 制卡编辑        │
└──────────────────┬───────────────────────────┘
                   │ Tauri Commands (IPC)
┌──────────────────▼───────────────────────────┐
│  核心服务层（Rust）                            │
│  QueryRouter / AnkiExporter / SelectionHook   │
│  ConfigManager / CacheManager                 │
└──────────────────┬───────────────────────────┘
                   │ 调用统一接口
┌──────────────────▼───────────────────────────┐
│  DictionarySource trait（数据源抽象）  
│  ┌────────────────┐   ┌──────────────────┐   │
│  │ LocalFileSource│   │ RemoteHttpSource │   │
│  └───────┬────────┘   └────────┬─────────┘   │
│          │ 内部按格式分发        │ 按响应规则解析 │
│  ┌───────▼────────┐   ┌────────▼─────────┐   │
│  │ FormatParser   │   │ ResponseParser   │   │
│  │ MDX / (未来…)  │   │ JSON / (未来…)   │   │
│  └────────────────┘   └──────────────────┘   │
└──────────────────────────────────────────────┘
```

ResponseParser 有两种实现：JsonPathParser（声明式字段映射）与 ScriptParser（执行用户提供的 JS 脚本，输入为原始响应体与词条，输出为 Entry[]）。脚本在 rquickjs 沙箱中运行，默认禁用网络、文件系统与定时器，并设置 200ms 执行超时。

### 关键抽象

```rust
// 统一词条结构，UI 与导出层只依赖它
pub struct Entry {
    pub headword: String,
    pub definition_html: String,
    pub phonetic: Option<String>,
    pub resources: Vec<ResourceRef>, // 图片/音频引用；渲染时解析为可访问 URI（Local → mdx-res://，Remote → https://）
    pub source_name: String,
}

pub enum ResourceRef {
    Local { dict_id: String, path: String }, // 由 mdx-res:// 协议解析
    Remote { url: String },
}

// 数据源抽象
pub trait DictionarySource: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn lookup(&self, query: &str) -> Result<Vec<Entry>, SourceError>;
    fn is_remote(&self) -> bool { false }
}

// 格式解析器抽象（LocalFileSource 内部使用）
pub trait FormatParser: Send + Sync {
    fn can_parse(&self, path: &Path) -> bool;
    fn open(&mut self, path: &Path) -> Result<(), ParserError>;
    fn lookup(&self, query: &str) -> Result<Vec<Entry>, ParserError>;
}

// AnkiPayload 结构
pub struct AnkiPayload {
    pub word: String,
    pub definition_html: String,
    pub sentence: Option<String>,
    pub source: String,
    // deck / model / field_map 不在 payload 中，一律由 [anki] 全局设置提供
}

// 语境辅助：候选义项与推荐结果（只作建议，不参与 Entry 的结构）
pub struct SenseCandidate {
    pub id: String,                 // 在同一 Entry 内唯一
    pub part_of_speech: Option<String>,
    pub definition_text: String,
    pub definition_html: String,
}

pub struct SenseResolution {
    pub status: SenseStatus,        // disabled | no_context | selected | ambiguous | low_confidence | unavailable | failed
    pub selected_sense_id: Option<String>,
    pub confidence: Option<f32>,    // 0..1，选择器自评，不是校准概率
    pub ai: Option<AiDefinition>,   // 仅在低置信度且启用 LLM 时出现
}

pub struct AiDefinition {
    pub definition_text: String,    // 纯文本，不含 HTML
    pub model: String,
    pub confidence: Option<f32>,
}
```

### 查询调度

QueryRouter 按配置的优先级依次调用各 DictionarySource，支持 local_first（默认）、first_hit、aggregate 三种策略。

- first_hit：按优先级依次查询，首个命中即返回。

- aggregate：查询所有启用源，聚合全部结果。

- local_first：先返回本地源结果，远程源异步查询，结果通过 Tauri event 追加。

### 语境辅助流程

```text
查询结果返回（全部义项照常展示）
  → 有原句且启用语境辅助：调用 JEV 类选择器（默认 800ms 超时）
      ├─ 高置信度：高亮推荐义项，保存默认使用该义项
      ├─ 中置信度：高亮并提示“建议确认”，用户可切换
      └─ 低置信度/无匹配：提示可能不在词典义项中
            → LLM 已启用：生成语境释义，标注模型，默认不选中
            → 未启用/失败：保留全部义项，由用户判断
  → 无原句、选择器失败、LLM 失败：只展示全部义项，不阻塞保存
```

默认阈值：低置信度 ≤ 0.55，高置信度 ≥ 0.72，可在设置中调整。用户手动选择义项后，选择器结果不再覆盖。

### 同词多卡流程

```text
保存 → 计算 词 + 规范化原句 + 规范化释义 的指纹
  ├─ 未见：直接导出（allowDuplicate: true）
  └─ 已见：提示“此语境已存过”，用户可“仍然保存”或“换义项”
保存成功后保留弹窗，提供“再存一张”
```

不同原句或不同义项不提示重复；应用层不做唯一性约束。

## API 约定和代码规范

### 1. 内部 Tauri Commands（前端 ↔ 后端）

```rust
lookup(query: string, source_ids?: string[]) -> Entry[]
// 注：lookup 仅返回同步可得的结果（本地源与缓存）；local_first / aggregate 策略下，
// 远程源结果通过 Tauri 事件 `lookup://remote-result`（携带 query_id、source_id、entries）异步追加
list_sources() -> SourceConfig[]
add_source(config: SourceConfig) -> SourceConfig
update_source(id, config) -> SourceConfig
remove_source(id) -> void
export_to_anki(payload: AnkiPayload) -> { note_id: number, exact_context_seen: boolean }
// allowDuplicate 恒为 true；exact_context_seen 只表示同词同句同释义已存在，供 UI 提示
check_card_context(payload: AnkiPayload) -> { seen: boolean, note_id?: number }
get_sense_resolution(query_id) -> SenseResolution | null
// 语境推荐结果；通过事件 `lookup://sense-result` 异步推送，超时/失败只影响辅助区
test_sense_selector(word, sentence) / test_llm(word, sentence, senses) -> 结果或错误，不写缓存
get_anki_decks() -> string[]
get_anki_models() -> { name: string, fields: string[] }[]
get_settings() / update_settings(settings)
```

### 2. AnkiConnect 调用契约

```json
POST http://localhost:8765
{
  "action": "addNote",
  "version": 6,
  "params": {
    "note": {
      "deckName": "<配置的牌组>",
      "modelName": "<配置的笔记类型>",
      "fields": {
      "Word": "<词条>",
      "Definition": "<释义HTML>",
      "Sentence": "<原句>",
      "Source": "<命中的词典源名称>"
      },
      "tags": ["global-dict"],
      "options": { "allowDuplicate": true }
    }
  }
}
```

错误处理：连接失败时提示"Anki 未运行或 AnkiConnect 未安装"，并给出安装引导。

重复策略：始终允许重复，以便同一单词在不同语境、不同义项下建多张卡。同语境提示由应用层 `saved_cards` 指纹完成，不依赖 AnkiConnect 的重复检查。AI 释义生成的卡片在 tags 中追加 `ai-generated`，字段 Source 记为 `AI · <模型名>`。

### 3. 远程词典 HTTP 契约（工具侧的预期格式）

建议工具内部支持一种通用响应映射，同时为未来自建 NAS 服务预留一个"标准契约"：

```text
GET {base_url}?q={word}
Headers: 可选 Authorization / X-Api-Key
→ 200 OK
{
  "entries": [
    {
      "headword": "string",
      "definition_html": "string",
      "phonetic": "string?",
      "resources": [{ "type": "image|audio", "url": "https://..." }]
    }
  ]
}
```

响应解析支持两种模式：jsonpath（MVP 必做）与 script（MVP 即支持，脚本钩子接收 { responseBody, query }，返回符合 Entry[] 结构的 JSON）。

Firefox 扩展侧配置要求：

1. manifest.json 的 permissions 或 host_permissions 必须包含 `http://127.0.0.1:<port>/*`，否则扩展的 fetch 会被 Firefox 阻止。端口若为动态，由用户在扩展设置中填写桌面端显示的端口和配对令牌；也可使用固定默认端口（如 12787）并允许修改。

1. Native Messaging 作为备选，但 Firefox 在 Windows 上需在注册表注册 native manifest，且 allowed_extensions 必须包含扩展 ID，配置负担远重于 localhost 桥接，MVP 不采用。

### 3.1 语境选择器 HTTP 契约（JEV 类服务适配）

```text
POST {selector_endpoint}      # 默认 http://127.0.0.1:8787/v1/select
{
  "word": "bank",
  "sentence": "He sat on the bank of the river.",
  "senses": [
    { "id": "e0-s1", "part_of_speech": "noun", "definition": "financial institution" },
    { "id": "e0-s2", "part_of_speech": "noun", "definition": "sloping land beside water" }
  ]
}
→ 200 OK
{
  "selected_id": "e0-s2",
  "confidence": 0.81,
  "model": "jev-like-<version>",
  "distribution": { "e0-s1": 0.19, "e0-s2": 0.81 }
}
```

规则：`selected_id` 必须属于请求中的候选，否则按低置信度处理；`confidence` 缺失按 0；只接受 HTTP 200 + JSON；失败返回 `unavailable`，不自动调用 LLM；默认仅允许回环地址，远程需用户显式允许。

### 3.2 语境释义 LLM 契约（OpenAI-compatible）

```text
POST {llm_endpoint}           # 默认 http://127.0.0.1:11434/v1/chat/completions
{ "model": "<配置的模型>", "temperature": 0, "max_tokens": 320, "messages": [...] }
→ choices[0].message.content 为 JSON：
{ "definition": "the land alongside a river", "part_of_speech": "noun", "confidence": 0.7, "explanation": "..." }
```

规则：仅在低置信度或词典外、且有原句、且用户启用时触发；输出为纯文本，拒绝 HTML；坏 JSON 丢弃；单次调用、不流式、不重试；API key 只从环境变量读取；远程端点需显式允许。LLM 输出只作为候选，写入卡片前必须由用户确认并显示“AI 生成 · 模型 · 可能不准确”。

### 4. 配置文件结构（TOML 示例）

```toml
[general]
hotkey = "CmdOrCtrl+Shift+D"
theme = "system"
query_strategy = "local_first"  # local_first | first_hit | aggregate

[anki]
endpoint = "http://localhost:8765"
deck = "Default"
model = "Basic"
field_map = { word = "Word", definition = "Definition", sentence = "Sentence", source = "Source" }
duplicate_policy = "warn"  # warn | allow；Anki 请求恒为 allowDuplicate: true

[sense]
enabled = false
selector_endpoint = "http://127.0.0.1:8787/v1/select"
selector_timeout_ms = 800
low_confidence = 0.55
high_confidence = 0.72

[llm]
enabled = false
endpoint = "http://127.0.0.1:11434/v1/chat/completions"
model = ""
api_key_env = "LEXIGRAB_LLM_API_KEY"
timeout_ms = 15000
allow_remote = false

[[sources]]
id = "local-cedict"
name = "CC-CEDICT"
type = "local_file"
enabled = true
priority = 10
paths = ["/path/to/cedict.mdx"]

[[sources]]
id = "nas-dict"
name = "NAS 词典"
type = "remote_http"
enabled = false
priority = 20
url_template = "https://nas.local/dict/api/search?q={word}"
# 二选一
response_parser = { mode = "jsonpath", expression = "$.entries[*]" }
# 或
response_parser = { mode = "script", path = "./parsers/nas.js", timeout_ms = 200 }
auth_header = "Authorization"
auth_value = "Bearer <token>"
timeout_ms = 3000
```

### 5. Manifest 规范

浏览器扩展的 manifest.json （或 WXT 配置）必须包含以下内容。

```json
{
  "browser_specific_settings": {
    "gecko": {
      "id": "global-dict@yourdomain.example",
      "strict_min_version": "109.0"
    }
  }
}
```

- gecko.id：格式可以是类似邮箱的字符串（`extensionname@example.org`），也可以是带花括号的 UUID 格式。这是一个永久标识符，一旦发布就应保持不变。

- strict_min_version：设为 109.0，以覆盖现代 Firefox 对 content script 和 window.getSelection() 的良好支持。

- 如果 WXT 使用 MV3 构建 Firefox 版本，还需要注意 MV3 扩展在提交到 AMO 时强制要求扩展 ID，且可能需要声明 data_collection_permissions。MVP 阶段直接采用 WXT 对 Firefox 的默认 MV2 构建，以降低签名和上架复杂度。

## 开发规范

1. 代码组织：Rust 侧按 core/（调度、配置、缓存）、sources/（数据源实现）、parsers/（格式解析）、anki/、platform/（划词捕获）分层；前端按 windows/popup、windows/manager、windows/settings 分离。

1. 错误处理：Rust 侧统一 thiserror 定义错误类型，IPC 边界转换为带 code/message 的结构体；前端对用户展示友好提示，不暴露原始堆栈。

1. 异步与超时：所有远程请求必须设超时（默认 3s，可配置）；查询使用 tokio 并发，本地与远程结果通过 Tauri event 分别推送。

1. 资源协议：注册自定义协议 mdx-res://{dict_id}/{path} 用于本地 MDD 资源加载，UI 层统一通过该 URI 引用，远程资源用 https://。

1. 测试：FormatParser 与 DictionarySource 必须有单元测试（可用小型测试 MDX 文件）；QueryRouter 的优先级与降级逻辑需覆盖测试。

1. 提交规范：Conventional Commits；每个 PR 聚焦单一模块。

1. 文档：README 含架构图、配置说明、AnkiConnect 安装指引；关键 trait 需 rustdoc 注释。

1. 许可证：项目整体采用 AGPL-3.0（与 libmdx 保持一致），根目录放置 LICENSE，README 注明。

1. 脚本沙箱：ScriptParser 必须限制执行时间、内存与可用全局对象；脚本加载失败或超时须降级为“该源无结果”，不得中断查询流程。

1. 语境辅助：选择器与 LLM 调用统一走 `spawn_blocking` 的 HTTP 客户端，必须设超时；结果通过事件推送，不得阻塞词典结果渲染；不得在日志中记录原句、令牌或 API key；AI 输出不得写回 Entry 或作为词典源缓存。

## 注意事项与风险

1. 许可证一致性：项目采用 AGPL-3.0 开源，与 libmdx 兼容；引入任何新依赖前需检查其许可证是否与 AGPL-3.0 兼容。

1. 句子捕获（Windows 优先）：MVP 仅在 Windows 上实现尽力而为的捕获（UI Automation 或模拟复制读剪贴板），X11/KDE/macOS 的实现路径记录在文档中供后续阶段参考。浏览器场景的可靠捕获由扩展的 content script 承担，不依赖桌面端。

1. 句子捕获的务实边界：浏览器扩展的 content script 负责捕获“词 + 原句 + 来源（URL / 页面标题 / 域名）”。 核心 API 是 `window.getSelection()`。推荐策略：

    1. 用户选中文本后，通过 `selection.getRangeAt(0)` 获取 Range。

    1. 从 `Range.startContainer` 向上遍历到最近的块级祖先元素（`<p>`、`<div>`、`<li>` 等），取该元素的 `textContent` 作为候选句子上下文。

    1. 如果块级上下文过长（超过一个句子），用简单的标点切分（`.!?` 及中文 `。！？`）定位选中词所在的那一句。

    1. 将 `{ word, sentence, url, title, domain }` 通过 localhost HTTP 桥接发送给桌面端。

    1. 可以直接参考两个已存在的 Firefox/Chrome 扩展项目：readtrace（专门做“上下文优先”的划词保存，会保存原句、段落上下文、URL、标题、域名和定位元数据），以及 dictionariez 的 content script 系统（包含从不同上下文中提取词和句子的工具函数）。

1. 务实的降级策略：自动捕获失败时，弹窗保留"例句输入框"，不阻塞制卡。

1. 划词弹窗焦点：弹窗出现时不应抢走原应用焦点，否则用户无法继续阅读；需要平台相关的"非激活窗口"实现（Tauri 的 always_on_top + focus: false 需按平台验证）。

1. MDX 大文件：部分 MDX 词库达数百 MB，首次打开需建索引。应在后台线程完成，UI 显示加载进度，避免阻塞。

1. 远程源不可用：默认配置中远程源应可"禁用"，且查询时失败静默跳过；不要让 NAS 未部署时影响本地体验。

1. AnkiConnect 未安装：启动时探测 AnkiConnect 可用性，未安装时在设置页给出明确引导，而不是在制卡时才报错。

1. 缓存：远程查询结果按 (source_id, query) 缓存（带 TTL，默认 24h），断网时可查看历史查询。义项选择与 AI 释义单独缓存，TTL 默认 7 天，可单独清理。

1. 义项局限与兜底：本地 MDX 只给出全部义项，无法判断语境。JEV 类选择器负责在义项间选择并给出置信度；置信度偏低说明含义可能在词典义项之外，此时才启用 LLM。两者都是建议，用户保留最终选择权。

1. 同词多卡：同一单词在不同语境下本就该有多张卡。禁止以单词作为唯一性判断；Anki 请求恒为 allowDuplicate: true；同语境提示只作提醒，不能阻断保存。

1. 本地 API 服务安全：桌面端生成随机 X-Auth-Token，用户在扩展设置中填入；本地 HTTP 端点校验 X-Auth-Token；同时校验 Origin 以 moz-extension:// 开头（可选加固），不声称“校验扩展 ID”。

## 交付里程碑（建议 agent 按此顺序推进）

阶段 | 交付物 | 验证标准
-- | -- | --
M0（技术风险验证） | Tauri 2 最小窗口 + WS_EX_NOACTIVATE + 选区捕获模块（SelectionHook） | 选中文本后弹窗出现，点击弹窗按钮时原应用光标不丢失；能拿到选中文本和光标坐标
M1（核心链路无 UI） | Rust 程序加载 MDX、查询、输出 HTML、切出候选义项；AnkiConnect 写入四字段卡片（允许重复） | 命令行可完成“查词 → 制卡”
M2（桌面端 MVP） | Tauri 划词弹窗 + 本地 MDX 管理 + 制卡（含手动原句输入）+ 同词多卡与同语境提示 | 在 Windows 记事本/浏览器中划词 → 弹窗 → 同一单词在不同句子下连续制卡成功
M3a（Firefox 扩展核心） | WXT 项目 + content script 句子提取 + localhost HTTP 桥接 | 在 Firefox 中选中英文单词，扩展能捕获词、原句、页面标题/URL，发送到桌面端并收到查询结果
M3b（.xpi 打包） | web-ext build 生成的 .xpi（未签名） | 在 Firefox about:addons 中安装 .xpi 成功；重启 Firefox 后扩展仍然存在（验证 gecko.id 生效）
M3c（可选：签名分发） | 构建的 .xpi 能在 Firefox Developer Edition / Nightly 中通过 about:addons 正常安装并持久存在；或在正式版 Firefox 中通过 about:debugging 临时加载并完成全部功能验证。文档中需说明目标用户的浏览器版本要求。 | ——
M4（远程 + 脚本钩子 + 语境辅助） | RemoteHttpSource + JSONPath + 脚本钩子 + 降级 + JEV 类义项选择 + 低置信度 LLM 兜底 + 缓存 | 配置一个非标准 API，通过脚本正确解析出 Entry[]；选择器高/低置信度状态正确，低置信度能生成标注模型且需手动采用的 AI 释义
M5（打磨 + 文档） | 设置界面、Anki 字段映射、语境辅助设置、README、许可证文件 | 可发布开源仓库

## 已确认决策

1. 平台优先级：Windows → X11/KDE → macOS；其他桌面环境不支持。

1. 项目开源，许可证 AGPL-3.0。

1. 前端框架：React + TypeScript。

1. 远程响应解析：MVP 即支持 JSONPath 与脚本钩子两种模式。

1. 浏览器扩展分发：不发布到 AMO。.xpi 用于本地/自行分发，依赖 Firefox Developer Edition/Nightly 或手动签名豁免设置。

1. 句子捕获：接受降级方案；桌面端 Windows 优先、尽力而为；浏览器端通过配套扩展实现完备支持。

1. Anki 默认字段：Word / Definition / Sentence / Source。

1. 默认快捷键为单次按下组合键（Ctrl/Cmd+Shift+D），非双击。

1. aggregate 多源命中时，Source 字段取全部命中源名称并以逗号分隔。

1. 查询策略共三种：local_first（默认）/ first_hit / aggregate。

1. 牌组、笔记类型与字段映射仅在设置中配置，AnkiPayload 不携带 deck/model。

1. 语境义项选择：接入 JEV 类本地服务，仅作推荐并给出模型自评置信度；置信度偏低时提示词典外含义，再由 LLM 生成语境释义。

1. AI 边界：LLM 默认关闭、默认回环、只在低置信度兜底；AI 释义必须标注模型、默认不选中、用户确认后才能写卡，不得冒充词典原文。

1. 同词多卡：允许同一单词建立多张卡片；Anki 请求恒为 allowDuplicate: true；应用层仅对“同词 + 原句 + 释义”完全相同的语境提示一次。

1. 选择器接入方式：MVP 只支持 HTTP 适配，由用户自行把 JEV 类 CLI 包装成本地服务；不直接启动第三方进程。
