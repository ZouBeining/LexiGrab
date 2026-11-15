/**
 * IPC DTO。字段与 docs/BACKEND_STRUCTURE.md 第 2、3、4 节保持一致；
 * 所有跨 IPC 字段使用 snake_case，对齐 Rust serde 输出。
 * Tauri 命令的“参数”在 JS 侧仍按 Tauri v2 默认使用 camelCase，见 ipc.ts。
 */

// ---------------------------------------------------------------------------
// M0 选区捕获（保留，M2 扩展字段）
// ---------------------------------------------------------------------------

export type CaptureMethod = "clipboard" | "uia" | "manual";

export interface ScreenAnchor {
  x: number;
  y: number;
  dpi_scale: number;
}

/** 选区上下文。M2 新增 sentence/url/title/domain，均为可空。 */
export interface SelectionContext {
  text: string;
  sentence: string | null;
  url: string | null;
  title: string | null;
  domain: string | null;
  anchor: ScreenAnchor | null;
  foreground_hwnd: number;
  app_name: string;
  method: CaptureMethod;
  captured_at_ms: number;
  elapsed_ms: number;
}

export interface CaptureError {
  code: string;
  message: string;
  detail: string | null;
}

export interface CaptureResult {
  selection: SelectionContext | null;
  error: CaptureError | null;
}

export interface CaptureSample {
  elapsed_ms: number;
  method: CaptureMethod | null;
  app_name: string;
  succeeded: boolean;
}

export interface CaptureStats {
  samples: CaptureSample[];
  p50_ms: number | null;
  p95_ms: number | null;
  success_rate: number | null;
}

// ---------------------------------------------------------------------------
// 词条
// ---------------------------------------------------------------------------

/** 资源引用。本地资源逻辑 URI：`mdx-res://{dict_id}/{percent-encoded path}`。 */
export type ResourceRef =
  | { kind: "local"; dict_id: string; path: string }
  | { kind: "remote"; url: string };

/** 词典条目。`definition_html` 已经过后端清洗。 */
export interface Entry {
  headword: string;
  definition_html: string;
  phonetic: string | null;
  resources: ResourceRef[];
  source_name: string;
}

// ---------------------------------------------------------------------------
// 数据源
// ---------------------------------------------------------------------------

/**
 * 数据源配置。M2 只有本地文件源；远程源（type: "remote_http"）在 M4 接入。
 * Rust 侧为 flattened SourceKind，故 `type` 与 `paths` 和 id/name 平级。
 */
export interface SourceConfig {
  id: string;
  name: string;
  type: "local_file";
  paths: string[];
  enabled: boolean;
  priority: number;
}

export type SourceStatus =
  | { status: "loading" }
  | { status: "ready"; entry_count: number }
  | { status: "failed"; message: string };

/** 管理器/设置页展示的数据源运行时状态。 */
export interface SourceStateEntry {
  id: string;
  name: string;
  enabled: boolean;
  priority: number;
  status: SourceStatus;
}

/** `test_source_query` 的返回值：草稿配置的查询结果。 */
export interface TestResult {
  entries: Entry[];
  source_name: string;
  elapsed_ms: number;
}

// ---------------------------------------------------------------------------
// 设置（镜像 src-tauri/src/core/config.rs）
// ---------------------------------------------------------------------------

/** 配置中的主题值。历史配置可能仍是 `system`，UI 落到默认主题。 */
export type ThemePreference = "figma" | "apple" | "cursor" | "system";

export type QueryStrategy = "local_first" | "first_hit" | "aggregate";

export type DuplicatePolicy = "warn" | "allow";

export interface AnkiFieldMap {
  word: string;
  definition: string;
  sentence: string;
  source: string;
}

export interface MetaSettings {
  schema_version: number;
  device_id: string;
}

export interface GeneralSettings {
  hotkey: string;
  theme: ThemePreference;
  query_strategy: QueryStrategy;
  max_entries_per_source: number;
  max_entries_total: number;
  start_minimized: boolean;
}

export interface AnkiSettings {
  endpoint: string;
  deck: string;
  model: string;
  tags: string[];
  duplicate_policy: DuplicatePolicy;
  field_map: AnkiFieldMap;
}

