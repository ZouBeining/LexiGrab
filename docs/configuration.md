# 配置参考

本文件说明 LexiGrab 配置文件的位置、schema v2 的字段、默认值、取值范围和保存规则。字段契约以 [BACKEND_STRUCTURE.md](BACKEND_STRUCTURE.md) §2.3 为准；默认值与校验逻辑核对自 `src-tauri/src/core/config.rs`（2026-11-21）。

大多数设置可以在界面中完成，手改配置不是必需操作。改配置前先退出应用，改完再启动。

## 1. 文件位置

默认布局：

| 内容 | 路径 |
| --- | --- |
| 配置 | `%APPDATA%\LexiGrab\config.toml` |
| 缓存 | `%LOCALAPPDATA%\LexiGrab\cache.db` |
| 日志 | `%LOCALAPPDATA%\LexiGrab\logs\` |

环境变量在进程启动时读取，修改后需要重启应用。

| 环境变量 | 作用 | 结果 |
| --- | --- | --- |
| 未设置 | 使用 Windows 用户目录 | 配置在 `%APPDATA%\LexiGrab`，缓存与日志在 `%LOCALAPPDATA%\LexiGrab` |
| `LEXIGRAB_HOME=<目录>` | 便携模式根目录 | 配置为 `<目录>\config\config.toml`，日志为 `<目录>\logs`，缓存为 `<目录>\cache.db` |
| `LEXIGRAB_CONFIG_DIR=<目录>` | 只覆盖配置目录 | 配置为 `<目录>\config.toml`；日志与缓存放在 `LEXIGRAB_HOME`（若设置）或该目录下 |
| 两者都设置 | 配置目录与数据根目录分离 | 配置在 `LEXIGRAB_CONFIG_DIR`，日志与缓存在 `LEXIGRAB_HOME` |

本地词典路径相对于配置文件所属目录解析；写绝对路径时原样使用。脚本路径同理。

## 2. schema v2

当前 `meta.schema_version = 2`。加载时：

- v1 配置的 `anki.allow_duplicate = false` 迁移为 `duplicate_policy = "warn"`，`true` 迁移为 `"allow"`。
- v1 缺少的 `[sense]`、`[llm]` 使用默认值，均为关闭。
- 迁移成功后给出一次“建议保存”的提示。
- 未知字段会被忽略，不会报错，也不保证在下次保存时保留。不要依赖配置里额外塞入的自定义键。

## 3. 保存与恢复规则

保存设置时（界面点击“保存”或调用 `update_settings`）：

1. 先校验完整配置；校验失败不写盘，界面保留输入。
2. 写 `config.toml.tmp`，执行 `sync_all`，再替换目标文件。
3. Windows 上先删除旧文件再重命名；替换失败时保留 tmp 并返回错误。
4. 热键变化时先尝试注册新热键；失败返回 `HOTKEY_CONFLICT`，恢复旧热键且不写配置。
5. 保存成功后重新加载词典源；端口或桥接开关变化会重启桥接。

加载配置时：

- 解析失败、字段类型错误或校验失败：把原文件重命名为 `config.toml.bak-<Unix 秒时间戳>`，以默认配置启动，并在界面提示。
- 原文件不存在：生成默认配置、设备 ID 和桥接令牌。
- 缓存数据库损坏：重命名为 `cache.db.corrupt-<时间戳>` 后重建，不影响配置。

## 4. 顶层结构

```toml
[meta]
[general]
[anki]
[bridge]
[cache]
[sense]
[llm]
[[sources]]
```

`[meta]` 与 `[general]`、`[anki]`、`[bridge]`、`[cache]`、`[sense]`、`[llm]` 各只有一个；`[[sources]]` 可以有多个。

### meta

| 字段 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `schema_version` | 整数 | `2` | 配置结构版本；加载时迁移，保存时写回 2 |
| `device_id` | 字符串 | `""`（首次启动或为空时生成 UUID） | 诊断用标识；为空时加载会生成 |

### general

| 字段 | 类型 | 默认 | 范围/说明 |
| --- | --- | --- | --- |
| `hotkey` | 字符串 | `CmdOrCtrl+Shift+D` | 必须包含修饰键，至少两段；Windows 上 `CmdOrCtrl` 映射为 Ctrl，即 `Ctrl+Shift+D` |
| `theme` | 枚举 | `figma` | `figma`、`apple`、`cursor`；历史值 `system` 可加载，界面按 `figma` 显示并在保存时写回 `figma` |
| `query_strategy` | 枚举 | `local_first` | `local_first`、`first_hit`、`aggregate` |
| `max_entries_per_source` | 整数 | `3` | 1–10，每个源最多返回条数 |
| `max_entries_total` | 整数 | `20` | 5–50，单次查询总条数上限 |
| `start_minimized` | 布尔 | `false` | 启动时不显示管理器窗口，只留托盘 |

副作用：热键变化会切换全局热键；查询策略与条数影响新查询，不重载词典；主题只换 CSS 变量。

### anki

| 字段 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `endpoint` | 字符串 | `http://localhost:8765` | 仅 http/https，必须有主机名 |
| `deck` | 字符串 | `Default` | 牌组名，不能为空 |
| `model` | 字符串 | `Basic` | 笔记类型名，不能为空 |
| `tags` | 字符串数组 | `["global-dict"]` | 每次导出追加；AI 卡片还会加 `ai-generated` |
| `duplicate_policy` | 枚举 | `warn` | `warn` 提示同语境重复；`allow` 静默保存。两者都不阻止不同语境建卡 |
| `field_map.word` | 字符串 | `Word` | 模板字段名，不能为空 |
| `field_map.definition` | 字符串 | `Definition` | 模板字段名，不能为空 |
| `field_map.sentence` | 字符串 | `Sentence` | 模板字段名，不能为空 |
| `field_map.source` | 字符串 | `Source` | 模板字段名，不能为空 |

