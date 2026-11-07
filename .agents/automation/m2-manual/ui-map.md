# LexiGrab M2 UI 地图（pywinauto / WebView2 UIA）

> 只读梳理，未启动应用、未运行 GUI 自动化。行号对应当前工作树。UIA 角色来自 ARIA 与 WebView2 行为的**推断**，需用 `print_control_identifiers()` 实测校正。前端**没有** `data-testid`（`grep -r data-testid src` 无匹配）。

## 1. 窗口清单

| 窗口 | label / 入口 | 标题 | 逻辑尺寸 | 装饰 | 启动时 |
| --- | --- | --- | --- | --- | --- |
| manager | `manager` / `manager.html?view=sources` | `LexiGrab` | 980×700，最小 900×600 | 有（默认） | `start_minimized=false` 时显示并聚焦 |
| settings | `settings` / `settings.html?section=general` | `LexiGrab 设置` | 900×700，最小 860×640 | 有（默认） | 预创建但隐藏 |
| popup（快速） | `popup` / `popup.html` | `LexiGrab` | 420×560 | **无** | 始终预创建隐藏，热键触发 |
| popup（编辑） | 同上 | 同上 | 560×640 | 无 | 点「编辑」后切换 |

- label/URL：`src-tauri/src/core/window.rs:8-14`；标题与尺寸：`:47-68`；启动显示逻辑：`src-tauri/src/lib.rs:96-106`。
- popup 特性：`resizable(false)`、`decorations(false)`、`always_on_top(true)`、`skip_taskbar(true)`、`focused(false)`、`shadow(true)`：`window.rs:59-68`。
- 快速模式 420×560 + 不抢焦点、编辑模式 560×640 + 聚焦：`window.rs:118-135`；编辑入口 `src/windows/popup/PopupApp.tsx:423-428` → `src-tauri/src/commands.rs:81-90`。
- 所有窗口 `visible(false)` 创建、关闭只隐藏：`window.rs:52/58/67`、`:105-115`。窗口 HTML `<title>` 与 Tauri 标题一致：`manager.html:6`、`settings.html:6`、`popup.html:6`。

### 关键可访问属性（供 UIA Name/AutomationId 使用）

| 窗口 | 属性 | 值 |
| --- | --- | --- |
| manager | `nav` aria-label | `主导航`（`ManagerApp.tsx:153`） |
| manager | `ul` aria-label | `词典源列表`（`SourceList.tsx:479`） |
| manager | 上移/下移按钮 aria-label | `上移 {name}` / `下移 {name}`（`SourceList.tsx:100,109`） |
| manager | 启停开关 | `role="switch"`，`aria-checked`，aria-label `停用 {name}` / `启用 {name}`（`SourceList.tsx:116-121`） |
| manager | 对话框 | `role="dialog"` `aria-modal="true"`，标题 id `source-dialog-title` / `delete-source-title`（`Modal.tsx:82-87`） |
| manager | 关闭对话框/关闭首次引导 | `关闭对话框`（`SourceDialog.tsx:198`）、`关闭首次引导`（`FirstRunGuide.tsx:169`） |
| settings | `nav` aria-label | `设置分区`（`SettingsApp.tsx:123`） |
| settings | 主题/查询策略/同语境重复 radiogroup aria-label | `主题` / `查询策略` / `同语境重复提示`（`GeneralSection.tsx:169,202`、`AnkiSection.tsx:388`） |
| settings | 热键录制按钮 aria-label | `录制全局热键`；录制中 `正在录制全局热键，按 Esc 取消`（`HotkeyInput.tsx:114`） |
| popup | 关闭 aria-label | `关闭弹窗`（`PopupApp.tsx:642`） |
| popup | 更多菜单 | aria-label `更多操作`，`role="menu"`，item `打开词典管理`/`设置`（`PopupApp.tsx:93-127`） |
| popup | 义项列表 | `ul role="radiogroup"` aria-label `词典义项`；每项 `role="radio"` aria-label `{headword} · {source_name}`（`ResultView.tsx:45-54`） |
| popup | 原句区 | `section` aria-label `原句预览`（`PopupApp.tsx:174`） |
| popup | 编辑 tab | `role="tablist"` aria-label `释义显示方式`；tab `预览`/`源码`（`EditView.tsx:115-135`） |

