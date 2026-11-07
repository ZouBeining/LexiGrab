# M2 手动验收：真实词典媒体（图片/音频）探查报告

- 采集时间：2026-10-30 08:35–08:55（+08:00）
- 范围：只读探查 `references/dict` 两份真实 MDX 的释义 HTML 媒体，为 `mdx-res` 渲染验收挑词；未启动 GUI、未改源码/配置、未提交。
- CLI：`src-tauri\target\debug\lexigrab-dev.exe`（已存在，构建时间 2026-10-30 08:30，晚于全部源码，未重新构建）。
- 临时文件：只落在 `src-tauri\target\e2e\`（`target` 已被 `.gitignore` 忽略），本仓库新增文件仅本报告。

## 1. 结论（给 Lead）

| 用途 | 推荐词 | 词典 | 说明 |
| --- | --- | --- | --- |
| **图片验收 headword** | **`apple`** | Oxford ALD 9th En-En | `fruit.jpg`，720×540 JPEG，32,945 B，含标签的「Fruit」插图，非 1×1，肉眼可见 |
| **冒烟查询词** | **`computer`** | 两本都有 | 释义短；Oxford 内联音标 `/kəmˈpjuːtə(r)/`、`/kəmˈpjuːtər/`（注意 `Entry.phonetic` 为 `null`，见 §6） |

请求 URI（按本报告配置的 source id 生成）：

```text
mdx-res://local-oxford-f1/fruit.jpg
Windows 形式：http://mdx-res.localhost/local-oxford-f1/fruit.jpg
```

## 2. 词典与数据源

| 文件 | 绝对路径 | 大小 | CLI 打开结果 |
| --- | --- | --- | --- |
| Collins MDX | `D:\00 CBackUp\Users\ZouBeining\Projects\MyProjects\LexiGrab\references\dict\Collins ALD_En-Cn.mdx` | 13,887,505 B | ready entries=34414 |
| Collins MDD | `D:\00 CBackUp\Users\ZouBeining\Projects\MyProjects\LexiGrab\references\dict\Collins ALD_En-Cn.mdd` | 15,307 B | 3 个资源：2×png（`arrow_down.png`/`arrow_up.png`）+ 1×css |
| Oxford MDX | `D:\00 CBackUp\Users\ZouBeining\Projects\MyProjects\LexiGrab\references\dict\Oxford ALD_9th_En-En.mdx` | 41,514,928 B | ready entries=59976 |
| Oxford MDD | `D:\00 CBackUp\Users\ZouBeining\Projects\MyProjects\LexiGrab\references\dict\Oxford ALD_9th_En-En.mdd` | 4,104,095 B | 244 个资源：235×jpg + 8×png + 1×css |

- 图片只在 Oxford 词典里；Collins 的 MDD 只有两个箭头小图和 CSS，`lookup` 结果中没有任何条目带图片。
- 本报告配置 source id 为 `local-collins` / `local-oxford`，每源首个文件的逻辑 `dict_id` 分别为 `local-collins-f1` / `local-oxford-f1`；`mdx-res` URI 中的 id 即后者。

## 3. 主用例：`apple`（Oxford ALD 9th En-En）

CLI 结果（截取关键字段，`--json`）：

```json
{
  "headword": "apple",
  "definition_html": "…<a class=\"topic\" title=\"\" href=\"http://www.oxfordlearnersdictionaries.com/media/english/fullsize/f/fru/fruit/fruit.jpg\" rel=\"noopener noreferrer\"><img src=\"mdx-res://local-oxford-f1/fruit.jpg\"></a>…",
  "phonetic": null,
  "resources": [
    { "kind": "local", "dict_id": "local-oxford-f1", "path": "fruit.jpg" }
  ],
  "source_name": "Oxford ALD 9th En-En"
}
```

原始 MDX 片段（`<img>` 来自 MDD，相对路径）：

```html
<a class="topic" title="" href="http://www.oxfordlearnersdictionaries.com/media/english/fullsize/f/fru/fruit/fruit.jpg"><img src="fruit.jpg"/></a>
```

清洗/改写后进入弹窗的 HTML：

```html
<a class="topic" title="" href="http://www.oxfordlearnersdictionaries.com/media/english/fullsize/f/fru/fruit/fruit.jpg" rel="noopener noreferrer"><img src="mdx-res://local-oxford-f1/fruit.jpg"></a>
```

资源核对（用与 App 相同的 `mdictlib 0.2.8` 直接读取 MDD，见 §8）：

| 项目 | 值 |
| --- | --- |
| 逻辑路径 | `fruit.jpg` |
| MDD key | `\fruit.jpg`（MDD 内 key 带前导反斜杠；`fruit.jpg` 与 `\fruit.jpg` 两种查询都能命中） |
| 字节数 | 32,945 B |
| 格式/尺寸 | JPEG `720×540`，magic `ff d8 ff e0` |
| 是否可见/有意义 | **是**。已导出并目视确认：带英文标签的「Fruit」水果图（cherry / apple / peach / kiwi / avocado / fig / grapes / banana / watermelon，apple 上标注 pip、core、skin、stalk 等），不是占位图或 1×1 |
| 图片外链 | `<a>` 保留指向 Oxford 官网 fullsize 图的 https 链接；按 T2.2 点击应交给系统浏览器 |

注意：`<img>` 没有 `width`/`height`/`alt`，原始尺寸 720×540 大于弹窗 420×560 的内容宽度；因此这次验收同时能检验前端对超宽图片的缩放/溢出处理。

## 4. 备选图片词（均已核对资源存在且尺寸可见）

| headword | source_name | 图片逻辑路径 | 字节 | 尺寸 | 备注 |
| --- | --- | --- | --- | --- | --- |
| `computer` | Oxford ALD 9th En-En | `computing_equipment.jpg` | 26,953 | 720×540 | 电脑外设标签图；**同一词也是推荐冒烟词** |
| `house` | Oxford ALD 9th En-En | `house_labels.jpg` | 38,396 | 720×540 | 房屋部位标签图 |
| `car` | Oxford ALD 9th En-En | `car_dashboard.jpg` + `trains.jpg` | 40,543 / 48,935 | 720×540 / 720×540 | 同一条目含 2 张图，可测多图 |
| `tree` | Oxford ALD 9th En-En | `tree_bark.jpg` | 23,717 | 720×540 | 树皮/年轮图 |
| `bird` | Oxford ALD 9th En-En | `birds.jpg` | 11,972 | 480×360 | |
| `bicycle` | Oxford ALD 9th En-En | `cycles.jpg` | 50,246 | 720×540 | |
| `chair` | Oxford ALD 9th En-En | `chairs_livingroom.jpg` | 12,781 | 480×360 | |
| `table` | Oxford ALD 9th En-En | `hobbies.jpg` | 38,657 | 720×540 | 图片主题与词义略偏（爱好图），仅作备选 |
| `door` | Oxford ALD 9th En-En | `kitchen_objects.jpg` | 52,231 | 720×540 | |
| `hand` | Oxford ALD 9th En-En | `hand_body.jpg` + `clock.jpg` + `cards.jpg` | 10,055 / 15,355 / 17,070 | 480×360 ×3 | 同条目 3 张图 |
| `eye` | Oxford ALD 9th En-En | `eye_body.jpg` + `fasteners_clothes.jpg` | 17,335 / 19,718 | 720×540 ×2 | |

以上 11 个词在 `--json` 结果中均带 `<img src="mdx-res://local-oxford-f1/…">`，且资源路径逐条在 MDD 中 `found=true`。

**已知坏引用（不要用来做图片验收）**：`window` 条目引用 `computing_desktop.jpg`，但该 key 在 Oxford MDD 中 `found=false`（应 404），同条目另一张 `house_labels.jpg` 正常。这也顺便验证 `mdx-res` 的资源缺失路径。

## 5. 音频情况：两份真实词典都没有可用的本地音频

- MDD 资源统计：Oxford `{jpg:235, png:8, css:1}`，Collins `{png:2, css:1}` —— **0 个 mp3/wav/ogg/m4a**。
- 原始 MDX 里只有**远程**发音地址，例如 `apple`：

```html
<div class="sound audio_play_button pron-uk icon-audio"
     data-src-mp3="http://www.oxfordlearnersdictionaries.com/media/english/uk_pron/a/app/apple/apple__gb_1.mp3"
     data-src-ogg="http://www.oxfordlearnersdictionaries.com/media/english/uk_pron_ogg/a/app/apple/apple__gb_1.ogg"
     title=" pronunciation English" style="cursor: pointer"> </div>
