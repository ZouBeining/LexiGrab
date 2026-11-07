# M2 手动验收：AnkiConnect 夹具报告

- 采集时间：2026-10-30 08:34（+08:00）
- 环境：Anki **26.9.3**（`D:\Anki\Anki.exe`，中文 UI，profile `Andy`），进程保持运行，未关闭/重启
- 端点：`http://127.0.0.1:8765`（仅监听 `127.0.0.1`）
- 协议：AnkiConnect API **version = 6**（`version` action 返回 6，不是 Anki 应用版本号）
- 状态：夹具已建好；**未实际调用 `addNote`**，`LexiGrab E2E` 牌组当前为空
- 本次仅新建 1 个 deck + 1 个 note type，未改动既有牌组/笔记类型

## 1. 基线与精确名称

`version` → `6`。

牌组（`deckNames`，创建后）：

```text
LexiGrab E2E
系统默认
```

笔记类型（`modelNames`，创建后）：

```text
LexiGrab Basic
图片遮盖
填空题
问答题
问答题（可选附翻转卡片）
问答题（输入答案）
问答题（附翻转卡片）
```

## 2. 新建的 E2E 夹具

| 项目 | 值 |
| --- | --- |
| 牌组 | `LexiGrab E2E`（id `1793320476460`） |
| 笔记类型 | `LexiGrab Basic`（id `1793320476621`，`type=0` 标准非填空） |
| 字段（顺序） | `Word`, `Definition`, `Sentence`, `Source` |
| 模板 | `Card 1` |

模板内容：

```json
{
  "Card 1": {
    "Front": "{{Word}}",
    "Back": "{{Definition}}<hr id=answer>{{Sentence}}<br><br>{{Source}}"
  }
}
```

CSS（minimal）：

```css
.card { font-family: arial; font-size: 20px; text-align: center; color: black; background-color: white; }
```

校验结果：`modelFieldNames("LexiGrab Basic")` 精确返回 `["Word","Definition","Sentence","Source"]`；`modelTemplates` 精确返回上表模板。

## 3. 可用性验证（未写卡）

对 `LexiGrab E2E` + `LexiGrab Basic` 的样例 note（字段 `compat-check` / `<b>compat</b> definition` / `A sample sentence.` / `manual-acceptance`，tags `["lexigrab-e2e"]`，`options.allowDuplicate=true`）：

```text
canAddNotes → {"result":[true],"error":null}
findNotes deck:"LexiGrab E2E"        → {"result":[],"error":null}
findNotes deck:"LexiGrab E2E" added:1 → {"result":[],"error":null}
```

结论：牌组空、模板可写、字段可映射，且未留下任何卡片。

## 4. 缺失字段错误用例的替代模型

`问答题`（字段本地化，不含 `Word/Definition/Sentence/Source`）：

```text
modelFieldNames("问答题") → ["正面", "背面"]
modelFieldNames("填空题") → ["文字", "背面额外"]   # 备用
```

负向探测（仅 `canAddNotes`，不写卡）：把 LexiGrab 四字段塞给 `问答题` → `{"result":[false],"error":null}`，牌组仍为空。说明 **AnkiConnect 对字段不匹配返回 `false` 而非 error**，缺失字段必须在应用侧用 `modelFieldNames` 预校验并映射为 `ANKI_FIELD_MISSING`。

## 5. 与 LexiGrab 默认配置的兼容注意

1. 应用默认 `deck="Default"`、`model="Basic"`，本机不存在这两个英文名 → E2E 前必须把设置改为 `LexiGrab E2E` / `LexiGrab Basic`，否则 `addNote` 会报 deck/model 不存在。
2. 中文 UI 下内置笔记类型字段名已本地化（如 `问答题` 的 `正面/背面`），因此导出前不能假设硬编码 `Word/Definition/Sentence/Source`，必须走 `modelFieldNames` 校验（本次新建的模型字段是英文，可直接对应默认 `field_map`）。
3. `canAddNotes` 的 `false` 不代表网络/API 错误；应用侧应保持「先 `modelFieldNames` 校验 → 再 `addNote`」的顺序。
4. 创建操作可重复执行：本报告脚本先查 `modelNames`，模型已存在则复用；`createDeck` 对已存在牌组同样可安全重复调用。
5. 未触碰 `系统默认` 牌组与既有笔记类型；未安装/更新任何 Anki 插件。

## 6. 清理步骤（后续按需执行）

当前无卡片，可直接删牌组：

```json
{"action":"deleteDecks","version":6,"params":{"decks":["LexiGrab E2E"],"cardsToo":true}}
```

如后续手动验收产生过卡片，先删笔记再删牌组：

```json
{"action":"findNotes","version":6,"params":{"query":"deck:\"LexiGrab E2E\""}}
{"action":"deleteNotes","version":6,"params":{"notes":["<note_id>"]}}
```

笔记类型 `LexiGrab Basic`：AnkiConnect 6 没有删除 model 的 action，需在 Anki 图形界面「工具 → 管理笔记类型」手动删除；不删也不影响后续验收（可保留复用）。清理后建议用 `deckNames` / `modelNames` 复核。