DOM `id` 也可能映射为 UIA AutomationId：`source-name`、`source-priority`、`source-dialog-title`、`delete-source-title`、`tester-source`、`tester-query`、`settings-hotkey`、`settings-per-source`、`settings-total`、`anki-endpoint`、`anki-deck`、`anki-model`、`anki-field-{word|definition|sentence|source}`、`anki-tags`、`definition-panel`（各表单源码）。

## 2. 管理器窗口

**Shell**：侧栏文字 `LexiGrab` / `词典管理器`，导航 `词典源`、`查询测试`、`运行状态`（`ManagerApp.tsx:9-13,150-151`），底部 `v0.1.0 · Windows`（有运行时状态则 `v{version} · Windows`，`:164`）。内容区最大 760px，侧栏 220px（`:148,169`）。

**词典源页**：`h1` = `词典源`，说明 `管理本地 .mdx 词典；列表按优先级升序，启停与排序会立即保存。`，右上按钮 `添加源`（`SourceList.tsx:398-405`）。
- 加载：Spinner `正在加载词典源…`（`:439`）；列表失败：`无法读取词典源` + `重试`（`:443-452`）；操作失败：`操作失败` + message + code + `关闭`（`:408-425`）；状态失败：`无法读取词典状态：{message}` + `重试`（`:467-478`）。
- 空态：标题 `还没有词典源`，说明 `添加本地 .mdx 词典后即可查询。词典只读取文件，不会修改或删除原文件。`，按钮 `添加词典`（`:455-460`）。
- 列表表头 `源列表` / `{n} 项`（`:464-465`）。

**行结构**（`li`，min-height 64px，横向 flex，`SourceList.tsx:72-151`）：
`名称` + `P{priority}`（`:75-80`）→ 路径摘要或 `未选择文件`（`:82-84`）→ 状态徽标（`SourceStatus.tsx`）→ `上移`/`下移`图标按钮（`↕`，无可见文字，靠 aria-label）→ 开关（`role="switch"`）→ `编辑` 文本按钮 → `删除` 文本按钮。
状态徽标文案：`已停用`（`SourceStatus.tsx:35`）、`正在加载`（`:43`）、`{entry_count} 条目`（`:52`）、`加载失败：{message}` + `重试`（`:60-70`）。

**添加/编辑对话框**（`SourceDialog.tsx`）：标题 `添加本地词典` / `编辑词典源`；副文案 `支持多选 .mdx；同名 .mdd 会自动配对。` / `修改后保存会重新加载该词典。`（`:186-193`）。字段：`词典文件*` + 按钮 `选择 .mdx 文件` + 提示 `选择多个文件时按顺序加载；同名 .mdd 图片与音频会自动生效。`（`:217-231`）；`名称*` placeholder `例如：牛津高阶`（`:267-283`）；`优先级` 数字输入（`:293-308`）；复选 `启用此源`（`:317`）；提示 `数值越小越优先；列表排序会重写为 10、20、30…。`（`:320`）。按钮 `取消` / `保存`（保存中 `保存中…`）（`:328-343`）。失败提示 `保存失败`（`:207-213`）；关闭按钮 aria-label `关闭对话框`。校验文案：`请输入词典名称。`、`词典名称不能超过 80 个字符。`、`请选择至少一个 .mdx 文件。`、`只支持 .mdx 文件：{name}。`、`该文件已被「{name}」使用：{file}。`、`优先级需为 1–9999 的整数。`（`:94-129`）。