```

- 该发音按钮以 `data-src-*` 属性承载 URL，`html.rs` 清洗后属性被移除，`definition_html` 中**不存在** `<audio>`/`sound://`/`.mp3` 引用（`--json` 中 resources 只有图片）。
- 结论：M2 这轮桌面验收**无法用这两份词典验证音频播放**；音频需等 M3 扩展或远程源。图片验收不受影响。

## 6. 冒烟词：`computer` + 音标现状

CLI 输出（两源都命中）：

| source_name | definition 文本（去标签，截断） | 图片 |
| --- | --- | --- |
| Collins ALD En-Cn | `computer ★★★★☆ 1 [N-COUNT 可数名词] 计算机;电脑； A computer is an electronic machine that can store and deal with large amounts of information. …` | 无 |
| Oxford ALD 9th En-En | `011256 ♠ computer noun noun BrE / kəmˈpjuːtə(r) / ; NAmE / kəmˈpjuːtər / an electronic machine that can store, organize and find information, do calculations and control other machines …` | `computing_equipment.jpg` |

关于「word + phonetic + short definition」：

- **`Entry.phonetic` 当前恒为 `null`**：CLI/解析器（`src-tauri/src/parsers/mdx.rs`）不提取音标字段，本次 9 个样本词的 JSON 中 `phonetic` 全为 `null`。
- 音标实际以**内联文本**出现在 Oxford 的 `definition_html` 里（`BrE /kəmˈpjuːtə(r)/`、`NAmE /kəmˈpjuːtər/`），弹窗可正常看到；若验收脚本断言 phonetic 字段，会立即失败——这是 M1 已知缺口，不是本次数据问题。
- 若只想最短的干净释义，`apple`（Collins 文本 ~590 字符）或 `computer`（Collins ~390 字符）都合适；建议冒烟用 `computer`，图片用 `apple`，互不干扰。

