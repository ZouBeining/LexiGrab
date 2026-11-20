# Firefox 扩展

本文件说明 LexiGrab Firefox 扩展的构建、安装、配对、端口与令牌、错误码和隐私边界。桌面端设置与用户操作见 [user-guide.md](user-guide.md)，桥接契约见 [BACKEND_STRUCTURE.md](BACKEND_STRUCTURE.md) §5。

扩展使用 WXT + MV2，只验收 Firefox 109 及以上版本。0.1.0 不发布到 AMO，不提供签名，也不支持 Chrome、Edge 或 Safari。

## 1. 构建

```powershell
pnpm install
pnpm -C extension build:firefox
pnpm -C extension lint:firefox
pnpm -C extension package:firefox
```

| 命令 | 结果 |
| --- | --- |
| `pnpm -C extension build:firefox` | 构建到 `extension/.output/firefox-mv2/` |
| `pnpm -C extension lint:firefox` | 用 `web-ext lint --self-hosted` 检查构建目录 |
| `pnpm -C extension package:firefox` | 生成 `extension/dist/lexigrab-firefox-0.1.0.xpi` |

`web-ext lint` 当前为 0 error；warning 包括 1 条 AMO 2025-11 的 `data_collection_permissions` 声明（自托管未签名分发不适用，补该键会与 `strict_min_version=109.0` 冲突），以及 React/DOMPurify 打包产物里的 `innerHTML`（释义已经过桌面端与扩展两次清洗）。

manifest 关键值：

| 项 | 值 |
| --- | --- |
| `manifest_version` | `2` |
| `permissions` | `storage`、`http://127.0.0.1/*` |
| `gecko.id` | `lexigrab@localhost` |
| `strict_min_version` | `109.0` |
| content script | `<all_urls>`，`document_idle` |
| options | `options.html` |

扩展不申请 `tabs`、`history` 或 `bookmarks` 权限。

## 2. 安装

### Firefox Developer Edition / Nightly

1. 打开 `about:config`，把 `xpinstall.signatures.required` 设为 `false`。
2. 打开 `about:addons`，点击齿轮菜单，选择“从文件安装附加组件”。
3. 选择 `extension/dist/lexigrab-firefox-0.1.0.xpi`。
4. 安装后扩展启用；重启 Firefox 后仍保留，`storage.local` 里的端口与令牌也保留。

### Firefox 正式版

正式版默认拒绝未签名扩展。使用临时加载：

1. 先构建：`pnpm -C extension build:firefox`。
2. 打开 `about:debugging#/runtime/this-firefox`。
3. 点击“临时载入附加组件”，选择 `extension/.output/firefox-mv2/manifest.json`。
4. 扩展在本次会话有效；重启 Firefox 后消失，需要重新载入。

不要直接修改构建产物的 manifest 来绕过签名。项目不提供签名证书，也不在 AMO 发布。

## 3. 配对

1. 启动桌面端，打开“设置 → 浏览器扩展”。
2. 确认“启用本地桥接”已打开，记下端口（默认 `12787`）。
3. 点击“复制令牌”。令牌是 64 位十六进制字符串，默认遮罩显示。
4. 安装扩展。首次安装会自动打开扩展设置页；如果没有打开，在 `about:addons` 中打开 LexiGrab 的“首选项”。
5. 填入端口与令牌，点击“测试连接”。
6. 测试成功后点击“保存”；回到网页划词即可查询。

配对信息保存在 Firefox 扩展的 `storage.local` 中。令牌只由 background 脚本读取，不注入页面上下文。

### 端口

- 默认 `12787`，必须与桌面端“设置 → 浏览器扩展”一致。
- 扩展权限只覆盖 `http://127.0.0.1/*`，因此端口填 `12787` 即可，不要写 `localhost` 域名。
- 桌面端修改端口后，扩展设置也要同步改，否则会显示连接失败。
- 端口被占用时桌面端保留已保存的端口并显示桥接错误；换一个端口保存后，扩展同步修改。

### 令牌

- 桌面端首次启动生成，64 位小写十六进制。
- 桌面端“重新生成令牌”需要二次确认；确认后旧令牌立即失效，扩展下一次请求返回 401，需要重新复制并配对。
- 令牌明文保存在桌面端配置中，依赖 Windows 用户目录权限；扩展侧保存在 `storage.local`。文档不声称加密。