**删除确认**：标题 `删除词典源？`，正文 `将移除「{name}」的配置。磁盘上的词典文件不会被删除。`，按钮 `取消` / `删除配置文件`（删除中 `删除中…`），失败 `删除失败：{message}`（`SourceList.tsx:515-542`）。

**首次引导**：标题 `三步完成初始配置`，说明 `每完成一步会自动打勾。也可以先跳过，之后在运行状态里查看。`；步骤 `01 添加词典`、`02 配置 Anki`、`03 设置热键`，状态 `已完成`/`未完成`/`检查中…`；动作按钮 `添加词典`、`打开设置`、`检查热键`；读取失败 `无法读取设置，引导中的完成状态可能不准确。`（`FirstRunGuide.tsx:120-145,159-213`）。

**查询测试**：`h1` = `查询测试`，说明 `使用当前配置做一次草稿查询：不写缓存，不弹出划词窗口，也不会写入 Anki。`（`QueryTester.tsx:191-196`）。字段 `查询范围`（select，首项 `全部启用源（{n}）`，其余为源名，`:211-227`）、`要查询的词`（placeholder `例如：serendipity`，`:230-254`）。按钮 `查询`（查询中 `查询中…`）、`清空`（`:246-253`）。空态：`输入词开始测试`；无启用源 `没有启用的词典源`；无结果 `没有查询到结果`；全失败 `查询失败` + `重试`（`:179-184,303-308,352-356,280-302`）。结果摘要 `“{word}” · {n} 条结果 · 最长耗时 {ms} ms`，每个源一块 `section` aria-label `{源名} 的查询结果`（`:311-313,326`）。

**运行状态**：`h1` = `运行状态`，说明 `热键、词典加载数与本地目录。`，按钮 `刷新`（`StatusView.tsx:68-75`）。行标签：`版本`、`全局热键`、`已就绪词典源`、`配置目录`、`日志目录`、`缓存文件`（`:94-135`）；热键状态 `已注册`/`未注册`（`:107`）；按钮 `打开配置目录`、`打开设置`（`:137-150`）。下方 `捕获测试`：按钮 `重新捕获`（`捕获中…`），指标 `方法`/`应用`/`耗时`/`锚点`，空提示 `还没有捕获记录。在任意应用选中文字后按热键。`，统计 `样本`/`P50`/`P95`/`成功率`（`CaptureTestPanel.tsx:50-103`）。

## 3. 设置窗口

**导航**（`SettingsApp.tsx:12-19,121-144`）：固定顺序 `通用`、`Anki`、`语境辅助`、`浏览器扩展`、`缓存`、`关于`；侧栏宽 200px，副标题 `设置`，aria-label `设置分区`。

**通用**（`GeneralSection.tsx`）：`h1` `通用`，说明 `热键、主题、查询策略与启动行为。`
- `全局热键`：热键录制按钮，显示当前加速键或 `尚未设置`，旁边 `点击后按下组合键，例如 Ctrl+Shift+D`；录制中按钮 `按下组合键…`、提示 `按 Esc 取消录制`；状态区 `至少包含一个修饰键。`；错误 `热键必须包含 Ctrl、Alt 或 Shift 修饰键。`（`HotkeyInput.tsx:114-129`）。
- `主题`（radiogroup）：`Figma（默认）`（`黑白核心与 pastel 色块。`）、`Apple`、`Cursor`（`GeneralSection.tsx:15-19`）；说明 `主题立即预览，点击保存后持久化；0.1.0 只提供浅色主题。`（`:196`）。
- `查询策略`：`本地优先`（默认）/`首个命中`/`聚合全部`（`:21-41`）。
- `每个源最多条数`（范围 1–10，默认 **3**）、`结果总条数`（范围 5–50，默认 **20**）、复选 `启动时最小化`（默认 **false**）（`config.rs:110-120`）。
- 保存：底部状态 `已保存` / `有未保存的更改`，按钮 `保存`（保存中 `保存中…`）（`:290-297`）。热键冲突 `HOTKEY_CONFLICT` 回滚为旧值并显示行内错误（`:119-123`）。