副作用：保存前校验四个映射非空；界面在已加载模板字段时校验字段存在；导出前还会校验字段存在且映射不重复。四字段契约不包含 URL，页面来源不会写入卡片。

### bridge

| 字段 | 类型 | 默认 | 范围/说明 |
| --- | --- | --- | --- |
| `enabled` | 布尔 | `true` | 关闭后扩展立即无法连接，配置与令牌保留 |
| `port` | 整数 | `12787` | 1024–65535；只绑定 `127.0.0.1` |
| `token` | 字符串 | `""`（首次启动或为空时生成 64 位十六进制） | 64 位十六进制；为空时加载会生成；明文存储 |

### cache

| 字段 | 类型 | 默认 | 范围/说明 |
| --- | --- | --- | --- |
| `ttl_hours` | 整数 | `24` | 1–720；远程源正结果缓存时长 |
| `negative_ttl_minutes` | 整数 | `5` | 1–60；远程源空结果缓存时长 |

本地 MDX 查询不经过缓存。错误结果不缓存。

### sense

| 字段 | 类型 | 默认 | 范围/说明 |
| --- | --- | --- | --- |
| `enabled` | 布尔 | `false` | 关闭时只展示全部义项，不调用选择器 |
| `selector_endpoint` | 字符串 | `http://127.0.0.1:8787/v1/select` | 仅 http/https；非回环地址需要 `selector_allow_remote = true` |
| `selector_timeout_ms` | 整数 | `800` | 200–3000 |
| `selector_allow_remote` | 布尔 | `false` | 允许非回环选择器；原句可能包含页面上下文 |
| `low_confidence` | 浮点数 | `0.55` | 必须大于 0，且小于 `high_confidence` |
| `high_confidence` | 浮点数 | `0.72` | 必须大于 `low_confidence`，且不超过 1 |
| `max_senses` | 整数 | `20` | 1–20；每次最多提交给选择器的候选义项数 |

置信度是选择器模型自评，不是校准概率。阈值只改变状态判定，不改变模型输出。

### llm

| 字段 | 类型 | 默认 | 范围/说明 |
| --- | --- | --- | --- |
| `enabled` | 布尔 | `false` | 关闭时不生成 AI 释义 |
| `endpoint` | 字符串 | `http://127.0.0.1:11434/v1/chat/completions` | OpenAI-compatible；仅 http/https |
| `model` | 字符串 | 空 | 启用 LLM 时不能为空；示例中为空但关闭状态合法 |
| `api_key_env` | 字符串 | `LEXIGRAB_LLM_API_KEY` | 只读环境变量名；不写入 API key 本身 |
| `timeout_ms` | 整数 | `15000` | 1000–60000 |
| `max_tokens` | 整数 | `320` | 1–4096 |
| `allow_remote` | 布尔 | `false` | 允许非回环 LLM；否则远程端点返回 `LLM_REMOTE_BLOCKED`，不发请求 |
| `cache_ttl_hours` | 整数 | `168` | 1–720；选择器与 LLM 共用的语境缓存时长 |

API key 只从 `api_key_env` 指定的环境变量读取。修改环境变量后需要重启应用；配置和日志都不会保存 key。

### sources

每个 `[[sources]]` 是 `SourceConfig`：

| 字段 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `id` | 字符串 | 后端生成 | `local-<uuid8>` 或 `remote-<uuid8>`；不要手改已有 ID |
| `name` | 字符串 | 无 | 显示名，不能为空 |
| `type` | 字符串 | 无 | `local_file` 或 `remote_http` |
| `enabled` | 布尔 | 无 | 关闭时不创建 worker，也不参与查询 |
| `priority` | 整数 | 无 | 数值越小越先查询；界面排序会重写为 10、20、30… |
| `paths` | 字符串数组 | 无 | 仅 `local_file`；至少一个 `.mdx`，路径不得包含 `..` |
| `url_template` | 字符串 | 无 | 仅 `remote_http`；必须恰好包含一个 `{word}`，只允许 http/https |
| `auth_header` | 字符串或省略 | 无 | 固定鉴权头名称；省略表示不带鉴权头 |
| `auth_value` | 字符串或省略 | 无 | 固定鉴权头值；明文存储 |
| `timeout_ms` | 整数 | 无 | 远程请求超时，200–15000；界面默认 3000 |
| `response_parser` | 表 | 无 | `{ mode = "jsonpath", expression = "..." }` 或 `{ mode = "script", path = "...", timeout_ms = 200 }` |

