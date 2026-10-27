# AGENTS.md

本文件是 AI 编码代理的操作性约束。设计目标、架构、API 契约、里程碑见 [PROMPT.md](PROMPT.md)，此处不重复。

## 仓库落位

新增代码必须落入既有层级：Rust 侧 `src-tauri/src/{core,sources,parsers,anki,platform,bridge}`；前端 `src/windows/{popup,manager,settings}` 与 `src/lib`；扩展 `extension/entrypoints`。不得在根目录随意新建目录。

## 前端

前端设计须参考 [DESIGN.md](DESIGN.md)。

## 常用命令

- 安装依赖：`pnpm install`
- 桌面开发：`pnpm tauri dev`
- 扩展开发：`cd extension && pnpm wxt -b firefox`
- Rust 测试：`cd src-tauri && cargo test`
- 前端检查：`pnpm typecheck && pnpm lint`
- 打包扩展：`cd extension && pnpm wxt build -b firefox && web-ext build`

提交前必须通过 `cargo test` 与 `pnpm typecheck && pnpm lint`。

## 包管理

前端与扩展统一使用 **pnpm**。添加开发依赖用 `pnpm add -D <pkg>`；workspace 根目录加 `-w`。不要混用 npm/yarn，不要提交非 pnpm 的锁文件。

## 版本控制

`Cargo.lock` 必须提交。不要提交 `.env`、`node_modules/`、`target/`、`.wxt/`、`.output/`、`dist/`。不要手动编辑 `src-tauri/gen/`、`.wxt/`、`.output/` 等生成目录。

## 编码细则

- Rust 非测试代码禁用 `unwrap()` / `expect()`。
- TypeScript 禁用 `any`；IPC 边界类型只从 `src/lib` 导入。
- 样式只用 Tailwind 工具类，不写内联 `style`。
- 前端与扩展不得直接调用 AnkiConnect 或远程词典，一律经 Tauri Command 或本地桥接。
- `FormatParser::lookup(&self)` 如需缓存，用 `Mutex`/`RwLock` 内部可变性，不改 trait 签名。
- `Entry.source_name` 由 `RemoteHttpSource` 统一填充，解析器不返回该字段。

## 提交

Conventional Commits；一个 commit 只动一个模块，Rust 与前端改动分开提交。

## 禁止

- 不得偏离 `PROMPT.md` 已确定的技术栈与许可证（AGPL-3.0）。
- 引入新依赖前检查许可证兼容性，不引入不兼容 copyleft 依赖。
- 不得把本地 API 绑定到 `0.0.0.0` 或省略 `X-Auth-Token` 校验。
- 不得在远程源失败时抛出阻断性错误或阻塞 UI。
