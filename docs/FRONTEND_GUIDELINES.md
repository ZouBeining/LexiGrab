# FRONTEND_GUIDELINES

本文件补充 LexiGrab 的设计系统与实现约定，不复制设计分析原文。

## 1. 参考层级

用户已确认：**Figma 是 UI 设计的主参考**。

| 参考 | 用途 | 链接 |
| --- | --- | --- |
| Figma | 主参考：布局、间距、断点、组件形状、按钮语法、色块节奏 | [figma.md](../.agents/design/figma.md) |
| Apple | 仅配色：蓝色强调、parchment 画布，作为可替换主题 | [apple.md](../.agents/design/apple.md) |
| Cursor | 仅配色：橙色强调、暖白画布，作为可替换主题 | [cursor.md](../.agents/design/cursor.md) |

规则：

- 组件结构、页面网格、响应式行为、圆角语法、按钮形状一律按 Figma。
- Apple 与 Cursor 不改变布局、字号比例、组件形态，只替换颜色 token。
- Figma 原文未定义的交互状态（桌面端 hover、focus、disabled、错误）由本文件补充。
- Figma 的营销页断点不直接套到桌面窗口。窗口宽度固定，内部栅格在 720px 处单列化，详见第 4 节。

## 2. 主题架构

主题只替换 CSS 变量，不替换组件。使用 `data-theme` 挂在 `<html>`：

```html
<html data-theme="figma">
```

0.1.0 提供三个浅色主题：

| 主题值 | 名称 | 颜色来源 | 适用 |
| --- | --- | --- | --- |
| `figma` | Figma（默认） | Figma 的黑白核心与 pastel 色块 | 默认 |
| `apple` | Apple | Apple 的 Action Blue 与 parchment | 偏好冷色 |
| `cursor` | Cursor | Cursor 的橙色与暖奶油 | 偏好暖色 |

0.1.0 不做深色主题。Figma 参考本身没有深色模式，强行派生会偏离主参考。后续如需深色，单独设计并走同样的 token 契约。

### 2.1 语义 token

应用只使用以下语义变量，组件不得直接写死色值：

```css
:root,
[data-theme="figma"] {
  --lg-canvas: ...;
  --lg-canvas-soft: ...;
  --lg-ink: ...;
  --lg-ink-muted: ...;
  --lg-hairline: ...;
  --lg-accent: ...;
  --lg-on-accent: ...;
  --lg-block-1: ...;
  --lg-block-2: ...;
  --lg-block-3: ...;
  --lg-success: ...;
  --lg-danger: ...;
  --lg-warning: ...;
  --lg-overlay: ...;
}
```

映射原则：

- `figma`：直接引用 [figma.md](../.agents/design/figma.md) 的 `colors.canvas`、`colors.ink`、`colors.hairline`、`colors.primary`、`colors.block-*` 与 `colors.semantic-success`。`danger` 与 `warning` 取 Cursor 的语义色，避免自造品牌色。
- `apple`：引用 [apple.md](../.agents/design/apple.md) 的 `colors.canvas`、`colors.canvas-parchment`、`colors.ink`、`colors.hairline`、`colors.primary`。pastel 色块退化为 parchment 与 surface-pearl。
- `cursor`：引用 [cursor.md](../.agents/design/cursor.md) 的 `colors.canvas`、`colors.canvas-soft`、`colors.ink`、`colors.hairline`、`colors.primary`、`colors.semantic-*`。pastel 色块退化为 canvas-soft 与 surface-card。

具体 hex 在实现时从对应设计文档的 front matter 读取，写进 Tailwind `@theme`，本文件不复制整张色板。

### 2.2 Tailwind 接入

