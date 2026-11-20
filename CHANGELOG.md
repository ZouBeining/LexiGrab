# CHANGELOG

本项目遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 的结构；
版本号遵循 [SemVer](https://semver.org/lang/zh-CN/)。0.1.0 是首个公开版本。

## [0.1.0] - 2026-11-21

首个 Windows 版本。只承诺 Windows 10 22H2 / Windows 11；macOS/Linux 未验收。

### 新增

- 全局热键划词查询与不抢焦点的快速弹窗；编辑态可改原句与释义源码后写入 Anki。
- 本地 MDX/MDD 词典：多文件源、同名 `.mdd` 资源、后台加载、坏源隔离。
- 远程 HTTP 词典：URL 模板、固定鉴权头、超时与一次重试、JSONPath 与 QuickJS 脚本解析。
- 查询策略 `local_first` / `first_hit` / `aggregate`，远程失败不阻断本地结果。
- SQLite 缓存：查询缓存（正/负 TTL）、语境缓存（按模型与 prompt version）、同语境卡片指纹。
- 语境辅助：JEV-like 选择器推荐义项、阈值状态机、低置信度时可选 LLM 兜底；AI 释义标注模型且默认不采用。
- AnkiConnect 制卡：牌组、笔记类型、四字段映射、标签、`allowDuplicate: true`、同词多卡提示。
- 三套浅色主题（figma / apple / cursor），只换 CSS 变量。
- Firefox 扩展：页面划词、句子提取、Shadow DOM 气泡、配对与保存；本地桥接只绑 `127.0.0.1`。
- 设置六个分区：通用、Anki、语境辅助、浏览器扩展、缓存、关于；热键冲突回滚、令牌轮换、目录打开与复制。
- 管理器：源增删改、启停、排序、状态与查询测试；首次引导。
- 开发 CLI `lexigrab-dev`：lookup / sources / anki-* / add-note / bench。
- 文档：README、用户指南、配置参考、扩展、开发、故障排查、性能、安全与架构文档。
- 合规：AGPL-3.0 许可证、SPDX 头、第三方许可证清单、`cargo deny` 配置。
- CI：前端/扩展/Rust 检查、依赖策略；tag 发布工作流产出 NSIS 安装包、`.xpi` 与 SHA256SUMS。

### 变更

- 查询缓存只覆盖远程源；本地 MDX 不缓存。
- 远程源与语境辅助都是可选增强：失败只影响自身，不阻断词典首屏。
- 语境结果只在成功状态写缓存，key 包含模型名与 prompt version。
- `AnkiPayload` 增加可选 `tags`；AI 卡由前端写 `AI · <model>` 与 `ai-generated`。
- 依赖标识统一为 `AGPL-3.0-only`，与 SPDX 头和计划一致。

### 修复

- Windows 上 Wry 把 `http://mdx-res.localhost/...` 还原为 `mdx-res://localhost/...` 导致破图；协议处理器跳过 `localhost` authority。
- 含本地媒体（图片/音频）的释义第二次保存时同语境提示失效；预检与导出统一走剥离媒体后的纯文本。
- 托盘图标缺少 tooltip 与可访问名称。
- 扩展把桥接非 2xx 统一映射为 `SERVER_ERROR`；现按白名单透出 `ANKI_UNREACHABLE` 等服务端错误码。
- Firefox Shadow DOM 不应用 Tailwind `@property` 默认值导致气泡缺边框与阴影；浮层作用域补齐变量。
- 远程请求失败的错误文案可能带完整 URL；统一用 `reqwest::Error::without_url()` 去除。

### 安全

- CSP 生产环境只允许本地脚本，不放行 `ws:`；词典 HTML 经 ammonia 清洗。
- `mdx-res` 拒绝路径穿越与非白名单 MIME，响应带 nosniff。
- 桥接校验 Origin、令牌（常量时间比较）、限流与 1MB 请求体上限；令牌轮换即时生效。
- 日志不记录令牌、完整 URL 或选中文本；令牌明文存本机配置，文档已披露。
- 脚本沙箱限制 16MB 内存、256KB 栈、50–1000ms，不注入 `fetch`/`require`/定时器。
- 安全审查十项结论见 [docs/security.md](docs/security.md)。

### 已知限制

见 README 的[“已知限制”](README.md#已知限制)，包括仅 Windows、MDX 私有变体、扩展与安装包未签名、
无自动更新、AI 释义可能不准确、同词多卡等。
