# M5 发布准备记录

日期：2026-11-21。版本 `0.1.0`。范围与出口项见 [PLAN_M5](../plans/PLAN_M5.md)。

## 1. 已产出产物

| 产物 | 路径 | 大小 | SHA256 |
| --- | --- | --- | --- |
| Windows release 可执行文件 | `src-tauri/target/release/lexigrab.exe` | 13,841,920 B | `d872f7646061aa8819336b577af6c2b3aee5d3a2b8674ce55648e89258d082af` |
| Firefox 未签名扩展 | `extension/dist/lexigrab-firefox-0.1.0.xpi` | 167,973 B | `75f6d04ad95225a36c39259c686b4df2cff1d2197b7496ba0690fcd0c98aa9f8` |

产物在 `.gitignore` 覆盖的构建目录中，不提交。

## 2. NSIS 安装包未在本地生成

`pnpm tauri build --bundles nsis` 完成 release 编译后，在打包阶段失败：

```text
Downloading https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.13/nsis-3.13.zip
failed to bundle project: `timeout: global`
```

手动 `curl` 同一 URL 也在 24 秒后 `Recv failure: Connection was reset`，与本机
访问 GitHub release 二进制的既有网络限制一致（见 PROGRESS 的 Schannel 记录）。
因此本地只有 release exe，没有 NSIS 安装包；安装包由
[.github/workflows/release.yml](../../.github/workflows/release.yml) 在能访问 GitHub
的 CI runner 上构建，发布前必须实跑一次并核对产物名与 SHA256。

## 3. 门禁结果

| 门禁 | 结果 |
| --- | --- |
| `cargo fmt --check` | 通过 |
| `cargo clippy --all-targets -- -D warnings` | 通过 |
| `cargo test` | 通过（328 通过 / 2 个性能探针默认忽略） |
| `cargo deny check licenses bans sources` | `bans ok, licenses ok, sources ok` |
| `pnpm typecheck && pnpm lint && pnpm test` | 通过（16 文件 140 测试） |
| `pnpm build` | 通过（tauri build 前置） |
| `pnpm -C extension typecheck && test` | 通过（42 测试） |
| `pnpm -C extension build:firefox && lint:firefox` | 0 error / 7 warning，exit 0 |
| `pnpm -C extension package:firefox` | 产出上表 `.xpi` |
| 版本五处一致 | `0.1.0`（`package.json`、`Cargo.toml`、`tauri.conf.json`、`extension/package.json`、`wxt.config.ts`） |
| 性能 | 见 [performance](../../docs/performance.md) |
| 安全 | 见 [security](../../docs/security.md) |

## 4. 发布前仍需执行

1. 在目标机器跑 8 小时长跑（`.agents/automation/m5-soak/soak.py --minutes 480`），把结果补进 performance。
2. 干净 Windows 虚拟机按 [用户指南](../../docs/user-guide.md) 与 PRD 成功标准逐项验收。
3. 打 `v0.1.0` tag（或手动触发 release 工作流），确认 NSIS 安装包、`.xpi`、`SHA256SUMS` 与草稿 release。
4. 从干净克隆按 README 构建一次（`pnpm install` → `pnpm tauri build`），确认无隐藏本地依赖。