```css
@import "tailwindcss";

@theme {
  --color-canvas: var(--lg-canvas);
  --color-canvas-soft: var(--lg-canvas-soft);
  --color-ink: var(--lg-ink);
  --color-ink-muted: var(--lg-ink-muted);
  --color-hairline: var(--lg-hairline);
  --color-accent: var(--lg-accent);
  --color-on-accent: var(--lg-on-accent);
  --color-block-1: var(--lg-block-1);
  --color-block-2: var(--lg-block-2);
  --color-block-3: var(--lg-block-3);
  --color-success: var(--lg-success);
  --color-danger: var(--lg-danger);
  --color-warning: var(--lg-warning);
}
```

使用 `bg-canvas`、`text-ink`、`border-hairline`、`bg-accent` 等工具类。禁止 `style={{ color: "#..." }}`、禁止任意值 hex 类（如 `bg-[#dceeb1]`）。主题切换只改 `data-theme`，不改组件。

## 3. 字体与字号

Figma 的 `figmaSans` / `figmaMono` 是专有字体，不打包。使用：

```css
--font-sans: Inter, "SF Pro Text", system-ui, -apple-system, "Segoe UI", sans-serif;
--font-mono: "JetBrains Mono", "SF Mono", ui-monospace, monospace;
```

字体未安装时回退系统字体，不下载远程字体。字号按桌面工具的使用距离收敛，不照搬营销页的 86px 显示字：

| 角色 | 字号/行高 | 字重 | 用途 |
| --- | --- | --- | --- |
| 弹窗词条 | 24 / 1.25 | 700 | 当前查询词 |
| 页面标题 | 26 / 1.35 | 540 | 管理器/设置分区标题 |
| 强调正文 | 16 / 1.45 | 540 | 卡片标题、按钮 |
| 正文 | 14 / 1.5 | 400 | 表单、说明 |
| 辅助 | 12 / 1.4 | 400 | 状态、来源、时间 |
| 等宽标签 | 12 / 1.3 | 400，字距 0.6px | 来源、快捷键、错误码 |

Figma 的负字距只用于大标题（26px 以上），正文不收紧字距。Figma mono 只用于标签，不用于段落。

## 4. 布局与窗口

### 4.1 固定窗口

| 窗口 | 尺寸 | 最小尺寸 | 说明 |
| --- | --- | --- | --- |
| manager | 980×700 | 900×600 | 侧栏 220 + 内容区 |
| popup | 420×560 | 固定 | 快速模式 |
| popup 编辑 | 560×640 | 固定 | 编辑模式 |
| settings | 900×700 | 860×640 | 侧栏 200 + 表单 |

桌面窗口不做移动导航。小于 720px 的内容区（仅缩小时可能发生）将双列卡片改为单列，侧栏折叠成顶部下拉。Figma 的 960/768/560 断点作为内部栅格参考，不作为窗口断点。

### 4.2 管理器

- 左侧深色/浅色导航 220px，顶部产品名，下面三个入口：词典源、查询测试、运行状态。
- 右侧内容区最大 760px，左对齐，不居中成大空白。
- 源列表用行，不用大卡片。行高 64px，底部 hairline。
- 空态可用一块 pastel 色块作为引导背景，这是 Figma 色块语法在工具里的唯一用途。

### 4.3 弹窗

- 白色画布，12px 外圆角，1px hairline。
- 顶部 44px，底部 56px，内容区自适应滚动。
- 释义区最大高度 300px，图片等比缩放，音频控件全宽。
- 来源徽标用等宽小字，不加彩色背景。
- 远程进度条为 2px 细线，不使用 spinner 遮挡内容。
- 编辑模式不改变布局语言，只增加输入区与 tab。

### 4.4 设置

- 侧栏 200px，五个分区。
- 表单标签在上，控件在下，错误紧贴控件。
- 每组之间 32px，组内 16px。
- 保存按钮固定在分区底部，不随滚动消失。

## 5. 组件规则

### 5.1 按钮

- 主按钮：pill，Figma 的 `primary` 背景与 `on-primary` 文本，最小高度 40px，内边距 10×20。
- 次按钮：透明或 canvas 背景，1px hairline 边框，pill。
- 文字按钮：无边框，使用 accent 文本，命中区域不小于 40×40。
- 危险操作：文字用 danger，不使用红色大按钮；删除必须二次确认。
- 所有按钮的按下态用 `scale(0.98)` 或背景微调，不使用阴影。
- disabled 用 48% 透明度并禁止点击，不只改颜色。