**Anki**（`AnkiSection.tsx`）：`h1` `Anki`。顶部探测区：加载 `正在探测 AnkiConnect…`；可用 `AnkiConnect 可用` + `v{version}`；失败 `Anki 未连接：{message}` 或 `探测失败：{message}`；按钮 `测试连接`（`:209-235`）。不可用时显示 `连接 Anki 的三步`（`:237-246`）。字段与默认值：`AnkiConnect 地址` = `http://localhost:8765`、`牌组` = `Default`、`笔记类型` = `Basic`、`标签` = `global-dict`（`config.rs:154-162`）；`字段映射` 四项标签 `单词`/`释义`/`原句`/`来源`，默认 `Word`/`Definition`/`Sentence`/`Source`（`config.rs:184-192`，`AnkiSection.tsx:23-28`）。`同语境重复` radiogroup：`提示后仍然保存`（默认）/`静默保存`（`:386-431`）。牌组/类型读取失败 `读取牌组/笔记类型失败：{message}` + `重试`（`:248-255`）。保存反馈同通用；字段缺失额外提示 `请确认四个字段都存在于所选笔记类型。`（`:441`）。

**关于**（`AboutSection.tsx:60-125`）：`h1` `关于`；行 `版本`/`配置目录`/`日志目录`/`缓存文件`；按钮 `打开配置目录`、`重新打开首次引导`；许可证块 `LexiGrab 以 AGPL-3.0 发布。…`。

**占位分区**（`PlaceholderSections.tsx:40-89`）：`语境辅助`、`浏览器扩展`、`缓存`；统一显示 `将在 {M3|M4} 提供` 与 `该分区暂不放控件，避免保存无效配置。研发进度按里程碑推进。`，无输入控件。

## 4. 弹窗窗口

阶段常量：`capturing|loading|results|empty|queryError|captureError|editing|saving|saved|saveError`（`src/windows/popup/state.ts:18-28`）。

| 阶段 | 可见标题/文案 | 按钮 |
| --- | --- | --- |
| capturing | Spinner `正在读取选区…`（`PopupApp.tsx:485`） | 头部关闭；底部菜单 |
| loading | Spinner `正在查询释义…`（`:487`） | 头部关闭；底部菜单 |
| captureError | `未捕获到文本`；默认文案 `未捕获到文本，请重新选择后再试。`（`:491,37-41`） | `重新捕获`、`打开词典管理`（`:493-495`） |
| queryError | `查询失败`；兜底 `词典没有返回结果，请稍后重试。`（`:501-502`） | `重试`、`打开词典管理` |
| empty | `未找到释义`；`当前词典中没有这个单词。可以补充释义后制卡，或先添加词典。`（`:511-513`） | `仍然制卡`、`添加词典`；底部 `关闭` |
| results | 义项列表 + 原句预览 | `编辑`、`保存所选义项`（`:581-585`）；菜单 `打开词典管理`/`设置` |
| saving | 横幅 `正在保存到 Anki…`（`:538`） | 禁用 `编辑`、`保存中…` |
| saved | `已保存到 Anki` + `note {id}`；若重复 `此语境之前保存过，本次已写入一张新卡片。`（`:542-551`） | `关闭`、`再存一张` |
| saveError | 错误消息；`ANKI_UNREACHABLE` 追加 `启动 Anki 并安装 AnkiConnect 后重试。`；`ANKI_FIELD_MISSING` 显示 `打开 Anki 设置`（`:185-206`） | `编辑`、`重试保存` |
| 同语境提示条 | `此语境已存过（note {id}），可换义项或仍然保存。`（`ContextWarningBar.tsx:28-31`） | `换义项`、`仍然保存` |

**头部**：`h1` = 当前单词（无词时 `LexiGrab`）+ 音标 + 来源徽标 + 关闭按钮（`PopupApp.tsx:627-649`）。**底部**：左 `⋯` 更多菜单，右为阶段按钮（`:662-668`）。