## 4. 划词与保存

1. 在网页中选中文本，松开鼠标或使用 Shift + 方向键。
2. 选区附近出现浮动按钮。
3. 点击按钮，content script 把词、原句、URL、标题、域名交给 background。
4. background 调用桌面端 `/v1/selection`，气泡显示加载态，然后显示释义、原句与来源。
5. 可以切换条目；点击保存后调用 `/v1/anki`。
6. 保存成功显示“已保存”和 note id，并提供“再存一张”入口。

边界：

- iframe 内的选区不支持。
- 气泡中的原句只读；编辑原句和释义源码在桌面端完成。
- 扩展会显示语境建议与 AI 释义，但保存仍使用所选词典义项，不能采用 AI 释义。
- 扩展不直接访问 AnkiConnect、远程词典、选择器或 LLM。
- 同一单词可以连续保存多张；同词同句同释义只提示一次，不阻断。
- 本地图片与音频无法在浏览器中读取，桥接返回前会替换为占位文本并清空本地资源。

## 5. 错误码

桥接非 2xx 响应的正文为 `{ code, message }`。扩展只透出白名单内的服务端码，未列入白名单的码统一显示 `SERVER_ERROR`。

| 错误码 | 含义 | 用户动作 |
| --- | --- | --- |
| `NOT_CONFIGURED` | 扩展设置里没有令牌 | 打开扩展设置，填入端口与令牌 |
| `DESKTOP_OFFLINE` | 无法连接桌面端桥接 | 确认桌面端已启动、桥接已启用、端口一致 |
| `AUTH_FAILED` | 令牌缺失或错误（401） | 从桌面端重新复制令牌并保存 |
| `ORIGIN_REJECTED` | Origin 不是 Firefox 扩展来源（403） | 通过 Firefox 加载扩展后再试 |
| `TIMEOUT` | 扩展请求 5 秒未完成 | 稍后重试；大词典可能仍在加载 |
| `SERVER_ERROR` | 桌面端返回了未透出的错误 | 查看桌面端日志与桥接状态，稍后重试 |
| `INVALID_RESPONSE` | 响应不是预期 JSON 结构 | 确认桌面端版本与扩展版本匹配 |
| `INVALID_REQUEST` | 词为空、过长或定义为空（400） | 重新划词；确认选区不是空白 |
| `RATE_LIMITED` | 超过 20 次/秒、突发 40（429） | 降低连续请求频率，稍后重试 |
| `PAYLOAD_TOO_LARGE` | 请求体超过 1MB 或定义 HTML 超过 512KB（413） | 缩短释义或重新查询 |
| `ANKI_UNREACHABLE` | AnkiConnect 不可达（503） | 启动 Anki，确认 AnkiConnect 已安装 |
| `ANKI_API_ERROR` | AnkiConnect 返回错误（502） | 在桌面端检查牌组、模板与字段 |
| `ANKI_FIELD_MISSING` | 模板缺少字段（422） | 在桌面端 Anki 设置重新映射四个字段 |
| `ANKI_DUPLICATE` | AnkiConnect 报告重复（409） | 检查模板与去重设置；应用层本身不阻止同词多卡 |

气泡不会显示原始堆栈；服务端细节只写入桌面端日志。

## 6. 隐私与网络

- 扩展只把用户明确触发的选区发送到 `127.0.0.1` 上的桌面端桥接。原句经回环桥接传输，不直接发往互联网。
- 请求带 `X-Auth-Token`；桥接只绑定回环地址，并校验 Origin 必须是 `moz-extension://`。
- content script 不直接 `fetch`；网络请求与令牌只存在于 background。
- 扩展不读取标签页历史、书签或无关页面内容。
- 页面数据包括词、原句、URL、标题、域名。它们用于查询与来源展示，不写入 Anki 的四字段；卡片没有 URL 字段。
- 释义在桌面端经 ammonia 清洗，进入气泡前再由 DOMPurify 清洗。本地媒体在返回给浏览器前被剥离。
- 远程词典返回的远程图片 URL 仍可能由浏览器加载，这属于词典内容，不是扩展上传的数据。
- 扩展不包含遥测。删除扩展会删除其 `storage.local` 中的端口与令牌；桌面端令牌不受影响。