## 7. 可复制的最小配置（绝对路径，已实测）

文件：`src-tauri\target\e2e\config.toml`（gitignored）。App 默认读 `%APPDATA%\LexiGrab\config.toml`；也可用环境变量 `LEXIGRAB_CONFIG_DIR` 指向本目录（该目录内需有 `config.toml`）。

```toml
[meta]
schema_version = 2
device_id = "e2e-device"

[general]
hotkey = "CmdOrCtrl+Shift+D"
theme = "figma"
query_strategy = "local_first"
max_entries_per_source = 3
max_entries_total = 20
start_minimized = false

[anki]
endpoint = "http://127.0.0.1:8765"
deck = "Default"
model = "Basic"
tags = ["global-dict", "lexigrab-test"]
duplicate_policy = "warn"
field_map = { word = "Word", definition = "Definition", sentence = "Sentence", source = "Source" }

[bridge]
enabled = false
port = 12787
token = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

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
id = "local-collins"
name = "Collins ALD En-Cn"
type = "local_file"
enabled = true
priority = 10
paths = ["D:/00 CBackUp/Users/ZouBeining/Projects/MyProjects/LexiGrab/references/dict/Collins ALD_En-Cn.mdx"]

[[sources]]
id = "local-oxford"
name = "Oxford ALD 9th En-En"
type = "local_file"
enabled = true
priority = 20
paths = ["D:/00 CBackUp/Users/ZouBeining/Projects/MyProjects/LexiGrab/references/dict/Oxford ALD_9th_En-En.mdx"]
```