**结果卡片**（`ResultView.tsx:45-83`）：`ul[role=radiogroup]` 每项 `li > div[role=radio][aria-checked][tabIndex=0]`，内含 `headword`、`phonetic`、来源徽标、`DefinitionView` 渲染的 HTML（`button` 由词典 HTML 渲染，链接为 hyperlink，点击走系统浏览器：`DefinitionView.tsx:29-42`）。M2 中 `recommendedSenseId` 恒为 null，`语境推荐` 条不会出现（`ResultView.tsx:9-11,34-43`）。原句区：标签 `原句`，无原句时 `未捕获到原句，可在编辑模式补充。`（`PopupApp.tsx:172-183`）。

**编辑模式**（`EditView.tsx`）：头部 `编辑模式`；只读输入 `单词`、`来源`（hint `只读`）；tab `预览`/`源码`（提示 `保存内容以源码为准`）；预览空时 `释义为空，可切换到“源码”补充。`；源码字段 `释义源码（HTML）`；`原句` textarea（placeholder `未捕获到原句，可在此补充。`）；底部提示 `Ctrl+Enter 保存 · Esc 取消`，按钮 `取消`、`保存卡片`（保存中 `保存中…`）（`:78-200`）。

**键盘**：编辑模式 `Esc` 取消、`Ctrl/Cmd+Enter` 保存（`EditView.tsx:54-68`）；管理器模态 `Esc` 关闭 + Tab 焦点环（`Modal.tsx:41-65`）；热键录制 `Esc` 取消（`HotkeyInput.tsx:81-84`）；更多菜单 `Esc` 关闭（`PopupApp.tsx:75-79`）。快速模式**没有** Esc 关闭处理器，靠再次按全局热键隐藏（`PopupApp.tsx` 无 keydown；`docs/APP_FLOW.md:257`）。

**快速 vs 编辑**：窗口 420×560 → 560×640；快速模式 `WS_EX_NOACTIVATE` 不抢焦点、鼠标点击不激活，编辑模式清除该样式并 `SetForegroundWindow`（`src-tauri/src/platform/windows/popup.rs:145-164,227-326`）；关闭后恢复原前台窗口（`:339-351`）。

## 5. WebView2 UIA 提示

| Web 元素 | 预计 ControlType | 备注 |
| --- | --- | --- |
| 文本按钮（`Button`/原生 button） | Button | Name = 可见中文文本 |
| 图标按钮 | Button | Name = aria-label（如 `关闭弹窗`、`上移 X`） |
| `role=switch` | Button + TogglePattern | Name = `停用 X`/`启用 X`，ToggleState 反映启停 |
| input / textarea | Edit | 有 label 时 Name = label；也可用 AutomationId（DOM id） |
| select | ComboBox | 选项为 ListItem |
| `ul`/`li` | List / ListItem | 词典源列表、义项列表 |
| `role=radiogroup` + `role=radio` | Group / RadioButton | 主题、查询策略、义项选择 |
| `role=tablist`/`tab` | Tab / TabItem | 预览、源码 |
| `role=dialog` | Window 或 Pane | 视 WebView2 版本，可能挂在 document 下 |
| 词典 HTML 内容 | Document / Text / Hyperlink | DefinitionView 渲染，深层树可能很大 |
| `role=alert` / `role=status` | Text（LiveRegion） | 错误、保存状态 |

若 `child_window` 找不到，先 `print_control_identifiers(depth=6)`；内容通常在 `Chrome_WidgetWin_1` / `Chrome Legacy Window` / `WebView2` 子树里，根源为 Document。