本地路径要求：

- 相对路径按配置文件目录解析，绝对路径原样使用。
- 同一路径不能出现在多个源中。
- 同名 `.mdd` 与 `.mdx` 放在同目录即可自动加载资源。

远程解析要求：

- 请求使用 GET，查询词按 UTF-8 百分号编码后替换 `{word}`。
- 响应体上限 2MB，单条 HTML 上限 512KB，单次最多 20 条。
- 超时或连接失败最多重试 1 次，间隔 150ms；跨主机重定向停止跟随。
- JSONPath 返回对象数组；缺 `headword` 时使用查询词，缺 `definition_html` 的条目跳过。
- 脚本只接收 `{ responseBody, query }`，返回 Entry 数组；没有 `fetch`、`XMLHttpRequest`、`require`、`process` 或定时器。
- 脚本是本地代码，沙箱不是强隔离；只运行可信脚本，不要从不可信来源复制解析脚本。
- 脚本超时默认 200ms，可在 50–1000ms 之间配置；内存上限 16MB，栈 256KB。
- 解析失败或超时只让该源失败，不阻断其他源，也不变成阻断性错误。

远程词典本身没有“允许远程”开关；`selector_allow_remote` 与 `llm.allow_remote` 只分别控制语境选择器与 LLM。

## 5. 保存规则

### 热键

- 界面保存时先注册新热键，再写配置。
- 注册失败返回 `HOTKEY_CONFLICT`，旧热键继续生效，配置不写盘，界面恢复旧值。
- 启动时注册失败不会阻止应用启动；状态页显示错误，并发送 `runtime://hotkey-error`。
- 热键字符串必须包含 Ctrl、Alt、Shift 或 Super 等修饰键，且至少两段。

### 端口

- 保存端口后配置先落盘，再重启桥接监听。
- 端口被占用时配置保留，桥接状态显示 `BRIDGE_PORT_IN_USE` 一类错误，不会自动换端口，也不会回滚已保存的端口。
- 换一个可用端口再保存即可；扩展需要同步改端口。
- 关闭桥接不会清空端口和令牌，重新启用后沿用。

### 令牌

- 首次启动生成两个 UUID 拼接的 64 位小写十六进制字符串。
- 令牌明文保存在配置中，依赖 Windows 用户目录权限；不提供加密。
- 界面“重新生成令牌”需要二次确认；写入后立即生效，旧令牌下一次请求返回 401，无需重启监听。
- 扩展需要重新复制令牌配对。手改令牌必须是 64 位十六进制。

### 主题

- 三个值：`figma`（默认）、`apple`、`cursor`。
- 设置页切换时即时预览；保存后写入配置，并在本机记录偏好。
- `system` 是历史值，可以加载，但界面按 `figma` 显示，保存时写回 `figma`。
- 0.1.0 没有深色主题。

### 远程源解析

- 保存源时校验 URL 模板、超时、脚本路径与脚本超时。
- JSONPath 表达式与脚本路径在创建源时解析；脚本源码按 mtime 缓存，修改后下一次查询重新读取。
- 测试查询使用草稿配置，不写盘、不写缓存；失败时附带前 4KB 原始响应摘要。
- 远程源启用或修改后，只重载受影响的源；远程失败只影响该源。

### 缓存 TTL

- 修改 `ttl_hours` 或 `negative_ttl_minutes` 只影响之后写入的条目；已有条目保留原过期时间。
- 过期条目按未命中处理，后台清理。
- `clear_lookup_cache` 只清 `lookup_cache`，保留 `saved_cards` 与 `sense_cache`。
- `clear_sense_cache` 只清选择器与 LLM 结果，不删除已保存的卡片历史。
- 语境缓存 TTL 使用 `llm.cache_ttl_hours`，默认 168 小时；失败结果不缓存。
- 修改或删除源时，该 `source_id` 的查询缓存失效。

## 6. 校验范围速查

| 项 | 范围 |
| --- | --- |
| 每源条数 | 1–10 |
| 总条数 | 5–50 |
| 桥接端口 | 1024–65535 |
| 缓存正 TTL | 1–720 小时 |
| 缓存负 TTL | 1–60 分钟 |
| 选择器超时 | 200–3000ms |
| 候选义项 | 1–20 |
| 低/高阈值 | `0 < low < high <= 1` |
| LLM 超时 | 1000–60000ms |
| LLM 最大 token | 1–4096 |
| 语境缓存 TTL | 1–720 小时 |
| 远程源超时 | 200–15000ms |
| 脚本超时 | 50–1000ms |

## 7. 与契约的关系

- 配置文件字段、IPC 与缓存表结构以 [BACKEND_STRUCTURE.md](BACKEND_STRUCTURE.md) 为准。
- 本文件只描述用户可见的配置行为，不复制 IPC 命令表。
- 代码与本文不一致时，先确认是文档过期还是实现偏差；改契约先改 docs，再改代码与测试。