### 5.2 输入

- 8px 圆角，1px hairline，白底，14px 正文。
- focus 用 2px accent 外环，偏移 1px。
- 错误用 danger 边框与 12px 文案，`aria-describedby` 关联。
- textarea 最小 88px，可垂直拉伸，不横向溢出。
- 密钥输入默认为密码类型，提供显示切换。

### 5.3 卡片与行

- 卡片 24px 圆角，1px hairline，无阴影。
- 列表行不用卡片包裹，避免嵌套卡片。
- 选中项用左侧 2px accent 竖线或浅色底，不改变尺寸。
- 状态徽标用文字 + 小圆点，不能只靠颜色。

### 5.4 色块

Figma 的 pastel 色块只用于：

- 首次启动引导。
- 空态引导。
- 关于页的许可证说明块。

同一视图最多一个色块。色块内文字仍用 ink，不使用反色，除非使用 navy 变体。

### 5.5 浮层与提示

- toast 出现在窗口右下角，3 秒消失，不遮挡主按钮。
- 对话框宽 420，24px 圆角，遮罩用 60% 黑。
- 不使用自定义 tooltip 承载关键信息。
- 加载骨架用 `canvas-soft` 与 hairline，不用闪烁动画。

## 6. 桌面交互

- hover 只做轻微背景变化，不能是唯一反馈。
- focus ring 必须在键盘导航时可见。
- 弹窗快速模式不抢焦点；按钮点击不激活窗口。
- 编辑模式激活后，Esc 退出，焦点回到触发前窗口。
- 滚动条使用系统样式，不隐藏。
- 文本可选择，释义中的链接点击后交系统浏览器打开。
- 高 DPI 下使用逻辑像素布局，图片按容器缩放。

## 7. 动效

- 只使用 120–180ms 的透明度与位移过渡。
- 不使用弹跳、缩放超过 1.02 的动画。
- 尊重 `prefers-reduced-motion`，关闭非必要过渡。
- 弹窗出现不做从 0 缩放，避免在无激活窗口上产生焦点异常。

## 8. 无障碍

- 正文与背景对比度 ≥ 4.5:1，辅助文字 ≥ 4.5:1。
- 所有图标按钮有 `aria-label`。
- 表单控件有可见 label。
- 错误状态同时有文字与图标。
- 对话框打开时焦点进入第一个可操作元素，关闭时回到触发元素。
- 键盘可达顺序与视觉顺序一致。

## 9. 文案

- 使用动词开头的短句，例如“添加词典”“保存到 Anki”。
- 错误文案说明发生了什么与下一步，例如“Anki 未运行，启动后重试”。
- 不显示原始堆栈、错误码长串或英文异常。
- 同一个概念只用一种叫法：词典源、词条、释义、原句、牌组、笔记类型。
- 不写营销口号，不写拟人化提示。

## 10. 实现约束

- 只使用 Tailwind 工具类，不写内联 `style`。
- 组件按窗口分目录：`src/windows/{popup,manager,settings}`；共享组件放 `src/components`。
- IPC 类型只从 `src/lib` 导入。
- 不引入组件库、图标包、CSS-in-JS、路由库。
- 主题值集中在 `src/lib/theme.ts`，窗口初始化时设置 `data-theme`。
- 所有弹窗、表单、列表必须有 loading、empty、error 三态。
- 新增组件先检查 Figma 参考中是否有对应 token，没有再用语义变量组合，不自造 hex。

## 11. 验收

- 三套主题切换后布局不位移，对比度达标。
- Figma 默认主题下不出现第二强调色。
- 弹窗、管理器、设置在 100% 与 150% 缩放下无溢出与截断。
- 键盘可完成添加源、保存设置、编辑并保存卡片。
- 没有内联 style，没有任意值 hex 类，lint 通过。
