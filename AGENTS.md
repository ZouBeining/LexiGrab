# AGENTS.md

操作约束。产品规格见 [PROMPT.md](PROMPT.md)。技术栈：Tauri 2 + Rust + React 19 + TypeScript + Tailwind 4；Firefox 扩展用 WXT。

## 会话启动

1. 先读 [PROGRESS.md](.agents/plans/PROGRESS.md)，从断点继续；再读当前阶段的 [PLAN_Mx](.agents/plans/)。
2. 产品范围见 [PRD.md](docs/PRD.md)，页面见 [APP_FLOW.md](docs/APP_FLOW.md)，接口见 [BACKEND_STRUCTURE.md](docs/BACKEND_STRUCTURE.md)，版本见 [TECH_STACK.md](docs/TECH_STACK.md)。
3. UI 主参考是 [figma.md](.agents/design/figma.md)：布局、断点、组件按它；Apple/Cursor 只做配色主题，规则见 [FRONTEND_GUIDELINES.md](docs/FRONTEND_GUIDELINES.md)。
4. 不得复制 docs 已有契约到计划；计划只链接。修改契约先改 docs，再改实现。

## 仓库落位

Rust 进 `src-tauri/src/{core,sources,parsers,anki,platform,bridge}`；前端进 `src/windows/{popup,manager,settings}` 与 `src/lib`；扩展进 `extension/entrypoints`。不得在根目录随意新建目录。

## 前端

- 只用 Tailwind 工具类，不写内联 `style`，不引入组件库/图标包。
- IPC 类型只从 `src/lib` 导入；组件不直接 `invoke`。
- 主题只换 CSS 变量，不换组件结构；默认 `figma`，0.1.0 为浅色三主题。
- 所有页面必须有 loading、empty、error 三态。

## 常用命令

- 安装：`pnpm install`
- 桌面开发：`pnpm tauri dev`
- 扩展开发：`pnpm -C extension wxt -b firefox`
- Rust 测试：`cd src-tauri && cargo test`
- 前端检查：`pnpm typecheck && pnpm lint`
- 扩展打包：`pnpm -C extension wxt build -b firefox && pnpm -C extension web-ext build`

提交前必须通过 `cargo test` 与 `pnpm typecheck && pnpm lint`。

## 包管理与版本控制

前端与扩展统一用 pnpm；加依赖用 `pnpm add -D <pkg>`，根目录加 `-w`。不要混用 npm/yarn，不提交非 pnpm 锁文件。`Cargo.lock` 必须提交。不提交 `.env`、`node_modules/`、`target/`、`.wxt/`、`.output/`、`dist/`；不手改生成目录。`references/*` 只有 `README.md` 入库。

## 编码细则

- Rust 非测试代码禁用 `unwrap()` / `expect()`；TypeScript 禁用 `any`。
- 前端与扩展不得直接调用 AnkiConnect 或远程词典，一律经 Tauri Command 或本地桥接。
- `FormatParser::lookup(&self)` 需要缓存时用内部可变性，不改 trait 签名。
- `Entry.source_name` 由 RemoteHttpSource / LocalFileSource 填写，解析器不返回。
- 语境辅助只做建议：JEV-like 结果高亮但不静默替换词典；AI 释义必须标注模型并由用户确认。
- 同一单词允许多张卡片；Anki 请求 `allowDuplicate: true`，应用层只提示“同词同句同释义”。
- 远程失败不得抛阻断错误或阻塞 UI；本地桥接不得绑定 `0.0.0.0`，不得省略 `X-Auth-Token`。
- `references/ODH`（MIT）只研究句子提取、词典脚本与 Anki 封装，不复制代码。

## 提交与禁止

Conventional Commits；一个 commit 只动一个模块，Rust、前端、扩展、文档分开。禁止偏离 PROMPT.md 的技术栈或 AGPL-3.0；禁止不兼容 copyleft 依赖；禁止整段复制设计文档。

## 活文档

用户纠正一次就更新本文件，保持 ≤60 行；功能完成同步更新 PROGRESS.md 与相关 docs。