**Tab 顺序**（DOM 顺序，无 `tabindex` 干扰）：
- 管理器：3 个导航 → `添加源` → 引导动作按钮 → 源行（上移/下移/开关/编辑/删除）→ 空态按钮。对话框：`选择 .mdx 文件` → 各 `移除` → 名称 → 优先级 → 启用 → 取消 → 保存；删除对话框：取消 → 删除配置文件。
- 设置：6 个分区 → 热键录制 → 3 个主题 radio → 3 个策略 radio → 每源条数 → 总条数 → 启动最小化 → 保存。Anki：`测试连接` → 地址 → 牌组 → 笔记类型 → 4 个映射 → 标签 → 2 个重复策略 radio → 保存。
- 弹窗快速：关闭 → 义项 radio（全部 `tabIndex=0`）→ `⋯` → `编辑` → 保存。编辑模式：单词（只读）→ 来源（只读）→ 预览/源码 tab → 源码 textarea（仅源码 tab）→ 原句 textarea → 取消 → 保存卡片。

## 6. pywinauto 选择器策略

**窗口定位**：`Desktop(backend="uia")`。`LexiGrab 设置` 标题唯一；manager 与 popup 同为 `LexiGrab`，用尺寸或子元素区分：
```python
desk = Desktop(backend="uia")
wins = desk.windows(title="LexiGrab")                 # manager + popup
manager = max(wins, key=lambda w: w.rectangle().width())   # 980 vs 420
popup = min(wins, key=lambda w: w.rectangle().width())
```
更稳的替代：先用 `win32gui.EnumWindows` 取 HWND，再用 `Desktop(backend="uia").window(handle=hwnd)`；或按可见子文本 `词典管理器` / `编辑模式` 判定。隐藏窗口不会出现在桌面 UIA 树中，需先经热键（默认 `Ctrl+Shift+D`）或 `show_window` 显示。

**每窗口首选 → 回退**：
1. manager：`window(title_re="LexiGrab")` + 宽度过滤 → `child_window(title="添加源", control_type="Button")`；源行操作用动态 aria-label：`title_re="停用 .*"`、`上移 .*`、`下移 .*`；输入用 AutomationId `source-name` 等。
2. settings：`window(title="LexiGrab 设置")` → `child_window(title="通用", control_type="Button")` 等分区按钮；字段按 `title="AnkiConnect 地址"` / AutomationId `anki-endpoint`；保存按钮 `title="保存"`。
3. popup：`window(title="LexiGrab")` 宽度 ~420×scale → `child_window(title="保存所选义项", control_type="Button")`；义项用 `control_type="RadioButton"` 的 Name `{word} · {来源}`；编辑模式按钮 `编辑`、`保存卡片`、`取消`。

**回退顺序**：UIA Name/ControlType → AutomationId → 键盘（`type_keys("{TAB}")` 按上表顺序 + `{ENTER}`）→ 截图坐标。快速模式为 `WS_EX_NOACTIVATE` 窗口，`click_input()` 用真实鼠标点击可行，但不要期待窗口获得焦点。

**坐标回退（仅固定布局可靠）**：popup 快速模式 420×560（DPI 缩放后按比例），头部 44px、底部 56px：
- 关闭：`(0.91, 0.04)`；`⋯`：`(0.05, 0.95)`；右对齐主按钮：约 `(0.80, 0.95)`，次按钮约 `(0.60, 0.95)`。
- 编辑模式 560×640：关闭按钮不存在，`取消` ≈ `(0.80, 0.96)`、`保存卡片` ≈ `(0.92, 0.96)`；头部/底部高度比例不变。
- manager 980×700（默认尺寸）：侧栏导航中心 x≈0.11，y≈0.17/0.24/0.31；`添加源` ≈ `(0.91, 0.07)`。settings 900×700：侧栏导航 x≈0.11，y 依次约 0.16–0.45；主区从 x≈0.27 开始。manager/settings 可调整大小，坐标仅在默认尺寸有效。

**排错提示**：若按钮没有 Name，检查 aria-label 是否被 WebView2 露出；`role=switch` 可能表现为 Button 而非 CheckBox；模态遮罩会拦截点击，先关闭对话框；保存后弹窗不自动关闭（`docs/APP_FLOW.md:278`），断言用 `已保存到 Anki` 文本而非窗口消失。