> source id 必须是 `local-oxford` 才能得到上面的 `local-oxford-f1` URI；换成别的 id 时，弹窗里 `<img>` 的 `mdx-res://` 前缀会随之变化，但 `fruit.jpg` 不变。

## 8. 使用的 CLI / 探查命令

```powershell
# 0) 二进制已存在，未重建
Get-Item src-tauri\target\debug\lexigrab-dev.exe

# 1) 确认两本词典可打开（约 1.2s）
src-tauri\target\debug\lexigrab-dev.exe sources --config src-tauri\target\e2e\config.toml
# local-collins  Collins ALD En-Cn  enabled=true priority=10 ready entries=34414
# local-oxford   Oxford ALD 9th En-En enabled=true priority=20 ready entries=59976

# 2) 批量查词并检查 definition_html / resources
src-tauri\target\debug\lexigrab-dev.exe lookup --config src-tauri\target\e2e\config.toml `
  --word apple --json
# 结果已保存为 src-tauri\target\e2e\<word>.json（gitignored）

# 3) 校验资源真实存在（throwaway，仅 under target/e2e）
#    同 App 使用 mdictlib 0.2.8 的 MddFile::lookup；并解析 JPEG/PNG/GIF 头拿尺寸
cd src-tauri\target\e2e\mddprobe
cargo build --offline
target\debug\mddprobe.exe keys   "D:/00 CBackUp/Users/ZouBeining/Projects/MyProjects/LexiGrab/references/dict/Oxford ALD_9th_En-En.mdd"
target\debug\mddprobe.exe get    "D:/00 CBackUp/Users/ZouBeining/Projects/MyProjects/LexiGrab/references/dict/Oxford ALD_9th_En-En.mdd" fruit.jpg
# key=fruit.jpg found=true bytes=32945 dims=jpeg 720x540
# 同时写出 src-tauri\target\e2e\mddprobe\probe_out\fruit.jpg 供肉眼确认
```

资源字节已导出到 `src-tauri\target\e2e\mddprobe\probe_out\`（`fruit.jpg`、`computing_equipment.jpg` 等，gitignored）；`fruit.jpg` 已目视确认是标签完整的水果插图。

## 9. 建议的验收步骤（Lead）

1. 用 §7 配置启动 App（或把 `config.toml` 复制到 `%APPDATA%\LexiGrab\config.toml`）。
2. 弹窗查 `apple`：Oxford 条目应显示 720×540 的「Fruit」插图；DevTools Network 应看到 `mdx-res://local-oxford-f1/fruit.jpg`（Windows 实际请求 `http://mdx-res.localhost/...`）返回 200 + `image/jpeg` + `nosniff`，且无 CSP 报错。
3. 检查图片是否被弹窗 CSS 限制宽度（原图 720 > 弹窗 420）。
4. 再查 `computer` 做冒烟：两本词典各一条结果，定义短、无报错；注意 `phonetic` 字段为 `null` 是已知解析缺口。
5. 可选资源缺失路径：查 `window`，其中 `computing_desktop.jpg` 应 404 且不影响整条 HTML 渲染。

## 10. 边界与风险

- 图片是 Oxford 的**标签插图**（720×540），不是单物体小图；在 420 宽弹窗里应能看到缩放后的整图，若被裁切则是前端 `max-width` 问题。
- `apple` 的 `<img>` 外包了一层 `<a href="http://…fullsize/…jpg">`；按 T2.2 点击图片应交系统浏览器打开，弹窗内不直接加载远程图。
- 没有本地音频资源（§5），音频播放分支本次无法验收。
- MDD 内 key 带前导 `\`，但 `MddFile::lookup("fruit.jpg")` 与 `lookup("\fruit.jpg")` 均命中；无需在协议层补反斜杠。
- 本次只读：未修改任何源码/配置，未提交；仓库内唯一新增文件为本报告；临时 config/probe 全部在 `src-tauri\target\e2e\`（已被 gitignore）。