export interface BridgeSettings {
  enabled: boolean;
  port: number;
  token: string;
}

/** 本地桥接运行状态（`get_bridge_status` 与 `bridge://status`）。 */
export interface BridgeStatus {
  enabled: boolean;
  running: boolean;
  port: number;
  url: string;
  api_version: number;
  last_success_ms: number | null;
  error: string | null;
}

export interface CacheSettings {
  ttl_hours: number;
  negative_ttl_minutes: number;
}

export interface SenseSettings {
  enabled: boolean;
  selector_endpoint: string;
  selector_timeout_ms: number;
  selector_allow_remote: boolean;
  low_confidence: number;
  high_confidence: number;
  max_senses: number;
}

export interface LlmSettings {
  enabled: boolean;
  endpoint: string;
  model: string;
  api_key_env: string;
  timeout_ms: number;
  max_tokens: number;
  allow_remote: boolean;
  cache_ttl_hours: number;
}

/** 完整配置。`update_settings` 需要回传完整对象。 */
export interface Settings {
  meta: MetaSettings;
  general: GeneralSettings;
  anki: AnkiSettings;
  bridge: BridgeSettings;
  cache: CacheSettings;
  sense: SenseSettings;
  llm: LlmSettings;
  sources: SourceConfig[];
}

// ---------------------------------------------------------------------------
// Anki
// ---------------------------------------------------------------------------

/** 写入 Anki 前的数据；牌组/模板/字段映射来自 Settings.anki。 */
export interface AnkiPayload {
  word: string;
  definition_html: string;
  sentence: string | null;
  source: string;
}

/** `export_to_anki` 返回值；`exact_context_seen` 仅用于同语境提示，不阻断。 */
export interface ExportResult {
  note_id: number;
  exact_context_seen: boolean;
}

/** `check_card_context` 返回值：同词同句同释义指纹是否已存在。 */
export interface CardContext {
  seen: boolean;
  note_id: number | null;
  created_at: number | null;
}

export interface AnkiStatus {
  available: boolean;
  version: number | null;
  message: string | null;
}

export interface AnkiModel {
  name: string;
  fields: string[];
}

// ---------------------------------------------------------------------------
// 查询元数据与运行状态
// ---------------------------------------------------------------------------

export interface QueryMeta {
  query_id: string;
  cached_sources: { source_id: string; source_name: string }[];
  pending_remote: number;
}

export interface HotkeyStatus {
  accelerator: string;
  registered: boolean;
  message: string | null;
}

export interface RuntimeStatus {
  version: string;
  hotkey: HotkeyStatus;
  config_dir: string;
  log_dir: string;
  cache_path: string;
  sources_ready: number;
  popup_visible: boolean;
}

// ---------------------------------------------------------------------------
// 语境辅助（M3/M4 使用；事件 `lookup://sense-result` 的载荷）
// ---------------------------------------------------------------------------

export interface SenseCandidate {
  id: string;
  entry_index: number;
  source_name: string;
  headword: string;
  part_of_speech: string | null;
  definition_text: string;
  definition_html: string;
}

export type SenseStatus =
  | "disabled"
  | "no_context"
  | "selected"
  | "ambiguous"
  | "low_confidence"
  | "unavailable"
  | "failed";

export interface AiDefinition {
  definition_text: string;
  part_of_speech: string | null;
  model: string;
  confidence: number | null;
  based_on_sense_id: string | null;
  generated_at: number;
}

export interface SenseResolution {
  query_id: string;
  headword: string;
  status: SenseStatus;
  selected_sense_id: string | null;
  confidence: number | null;
  selector_model: string | null;
  ai: AiDefinition | null;
  error: string | null;
}

// ---------------------------------------------------------------------------
// IPC 错误与窗口
// ---------------------------------------------------------------------------

export interface IpcError {
  code: string;
  message: string;
  detail: string | null;
}

export type WindowLabel = "manager" | "settings" | "popup";

/** `open_window` 的白名单目标。 */
export type OpenWindowTarget = "manager" | "settings";
